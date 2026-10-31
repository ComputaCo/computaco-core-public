//! A platform pays a creator as itself. In Lantern Media's world YouTube's studio owes
//! the channel its finalized earnings; on the payout day its own timer sends an ACH
//! credit to Northwind, Google's bank, as `service:youtube` — not as whoever's visit
//! happened to run the clock — and Northwind takes it under the origination agreement
//! that principal holds on Google's payout account. The entry goes to FedLine at the
//! next cut-off and lands in Lantern's account at Pinecrest the next business day.
use computerworld::{EnvironmentConfig, World, WorldDefinition};
use cw_protocol::ActionEnvelope;
use serde_json::{json, Value};

const SECOND: u64 = 1_000_000;
const HOUR: u64 = 3_600 * SECOND;
const DAY: u64 = 24 * HOUR;
const OWED: i64 = 1_234_500;

fn lantern() -> (World, String) {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../worlds/lantern-media/world.json"
    ))
    .unwrap();
    let definition: WorldDefinition = serde_json::from_str(&text).unwrap();
    let mut world = World::new(definition, 7).unwrap();
    // A finished cut waiting to go public, and September's earnings already owed.
    world
        .patch_service_state(
            "youtube",
            &json!({"studio": {
                "uploads": {"v-dock": {"id": "v-dock", "channel": "lantern", "uploader": "kenji",
                    "filename": "dock-tour.apng", "status": "ready", "title": "Dock tour",
                    "made_for_kids": "no", "visibility": "private"}},
                "partners": {"lantern": {"balance_cents": OWED}}
            }}),
        )
        .unwrap();
    let session = world
        .environment(EnvironmentConfig {
            actor: "priya".into(),
            machines: vec!["priya-mac".into()],
            actions: vec!["browser.v1".into()],
            observations: vec!["browser.v1".into()],
            action_budget: 1 << 20,
        })
        .unwrap();
    (world, session)
}
fn act(world: &mut World, session: &str, op: &str, payload: Value) {
    let r = world
        .step(
            session,
            vec![ActionEnvelope::new("browser.v1", op, "priya-mac", payload)],
        )
        .unwrap();
    assert!(r.outcomes[0].success, "{op}: {:?}", r.outcomes[0]);
}
fn lantern_account(world: &World) -> Value {
    world.service_state("pinecrest").unwrap()["accounts"]["lantern-operating"].clone()
}

#[test]
fn a_payout_is_originated_by_the_platform_itself_and_lands_in_the_creators_bank() {
    let (mut world, session) = lantern();
    let before = lantern_account(&world)["balance_cents"].as_i64().unwrap();
    // Publishing starts the studio's daily count, which pays on the 21st.
    act(
        &mut world,
        &session,
        "navigate",
        json!({"url": "http://studio.youtube.com/videos/v-dock"}),
    );
    act(
        &mut world,
        &session,
        "click",
        json!({"id": "make-public-submit"}),
    );
    let mut payout = Value::Null;
    for _ in 0..40 {
        world.advance(DAY).unwrap();
        let tube = world.service_state("youtube").unwrap();
        if let Some(p) = tube["studio"]["payouts"]
            .as_object()
            .and_then(|m| m.values().next())
        {
            payout = p.clone();
            break;
        }
    }
    assert_eq!(payout["status"], "paid", "{payout}");
    assert!(payout["amount_cents"].as_i64().unwrap() >= OWED, "{payout}");
    let amount = payout["amount_cents"].as_i64().unwrap();
    assert!(
        payout["note"]
            .as_str()
            .unwrap()
            .starts_with("accepted by the bank as ach-"),
        "{payout}"
    );
    // Northwind took it from Google's payout account under the agreement.
    let northwind = world.service_state("northwind").unwrap();
    let entry = northwind["ach"]
        .as_object()
        .unwrap()
        .values()
        .find(|a| a["account"] == "google-creator-payouts")
        .expect("the originated entry")
        .clone();
    assert_eq!(entry["side"], "originated");
    assert_eq!(entry["entry"]["company_name"], "GOOGLE LLC");
    assert_eq!(entry["entry"]["receiver_routing"], "321080408");
    assert_eq!(entry["entry"]["receiver_account"], "88004417");
    assert_eq!(entry["entry"]["amount_cents"], amount);
    // The request reached the bank as the platform's principal.
    assert!(world.trajectory().iter().any(|e| {
        e.kind == "http.submitted"
            && e.actor.as_deref() == Some("service:youtube")
            && e.data["request"]["url"] == "http://northwind.example/api/ach"
    }));
    // The next business day it is in Lantern's account at Pinecrest.
    for _ in 0..5 {
        if lantern_account(&world)["balance_cents"].as_i64().unwrap() > before {
            break;
        }
        world.advance(DAY).unwrap();
    }
    assert_eq!(
        lantern_account(&world)["balance_cents"].as_i64().unwrap(),
        before + amount
    );
    let pinecrest = world.service_state("pinecrest").unwrap();
    assert!(pinecrest["transactions"]
        .as_object()
        .unwrap()
        .values()
        .any(|t| t["account"] == "lantern-operating" && t["amount_cents"] == amount));
}
