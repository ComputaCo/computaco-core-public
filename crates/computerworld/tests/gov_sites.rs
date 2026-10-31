//! Government offices end to end through the agent API, on the reference world's three
//! desktops and the kernel's own network and clock: Alice forms an LLC at the Secretary of
//! State, gets its EIN from the Federal Revenue Service and a sales tax license from the
//! Department of Revenue, which checks both numbers with the other offices over the
//! network before it issues; she authorises Bob, who files the first return. Carol takes
//! a sign permit through Port Linden's plan check, a correction and a final inspection.
//! The letters travel to the national post and arrive business days later, by
//! `World::advance`. Every record is read back from `World::inspect()`.
use computerworld::{reference_world, World};
use cw_protocol::{ActionEnvelope, EnvironmentConfig};
use serde_json::{json, Value};

const SETTLE: u64 = 1_000_000;
const DAY: u64 = 86_400_000_000;

struct Team {
    world: World,
    sessions: Vec<(String, String, String)>,
}
impl Team {
    fn new() -> Self {
        let mut world = World::new(reference_world(), 7).unwrap();
        let mut sessions = vec![];
        for (actor, machine) in [
            ("alice", "alice-mac"),
            ("bob", "bob-windows"),
            ("carol", "carol-ubuntu"),
        ] {
            let id = world
                .environment(EnvironmentConfig {
                    actor: actor.into(),
                    machines: vec![machine.into()],
                    actions: vec!["browser.v1".into(), "keyboard.v1".into()],
                    observations: vec!["semantic.v1".into(), "browser.v1".into()],
                    action_budget: 1 << 20,
                })
                .unwrap();
            sessions.push((actor.to_owned(), machine.to_owned(), id));
        }
        Team { world, sessions }
    }
    fn who(&self, actor: &str) -> (String, String) {
        let (_, machine, id) = self.sessions.iter().find(|(a, _, _)| a == actor).unwrap();
        (machine.clone(), id.clone())
    }
    fn act(&mut self, actor: &str, op: &str, payload: Value) {
        let (machine, session) = self.who(actor);
        let result = self
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
            result.outcomes[0].success,
            "{actor} {op} {payload}: {:?}",
            result.outcomes[0]
        );
    }
    fn go(&mut self, actor: &str, url: &str) {
        self.act(actor, "navigate", json!({ "url": url }));
    }
    /// Fills a form's fields and submits it.
    fn form(&mut self, actor: &str, form: &str, fields: &[(&str, &str)]) {
        for (id, value) in fields {
            self.act(actor, "fill", json!({"id": id, "value": value}));
        }
        self.act(actor, "submit", json!({ "id": form }));
    }
    fn url(&self, actor: &str) -> String {
        let (machine, session) = self.who(actor);
        self.world.observe(&session).unwrap().channels["browser.v1"][machine]["url"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    fn says(&self, actor: &str, needle: &str) -> bool {
        let (machine, session) = self.who(actor);
        self.world.observe(&session).unwrap().channels["semantic.v1"][machine]
            .to_string()
            .contains(needle)
    }
    fn state(&self, service: &str) -> Value {
        self.world.inspect()["state"]["services"][service]["state"].clone()
    }
    /// The case number in the address bar: `http://host/cases/<case>/...`.
    fn case(&self, actor: &str) -> String {
        self.url(actor)
            .split("/cases/")
            .nth(1)
            .unwrap()
            .split('/')
            .next()
            .unwrap()
            .to_owned()
    }
    fn account(&mut self, actor: &str, host: &str, fields: [&str; 6]) {
        self.go(actor, &format!("http://{host}/account"));
        let ids = [
            "acct-name",
            "acct-dob",
            "acct-ssn",
            "acct-address",
            "acct-zip",
            "acct-email",
        ];
        let filled: Vec<(&str, &str)> = ids.into_iter().zip(fields).collect();
        self.form(actor, "account", &filled);
    }
    fn pages(
        &mut self,
        actor: &str,
        host: &str,
        program: &str,
        pages: &[&[(&str, &str)]],
    ) -> String {
        self.go(actor, &format!("http://{host}/services/{program}"));
        self.act(actor, "submit", json!({"id": "start"}));
        let case = self.case(actor);
        for fields in pages {
            let filled: Vec<(String, &str)> =
                fields.iter().map(|(k, v)| (format!("f-{k}"), *v)).collect();
            let refs: Vec<(&str, &str)> = filled.iter().map(|(k, v)| (k.as_str(), *v)).collect();
            self.form(actor, "page", &refs);
        }
        case
    }
    fn sign(&mut self, actor: &str, name: &str) {
        self.form(actor, "sign", &[("signature", name)]);
    }
}

const ALICE: [&str; 6] = [
    "Alice Chen",
    "06/14/1991",
    "532-18-4471",
    "220 Harbor Street, Port Linden, AN 97301",
    "97301",
    "alice@northstar.example",
];
const BOB: [&str; 6] = [
    "Bob Martinez",
    "02/27/1985",
    "547-90-2316",
    "41 Cedar Avenue, Port Linden, AN 97302",
    "97302",
    "bob@northstar.example",
];
const CAROL: [&str; 6] = [
    "Carol Nakamura",
    "10/05/1993",
    "561-33-8820",
    "9 Ferry Road, Port Linden, AN 97303",
    "97303",
    "carol@northstar.example",
];
const CARD: [(&str, &str); 4] = [
    ("card-name", "Alice Chen"),
    ("card-number", "4242 4242 4242 4242"),
    ("card-expiry", "09/29"),
    ("card-cvc", "314"),
];

#[test]
fn a_founder_registers_a_business_with_three_offices_and_her_bookkeeper_files_the_first_return() {
    let mut t = Team::new();
    let sos = "sos.arden.gov.example";
    t.account("alice", sos, ALICE);
    assert_eq!(
        t.state("gov-arden-sos")["accounts"]["alice"]["verified"],
        true,
        "the post's residents reach the office"
    );
    t.go(
        "alice",
        &format!("http://{sos}/name-check?name=Northstar+Press+LLC"),
    );
    assert!(t.says("alice", "Available."));
    let llc = t.pages(
        "alice",
        sos,
        "llc",
        &[
            &[("entity_name", "Northstar Press LLC")],
            &[(
                "principal_address",
                "220 Harbor Street, Port Linden, AN 97301",
            )],
            &[
                ("agent_name", "Alice Chen"),
                ("agent_address", "220 Harbor Street, Port Linden, AN 97301"),
                ("agent_consent", "Yes"),
            ],
            &[("management", "Member-managed")],
        ],
    );
    t.sign("alice", "Alice Chen");
    t.form("alice", "pay-card", &CARD);
    assert!(t
        .url("alice")
        .ends_with(&format!("/cases/{llc}/confirmation")));
    let entity = t.state("gov-arden-sos")["cases"][&llc]["issued"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        t.state("gov-arden-sos")["records"][&entity]["status"],
        "active"
    );

    let frs = "frs.gov.example";
    t.account("alice", frs, ALICE);
    let ein_case = t.pages(
        "alice",
        frs,
        "ein",
        &[
            &[
                ("entity_type", "Limited liability company"),
                ("members", "1"),
                ("formed_in", "AN"),
            ],
            &[
                ("responsible_ssn", "532-18-4471"),
                ("responsible_title", "Member"),
            ],
            &[
                ("legal_name", "Northstar Press LLC"),
                (
                    "mailing_address",
                    "220 Harbor Street, Port Linden, AN 97301",
                ),
                ("start_date", "09/17/2026"),
                ("reason", "Started a new business"),
                ("activity", "Retail"),
                ("employees", "2"),
            ],
        ],
    );
    t.sign("alice", "Alice Chen");
    let ein = t.state("gov-frs")["cases"][&ein_case]["issued"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(t.says("alice", &ein));

    // The license: Revenue asks the Secretary of State and the FRS over the network.
    let dor = "revenue.arden.gov.example";
    t.account("alice", dor, ALICE);
    let lic = t.pages(
        "alice",
        dor,
        "sales-tax-license",
        &[
            &[
                ("business_name", "Northstar Press LLC"),
                ("entity_number", &entity),
                ("ein", &ein),
            ],
            &[
                (
                    "location_address",
                    "220 Harbor Street, Port Linden, AN 97301",
                ),
                ("start_date", "10/01/2026"),
                ("products", "Printed field guides"),
            ],
            &[("owner_title", "Member")],
        ],
    );
    t.sign("alice", "Alice Chen");
    t.form("alice", "pay-card", &CARD);
    // The checks went out and came back while the browser waited on the redirect.
    let c = t.state("gov-arden-revenue")["cases"][&lic].clone();
    assert_eq!(c["checks"], json!({"0": "matched", "1": "matched"}), "{c}");
    assert_eq!(c["status"], "approved");
    let license = c["issued"].as_str().unwrap().to_owned();
    t.go("alice", &format!("http://{dor}/records/{license}"));
    t.form("alice", "team-form", &[("team-member", "bob")]);

    // Bob files September from the license's filing list, by ACH.
    t.account("bob", dor, BOB);
    t.go("bob", &format!("http://{dor}/account"));
    let ob = t.state("gov-arden-revenue")["obligations"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    t.act("bob", "click", json!({"id": format!("file-{ob}")}));
    t.act("bob", "submit", json!({"id": "start"}));
    t.form(
        "bob",
        "page",
        &[("f-gross_sales", "4,000"), ("f-exempt_sales", "0")],
    );
    assert!(t.says("bob", "$250.00"));
    t.sign("bob", "Bob Martinez");
    t.form(
        "bob",
        "pay-bank",
        &[
            ("bank-name", "Bob Martinez"),
            ("bank-routing", "123456780"),
            ("bank-account", "4417002291"),
        ],
    );
    let s = t.state("gov-arden-revenue");
    assert_eq!(s["obligations"][&ob]["status"], "filed");
    let ret = s["obligations"][&ob]["case"].as_str().unwrap();
    assert_eq!(s["cases"][ret]["paid_cents"], 25000);
    assert_eq!(
        s["payments"]
            .as_object()
            .unwrap()
            .values()
            .next_back()
            .unwrap()["method"],
        "bank"
    );

    // The letters went to the national post and arrive business days later.
    t.world.advance(SETTLE).unwrap();
    let post = t.state("gov-post");
    let subjects: Vec<&str> = post["letters"]
        .as_object()
        .unwrap()
        .values()
        .filter(|p| p["to"] == "alice")
        .map(|p| p["subject"].as_str().unwrap())
        .collect();
    assert!(
        subjects.contains(&"Certificate of Organization: Northstar Press LLC"),
        "{subjects:?}"
    );
    assert!(subjects.contains(&"CP 575: We assigned you an Employer Identification Number"));
    t.go("alice", "http://mailbox.post.gov.example/");
    assert!(t.says("alice", "Coming to your mailbox"));
    assert!(!t.says("alice", "CP 575"));
    // The FRS mails in five business days: Thursday the 24th.
    t.world.advance(8 * DAY).unwrap();
    t.go("alice", "http://mailbox.post.gov.example/");
    assert!(t.says("alice", "CP 575"));
}

#[test]
fn a_contractor_takes_a_sign_permit_through_a_correction_to_a_passed_final_inspection() {
    let mut t = Team::new();
    let city = "permits.portlinden.gov.example";
    t.account("carol", city, CAROL);
    let case = t.pages(
        "carol",
        city,
        "sign-permit",
        &[
            &[
                ("site_address", "220 Harbor Street, Port Linden, AN 97301"),
                ("zone", "C-1"),
                ("business_name", "Northstar Press LLC"),
            ],
            &[
                ("sign_type", "Wall"),
                ("illuminated", "No"),
                ("contractor", "Nakamura Signworks"),
                ("contractor_license", "PL-SC-1187"),
                ("valuation", "2,400"),
            ],
        ],
    );
    let sheet = "Site address: 220 Harbor Street, Port Linden, AN 97301\nSign area (sq ft): 40\nHeight above grade (ft): 12\nMounting: Wall";
    t.form("carol", "attachments", &[("a-plan", sheet)]);
    t.sign("carol", "Carol Nakamura");
    t.form(
        "carol",
        "pay-card",
        &[
            ("card-name", "Carol Nakamura"),
            ("card-number", "5555 5555 5555 4444"),
            ("card-expiry", "03/28"),
            ("card-cvc", "202"),
        ],
    );
    assert_eq!(
        t.state("gov-portlinden")["cases"][&case]["status"],
        "corrections_required"
    );
    t.act("carol", "click", json!({"id": "view-corrections"}));
    assert!(t.says("carol", "zone C-1 allows at most 32 sq ft"));
    t.act("carol", "click", json!({"id": "edit-attachments"}));
    t.form(
        "carol",
        "attachments",
        &[("a-plan", &sheet.replace("40", "32"))],
    );
    t.act("carol", "submit", json!({"id": "resubmit"}));
    let permit = t.state("gov-portlinden")["cases"][&case]["issued"]
        .as_str()
        .unwrap()
        .to_owned();
    t.act("carol", "click", json!({"id": "issued-number"}));
    t.form("carol", "inspect-1", &[("inspect-date-1", "09/22/2026")]);
    assert_eq!(
        t.state("gov-portlinden")["records"][&permit]["status"],
        "issued"
    );
    t.world.advance(6 * DAY).unwrap();
    let s = t.state("gov-portlinden");
    assert_eq!(s["records"][&permit]["status"], "finaled");
    assert_eq!(
        s["inspections"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()["status"],
        "passed"
    );
}
