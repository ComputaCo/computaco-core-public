//! Doing business on the simulated internet, end to end through the agent API on the
//! reference world's three desktops: carol deploys a landing page on Netlify, alice sells
//! a product through a Stripe payment link and promotes the page on X Ads, wiring Stripe's
//! webhook to the ad account's conversion endpoint; bob sees the ad on his X timeline,
//! clicks it, lands on the page with the click id and buys. Every record is then read
//! back from `World::inspect()`: the deploy, the impression, the click and its cost, the
//! payment with the click id it carried, the webhook delivery and the attributed sale.
//! A second site is built from a GitHub repository over the synthetic network.
use computerworld::{reference_world, World};
use cw_protocol::{ActionEnvelope, EnvironmentConfig};
use serde_json::{json, Value};

/// One second of world time: long enough for any request a service sent to land.
const SETTLE: u64 = 1_000_000;

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
    fn act(&mut self, actor: &str, op: &str, payload: Value) -> Value {
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
        result.outcomes[0].value.clone()
    }
    fn go(&mut self, actor: &str, url: &str) {
        self.act(actor, "navigate", json!({ "url": url }));
    }
    fn fill(&mut self, actor: &str, fields: &[(&str, &str)]) {
        for (id, value) in fields {
            self.act(actor, "fill", json!({"id": id, "value": value}));
        }
    }
    fn submit(&mut self, actor: &str, form: &str) {
        self.act(actor, "submit", json!({ "id": form }));
    }
    fn url(&self, actor: &str) -> String {
        let (machine, session) = self.who(actor);
        self.world.observe(&session).unwrap().channels["browser.v1"][machine]["url"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    /// The semantic tree's ids, for "is it on the page".
    fn ids(&self, actor: &str) -> Vec<String> {
        fn walk(e: &[Value], out: &mut Vec<String>) {
            for x in e {
                if let Some(id) = x["id"].as_str() {
                    out.push(id.to_owned());
                }
                walk(
                    x["children"].as_array().map(Vec::as_slice).unwrap_or(&[]),
                    out,
                );
            }
        }
        let (machine, session) = self.who(actor);
        let page = &self.world.observe(&session).unwrap().channels["semantic.v1"][machine];
        let mut out = vec![];
        walk(page["elements"].as_array().unwrap(), &mut out);
        out
    }
    /// Whether the page shows `needle` anywhere in its text.
    fn says(&self, actor: &str, needle: &str) -> bool {
        let (machine, session) = self.who(actor);
        let page = &self.world.observe(&session).unwrap().channels["semantic.v1"][machine];
        page.to_string().contains(needle)
    }
    fn state(&self, service: &str) -> Value {
        self.world.inspect()["state"]["services"][service]["state"].clone()
    }
}

const LANDING: &str = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>Atlas Field Guide</title></head>
<body>
<h1 id="hero">The Atlas Field Guide</h1>
<p>Deterministic testing, from the people who wrote the runtime.</p>
<a id="buy" href="https://buy.stripe.com/CODE">Buy the guide</a>
<script>
var buy = document.getElementById('buy');
if (location.search) { buy.setAttribute('href', buy.getAttribute('href') + location.search); }
</script>
</body></html>
"#;

#[test]
fn a_team_deploys_a_page_advertises_it_and_a_customer_who_clicked_the_ad_buys() {
    let mut t = Team::new();

    // Alice: a Stripe account, a product and a payment link.
    t.go("alice", "http://dashboard.stripe.com/");
    t.fill(
        "alice",
        &[
            ("account-name", "Northstar Press"),
            ("account-website", "https://atlas-guide.netlify.app/"),
            ("account-team", "carol"),
        ],
    );
    t.submit("alice", "account");
    t.go("alice", "http://dashboard.stripe.com/products");
    t.fill(
        "alice",
        &[
            ("product-name", "Atlas Field Guide"),
            ("product-description", "The PDF and the example repository"),
            ("product-price", "39"),
        ],
    );
    t.submit("alice", "product");
    assert_eq!(
        t.url("alice"),
        "http://dashboard.stripe.com/products/prod-1"
    );
    t.submit("alice", "create-link");
    let code = t.url("alice").rsplit('/').next().unwrap().to_owned();
    assert_eq!(
        t.state("stripe-dashboard")["links"][&code]["product"],
        "prod-1"
    );

    // Carol: a Netlify site with the landing page, deployed.
    t.go("carol", "http://app.netlify.com/");
    t.fill(
        "carol",
        &[("new-site-name", "atlas-guide"), ("new-site-team", "alice")],
    );
    t.submit("carol", "new-site");
    assert_eq!(t.url("carol"), "http://app.netlify.com/sites/atlas-guide");
    t.fill(
        "carol",
        &[
            ("upload-path", "index.html"),
            ("upload-content", &LANDING.replace("CODE", &code)),
        ],
    );
    t.submit("carol", "upload");
    t.fill("carol", &[("deploy-message", "Launch page")]);
    t.submit("carol", "deploy");
    assert_eq!(
        t.url("carol"),
        "http://app.netlify.com/sites/atlas-guide/deploys/dpl-1"
    );
    let netlify = t.state("netlify");
    assert_eq!(netlify["sites"]["atlas-guide"]["published"], "dpl-1");
    assert_eq!(
        netlify["sites"]["atlas-guide"]["team"],
        json!(["carol", "alice"])
    );

    // Anyone can now browse it at its name, through the wildcard record.
    t.go("bob", "https://atlas-guide.netlify.app/");
    assert!(t.ids("bob").contains(&"hero".to_owned()));

    // Alice: an X Ads account, the conversion webhook, and a campaign for software people.
    t.go("alice", "http://ads.x.com/");
    t.fill(
        "alice",
        &[
            ("account-name", "Northstar Press"),
            ("account-website", "https://atlas-guide.netlify.app/"),
        ],
    );
    t.submit("alice", "account");
    t.go("alice", "http://dashboard.stripe.com/webhooks");
    t.fill(
        "alice",
        &[(
            "webhook-url",
            "http://ads.x.com/conversions/northstar-press",
        )],
    );
    t.submit("alice", "webhook");
    t.go("alice", "http://ads.x.com/campaigns/new");
    t.fill(
        "alice",
        &[
            ("campaign-name", "Field guide launch"),
            ("campaign-headline", "Make your tests replay byte for byte"),
            ("campaign-text", "The Atlas Field Guide is out: $39."),
            ("campaign-url", "https://atlas-guide.netlify.app/"),
            ("campaign-interests", "software"),
            ("campaign-regions", "US"),
            ("campaign-placements", "x-home"),
            ("campaign-budget", "20"),
            ("campaign-bid", "0.75"),
        ],
    );
    t.submit("alice", "campaign");
    assert_eq!(t.url("alice"), "http://ads.x.com/campaigns/cmp-2");

    // Carol is in Canada: her timeline gets no ad, and nothing is recorded.
    t.go("carol", "http://x.com/");
    assert!(!t.ids("carol").contains(&"ad-x-home-link".to_owned()));
    // Bob's timeline asks X Ads for an ad; the campaign wins it.
    t.go("bob", "http://x.com/");
    assert!(
        t.ids("bob").contains(&"ad-x-home-link".to_owned()),
        "{:?}",
        t.ids("bob")
    );
    let ads = t.state("x-ads");
    assert_eq!(ads["impressions"]["imp-1"]["actor"], "bob");
    assert_eq!(ads["impressions"]["imp-1"]["campaign"], "cmp-2");
    assert_eq!(
        ads["impressions"]["imp-1"]["price_cents"], 25,
        "alone, it pays the reserve"
    );
    assert!(ads["impressions"].get("imp-2").is_none());

    // The click lands on the page with the tracking parameters, which the page passes on.
    t.act("bob", "click", json!({"id": "ad-x-home-link"}));
    assert_eq!(
        t.url("bob"),
        "https://atlas-guide.netlify.app/?utm_source=x&utm_medium=cpc&utm_campaign=cmp-2&xclid=clk-1"
    );
    let ads = t.state("x-ads");
    assert_eq!(ads["clicks"]["clk-1"]["cost_cents"], 25);
    assert_eq!(ads["campaigns"]["cmp-2"]["spend_cents"], 25);
    t.act("bob", "click", json!({"id": "buy"}));
    assert_eq!(
        t.url("bob"),
        format!("https://buy.stripe.com/{code}?utm_source=x&utm_medium=cpc&utm_campaign=cmp-2&xclid=clk-1")
    );
    t.fill(
        "bob",
        &[
            ("pay-email", "bob@northstar.example"),
            ("pay-card", "4242 4242 4242 4242"),
            ("pay-exp", "08 / 29"),
            ("pay-cvc", "314"),
            ("pay-name", "Bob Martinez"),
        ],
    );
    t.submit("bob", "pay");
    assert_eq!(t.url("bob"), "https://buy.stripe.com/receipts/pay-1");
    t.world.advance(SETTLE).unwrap();

    let stripe = t.state("stripe-dashboard");
    let payment = &stripe["payments"]["pay-1"];
    assert_eq!(payment["customer"], "bob");
    assert_eq!(payment["amount_cents"], 3900);
    assert_eq!(payment["fee_cents"], 143);
    assert_eq!(payment["metadata"]["xclid"], "clk-1");
    assert_eq!(stripe["accounts"]["northstar-press"]["balance_cents"], 3757);
    assert_eq!(stripe["deliveries"]["evt-1"]["status"], "delivered");
    let ads = t.state("x-ads");
    let sale = &ads["conversions"]["cnv-1"];
    assert_eq!(sale["click"], "clk-1");
    assert_eq!(sale["method"], "click_id");
    assert_eq!(sale["value_cents"], 3900);
    assert_eq!(sale["order"], "pay-1");
    assert_eq!(ads["campaigns"]["cmp-2"]["revenue_cents"], 3900);

    // The founder reads it off the dashboards.
    t.go("alice", "http://ads.x.com/reports");
    assert!(t.says("alice", "Last click") || t.says("alice", "Click id"));
    assert!(t.says("alice", "$39.00"));
    t.go("alice", "http://dashboard.stripe.com/");
    assert!(t.ids("alice").contains(&"payment-pay-1".to_owned()));
}

#[test]
fn a_site_linked_to_a_github_repository_builds_from_it_and_a_registered_domain_serves_it() {
    let mut t = Team::new();
    t.go("carol", "http://app.netlify.com/");
    t.fill("carol", &[("new-site-name", "atlas-docs")]);
    t.submit("carol", "new-site");
    t.fill(
        "carol",
        &[
            ("repo-url", "http://github.com/northstar/atlas"),
            ("repo-branch", "main"),
        ],
    );
    t.submit("carol", "repo");
    t.submit("carol", "deploy");
    assert_eq!(
        t.state("netlify")["sites"]["atlas-docs"]["deploys"]["dpl-1"]["state"],
        "building"
    );
    t.world.advance(SETTLE).unwrap();
    let site = &t.state("netlify")["sites"]["atlas-docs"];
    let deploy = &site["deploys"]["dpl-1"];
    assert_eq!(deploy["state"], "ready", "{deploy}");
    assert_eq!(site["published"], "dpl-1");
    let github = t.state("github");
    let tip = github["repositories"]["atlas"]["refs"]["refs/heads/main"]
        .as_str()
        .unwrap();
    assert_eq!(deploy["commit"], tip);
    assert_eq!(
        deploy["files"]["README.md"],
        github["repositories"]["atlas"]["objects"][tip]["files"]["README.md"]
    );
    // The deploy page refreshed itself while building; now it shows the result.
    t.go(
        "carol",
        "http://app.netlify.com/sites/atlas-docs/deploys/dpl-1",
    );
    assert!(t.ids("carol").contains(&"deploy-url".to_owned()));

    t.go("carol", "http://app.netlify.com/domains");
    t.fill(
        "carol",
        &[
            ("register-domain", "atlas-docs.shop"),
            ("register-site", "atlas-docs"),
        ],
    );
    t.submit("carol", "register");
    assert_eq!(
        t.state("netlify")["domains"]["atlas-docs.shop"]["site"],
        "atlas-docs"
    );
    t.go("bob", "http://atlas-docs.shop/README.md");
    assert_eq!(t.url("bob"), "http://atlas-docs.shop/README.md");
    // An unregistered .shop name reaches the registrar's page rather than nowhere.
    let (machine, session) = t.who("bob");
    let url = json!({"url": "http://nobody-yet.shop/"});
    t.world
        .step(
            &session,
            vec![ActionEnvelope::new("browser.v1", "navigate", &machine, url)],
        )
        .unwrap();
    assert!(t.ids("bob").contains(&"notice-title".to_owned()));
}

#[test]
fn a_search_ad_runs_against_a_competitor_on_google() {
    let mut t = Team::new();
    t.go("alice", "http://ads.google.com/");
    t.fill(
        "alice",
        &[
            ("account-name", "Northstar Coffee Lab"),
            ("account-website", "https://etsy.com/"),
        ],
    );
    t.submit("alice", "account");
    t.go("alice", "http://ads.google.com/campaigns/new");
    t.fill(
        "alice",
        &[
            ("campaign-name", "Cold brew"),
            ("campaign-headline", "Cold brew concentrate, shipped"),
            ("campaign-url", "https://etsy.com/"),
            ("campaign-keywords", "cold brew"),
            ("campaign-budget", "30"),
            ("campaign-bid", "1.50"),
        ],
    );
    t.submit("alice", "campaign");
    t.go("bob", "http://google.com/search?q=best+cold+brew+ratio");
    assert!(
        t.ids("bob")
            .contains(&"ad-google-search-top-link".to_owned()),
        "{:?}",
        t.ids("bob")
    );
    let ads = t.state("google-ads");
    // Beanline bids $1.20 on "cold brew": alice's $1.50 wins and pays $1.21.
    assert_eq!(ads["impressions"]["imp-1"]["campaign"], "cmp-2");
    assert_eq!(ads["impressions"]["imp-1"]["price_cents"], 121);
    t.act("bob", "click", json!({"id": "ad-google-search-top-link"}));
    assert!(t.url("bob").starts_with(
        "https://etsy.com/?utm_source=google&utm_medium=cpc&utm_campaign=cmp-2&gclid=clk-1"
    ));
    // A query no campaign bids on shows no ad.
    t.go("bob", "http://google.com/search?q=atlas+runtime");
    assert!(!t
        .ids("bob")
        .contains(&"ad-google-search-top-link".to_owned()));
}
