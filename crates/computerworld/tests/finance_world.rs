//! Money on the simulated rails, end to end in robotics-co through the agent API, with the
//! world clock doing the waiting: Halyard runs payroll in QuickBooks and the direct
//! deposits reach Northwind and Pinecrest accounts the next business day; a customer pays
//! an invoice by ACH from Pinecrest and the bank feed brings it into the books; a card
//! Pinecrest issued buys from Halyard's Stripe, is authorized by the issuer over the card
//! network, clears at 18:00 and is paid out to Northwind two business days on; a
//! chargeback answered with evidence is won; and an R01 return is booked from the feed.
use computerworld::World;
use cw_protocol::{ActionEnvelope, EnvironmentConfig, WorldDefinition};
use serde_json::{json, Value};

const SECOND: u64 = 1_000_000;
const HOUR: u64 = 3_600 * SECOND;
const DAY: u64 = 24 * HOUR;

struct Halyard {
    world: World,
    sessions: Vec<(String, String, String)>,
}
impl Halyard {
    fn new() -> Self {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../worlds/robotics-co/world.json"
        ))
        .unwrap();
        let mut world = World::new(WorldDefinition::from_json(&text).unwrap(), 7).unwrap();
        let mut sessions = vec![];
        for (actor, machine) in [
            ("omar", "omar-win"),
            ("rosa", "rosa-mac"),
            ("nina", "nina-win"),
            ("casey", "casey-pc"),
        ] {
            let id = world
                .environment(EnvironmentConfig {
                    actor: actor.into(),
                    machines: vec![machine.into()],
                    actions: vec!["browser.v1".into()],
                    observations: vec!["semantic.v1".into(), "browser.v1".into()],
                    action_budget: 1 << 20,
                })
                .unwrap();
            sessions.push((actor.to_owned(), machine.to_owned(), id));
        }
        Halyard { world, sessions }
    }
    fn act(&mut self, actor: &str, op: &str, payload: Value) {
        let (_, machine, session) = self
            .sessions
            .iter()
            .find(|(a, _, _)| a == actor)
            .unwrap()
            .clone();
        let r = self
            .world
            .step(
                &session,
                vec![ActionEnvelope::new(
                    "browser.v1",
                    op,
                    &machine,
                    payload.clone(),
                )],
            )
            .unwrap();
        assert!(
            r.outcomes[0].success,
            "{actor} {op} {payload}: {:?}",
            r.outcomes[0]
        );
    }
    fn form(&mut self, actor: &str, url: &str, fields: &[(&str, &str)], form: &str) {
        self.act(actor, "navigate", json!({"url": url}));
        for (id, value) in fields {
            self.act(actor, "fill", json!({"id": id, "value": value}));
        }
        self.act(actor, "submit", json!({"id": form}));
        self.world.advance(SECOND).unwrap();
    }
    fn state(&self, service: &str) -> Value {
        self.world.inspect()["state"]["services"][service]["state"].clone()
    }
    /// Let the world clock run until `tick`.
    fn until(&mut self, tick: u64) {
        let now = self.world.tick();
        if tick > now {
            self.world.advance(tick - now).unwrap();
        }
    }
    fn books(&self) -> Value {
        self.state("quickbooks")["companies"]["halyard"].clone()
    }
}
/// `day` days after the epoch's date, at `hh:mm`.
fn at(day: u64, hh: u64, mm: u64) -> u64 {
    day * DAY + (hh * 60 + mm) * 60 * SECOND - 9 * HOUR
}

#[test]
fn payroll_reaches_employees_banks_the_next_business_day() {
    let mut h = Halyard::new();
    let before = h.state("northwind")["accounts"].clone();
    h.form(
        "omar",
        "qbo.intuit.com/payroll",
        &[
            ("run-start", "2026-09-16"),
            ("run-end", "2026-09-30"),
            ("run-pay", "2026-09-30"),
            ("run-hours-luis", "84"),
            ("run-hours-priya", "80"),
        ],
        "run",
    );
    let run = h.books()["payroll"]["PR-0004"].clone();
    assert_eq!(run["status"], "draft", "{run}");
    let net = run["net_cents"].as_i64().unwrap();
    // The preparer cannot approve; the CTO can.
    h.form("rosa", "qbo.intuit.com/payroll/PR-0004", &[], "approve");
    let run = h.books()["payroll"]["PR-0004"].clone();
    assert_eq!(run["status"], "approved");
    assert_eq!(run["approved_by"], "rosa");
    assert_eq!(run["effective_date"], "2026-09-18", "{run}");
    let nw = h.state("northwind");
    let op = &nw["accounts"]["halyard-operating"];
    assert_eq!(
        op["available_cents"].as_i64().unwrap(),
        op["balance_cents"].as_i64().unwrap() - net,
        "held until it settles"
    );
    // The 10:00 cut-off sends the file; the operator settles it Friday 08:30.
    h.until(at(1, 8, 31));
    let nw = h.state("northwind");
    let pc = h.state("pinecrest");
    assert_eq!(
        nw["accounts"]["halyard-operating"]["balance_cents"]
            .as_i64()
            .unwrap(),
        before["halyard-operating"]["balance_cents"]
            .as_i64()
            .unwrap()
            - net
    );
    let stub = |id: &str| {
        run["stubs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["employee"] == id)
            .unwrap()["net_cents"]
            .as_i64()
            .unwrap()
    };
    assert_eq!(
        nw["accounts"]["nw-rosa"]["balance_cents"].as_i64().unwrap(),
        before["nw-rosa"]["balance_cents"].as_i64().unwrap() + stub("rosa")
    );
    assert_eq!(
        pc["accounts"]["pc-luis"]["balance_cents"].as_i64().unwrap(),
        412_000 + stub("luis")
    );
    let fed = h.state("fedline");
    assert_eq!(
        fed["positions"]["125000105"].as_i64().unwrap(),
        -(net
            - ["rosa", "maya", "dev", "sam"]
                .iter()
                .map(|e| stub(e))
                .sum::<i64>())
    );
}

#[test]
fn an_invoice_paid_by_ach_from_another_bank_lands_in_the_books_through_the_feed() {
    let mut h = Halyard::new();
    h.form(
        "nina",
        "qbo.intuit.com/invoices",
        &[
            ("inv-customer", "northgate"),
            ("inv-l1-desc", "Halyard Rover R1"),
            ("inv-l1-qty", "2"),
            ("inv-l1-rate", "8,950"),
            ("inv-l1-taxable", "yes"),
        ],
        "invoice",
    );
    let inv = h.books()["invoices"]["INV-1045"].clone();
    assert_eq!(inv["total_cents"], 1_919_775, "{inv}");
    assert_eq!(inv["due"], "2026-10-02");
    // QuickBooks mailed it from nina's Gmail with the remittance details.
    let mail = h.state("google-mail");
    let sent = mail["messages"]
        .as_object()
        .unwrap()
        .values()
        .find(|m| m["subject"] == "Invoice INV-1045 from Halyard Robotics")
        .unwrap()
        .clone();
    assert_eq!(sent["sender"], "nina");
    assert!(
        sent["body"]
            .as_str()
            .unwrap()
            .contains("routing 125000105, account 5500402210"),
        "{sent}"
    );
    // Casey pays from Northgate's operating account at Pinecrest; it leaves at the 10:00
    // cut-off and settles Friday 08:30.
    h.form(
        "casey",
        "pinecrest.example/moves",
        &[
            ("ext-account", "pc-northgate-operating"),
            ("ext-routing", "125000105"),
            ("ext-number", "5500402210"),
            ("ext-name", "Halyard Robotics"),
            ("ext-amount", "19,197.75"),
            ("ext-memo", "INV-1045"),
        ],
        "ext",
    );
    let entry = h.state("pinecrest")["ach"]["ach-1"].clone();
    assert_eq!(entry["status"], "scheduled", "{entry}");
    assert_eq!(entry["entry"]["amount_cents"], 1_919_775);
    let before = h.state("northwind")["accounts"]["halyard-operating"]["balance_cents"]
        .as_i64()
        .unwrap();
    h.until(at(1, 8, 31));
    assert_eq!(h.state("pinecrest")["ach"]["ach-1"]["status"], "settled");
    assert_eq!(
        h.state("northwind")["accounts"]["halyard-operating"]["balance_cents"]
            .as_i64()
            .unwrap(),
        before + 1_919_775
    );
    // Omar updates the bank feed and matches the deposit to the invoice.
    h.form("omar", "qbo.intuit.com/banking", &[], "sync-1000");
    let feed = h.books()["feed"].clone();
    let line = feed
        .as_object()
        .unwrap()
        .values()
        .find(|f| {
            f["description"] == "ACH CREDIT NORTHGATE FULFILLMENT" && f["date"] == "2026-09-18"
        })
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    h.form(
        "omar",
        "qbo.intuit.com/banking",
        &[(&format!("match-{line}-target"), "INV-1045")],
        &format!("match-{line}"),
    );
    let b = h.books();
    assert_eq!(
        b["invoices"]["INV-1045"]["status"], "paid",
        "{}",
        b["activity"]
    );
    assert_eq!(b["feed"][&line]["status"], "matched");
}

#[test]
fn a_card_payment_clears_settles_and_pays_out_and_a_chargeback_is_won_on_evidence() {
    let mut h = Halyard::new();
    // Nina makes a payment link for the charging dock; Casey buys three with Northgate's
    // Pinecrest card, which the issuer authorizes over the network.
    h.act(
        "nina",
        "navigate",
        json!({"url": "dashboard.stripe.com/products/prod-3"}),
    );
    h.act("nina", "submit", json!({"id": "create-link"}));
    h.world.advance(SECOND).unwrap();
    let code = h.state("stripe-dashboard")["links"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    h.form(
        "casey",
        &format!("buy.stripe.com/{code}"),
        &[
            ("pay-email", "casey.brooks@northgate-fulfillment.example"),
            ("pay-card", "4519 0700 9900 1124"),
            ("pay-exp", "11 / 29"),
            ("pay-cvc", "604"),
            ("pay-name", "Casey Brooks"),
            ("pay-qty", "3"),
        ],
        "pay",
    );
    let p = h.state("stripe-dashboard")["payments"]["pay-6"].clone();
    assert_eq!(p["status"], "succeeded", "{p}");
    assert_eq!(p["amount_cents"], 375_000);
    let pc = h.state("pinecrest");
    assert_eq!(
        pc["accounts"]["pc-cc-casey"]["available_cents"],
        2_500_000 - 375_000,
        "held by the issuer"
    );
    assert_eq!(pc["accounts"]["pc-cc-casey"]["balance_cents"], 0);
    // A wrong security code is declined by the issuer, with its code.
    h.form(
        "casey",
        &format!("buy.stripe.com/{code}"),
        &[
            ("pay-email", "casey.brooks@northgate-fulfillment.example"),
            ("pay-card", "4519 0700 9900 1124"),
            ("pay-exp", "11 / 29"),
            ("pay-cvc", "999"),
            ("pay-name", "Casey Brooks"),
        ],
        "pay",
    );
    let p = h.state("stripe-dashboard")["payments"]["pay-7"].clone();
    assert_eq!(
        (p["status"].as_str(), p["decline_code"].as_str()),
        (Some("failed"), Some("N7")),
        "{p}"
    );
    // The 18:00 cut-off clears it to the issuer and settles it to Stripe net of interchange.
    h.until(at(0, 18, 1));
    assert_eq!(
        h.state("pinecrest")["accounts"]["pc-cc-casey"]["balance_cents"],
        -375_000
    );
    let p = h.state("stripe-dashboard")["payments"]["pay-6"].clone();
    assert_eq!(p["settled"], "2026-09-18");
    assert_eq!(p["interchange_cents"], 6_750 + 10 + 488);

    // Nina answers dp-1 (never received) with the tracking and the signed delivery.
    h.form(
        "nina",
        "dashboard.stripe.com/disputes/dp-1",
        &[
            ("ev-shipping_tracking", "UPS 1Z4F7A830312345678"),
            (
                "ev-delivery_confirmation",
                "Delivered 2026-09-10 14:12, signed J. PARK",
            ),
        ],
        "evidence",
    );
    assert_eq!(
        h.state("stripe-dashboard")["disputes"]["dp-1"]["status"],
        "under_review"
    );
    assert_eq!(
        h.state("keystone")["disputes"]["cbk-1"]["status"],
        "under_review"
    );
    // Monday 00:00 the charge is available and paid out; Tuesday 08:30 it is in Northwind.
    let before = h.state("northwind")["accounts"]["halyard-operating"]["balance_cents"]
        .as_i64()
        .unwrap();
    h.until(at(4, 0, 1));
    let s = h.state("stripe-dashboard");
    let payout = s["payouts"]["po-1"].clone();
    assert_eq!(payout["status"], "in_transit", "{payout}");
    let amount = payout["amount_cents"].as_i64().unwrap();
    assert_eq!(
        s["accounts"]["halyard-robotics"]["available_cents"]
            .as_i64()
            .unwrap_or(0),
        0
    );
    h.until(at(5, 10, 1));
    assert_eq!(
        h.state("stripe-dashboard")["payouts"]["po-1"]["status"],
        "paid"
    );
    assert_eq!(
        h.state("northwind")["accounts"]["halyard-operating"]["balance_cents"]
            .as_i64()
            .unwrap(),
        before + amount
    );
    // Tuesday 10:00, three business days after the evidence, the network rules for Halyard.
    assert_eq!(h.state("keystone")["disputes"]["cbk-1"]["status"], "won");
    assert_eq!(
        h.state("stripe-dashboard")["disputes"]["dp-1"]["status"],
        "won"
    );
    let pc = h.state("pinecrest");
    assert_eq!(pc["disputes"]["cb-1"]["status"], "lost");
    assert_eq!(
        pc["accounts"]["pc-cc-jpark"]["balance_cents"], -42_000,
        "the provisional credit is reversed"
    );
}

#[test]
fn the_books_take_an_r01_return_from_the_feed_and_present_it_again() {
    let mut h = Halyard::new();
    h.form("omar", "qbo.intuit.com/banking", &[], "sync-1000");
    let feed = h.books()["feed"].clone();
    let line = feed
        .as_object()
        .unwrap()
        .values()
        .find(|f| f["description"] == "ACH RETURN R01 NORTHGATE FULFILLMENT")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    h.form(
        "omar",
        "qbo.intuit.com/banking",
        &[
            (&format!("match-{line}-target"), "PMT-0001"),
            (&format!("match-{line}-fee"), "35"),
        ],
        &format!("match-{line}"),
    );
    let b = h.books();
    assert_eq!(
        b["payments"]["PMT-0001"]["status"], "returned",
        "{}",
        b["activity"]
    );
    assert_eq!(b["invoices"]["INV-1042"]["status"], "open");
    assert_eq!(b["invoices"]["INV-1045"]["total_cents"], 3_500);
    // R01 may be presented again; Northgate still cannot cover it, so it comes back again.
    h.form("omar", "qbo.intuit.com/invoices/INV-1042", &[], "collect");
    let pmt = h.books()["payments"]["PMT-0004"].clone();
    assert_eq!(pmt["method"], "ach_debit", "{pmt}");
    h.until(at(1, 8, 31));
    let pc = h.state("pinecrest");
    let returned = pc["ach"]
        .as_object()
        .unwrap()
        .values()
        .find(|r| r["side"] == "received")
        .unwrap()
        .clone();
    assert_eq!(returned["status"], "returned");
    assert_eq!(returned["return_code"], "R01");
    h.until(at(4, 8, 31));
    let nw = h.state("northwind");
    let back = nw["transactions"]
        .as_object()
        .unwrap()
        .values()
        .filter(|t| t["merchant"] == "ACH RETURN R01 NORTHGATE FULFILLMENT")
        .count();
    assert_eq!(back, 2, "the seeded return and the new one");
}
