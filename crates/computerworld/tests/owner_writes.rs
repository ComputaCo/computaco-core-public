//! The owner's narrow write: an RFC 7386 merge patch to one service's state, read back
//! through `service_state`, carried by the trajectory, and made again by a replay of the
//! action journal at the tick it was first made.
use computerworld::{reference_world, World};
use cw_protocol::{ActionEnvelope, EnvironmentConfig};
use serde_json::json;

const MACHINE: &str = "alice-mac";
const SITE: &str = "x-social";

fn world() -> (World, String) {
    let mut world = World::new(reference_world(), 7).unwrap();
    let session = world
        .environment(EnvironmentConfig {
            actor: "alice".into(),
            machines: vec![MACHINE.into()],
            actions: vec!["browser.v1".into()],
            observations: vec!["semantic.v1".into()],
            action_budget: 1 << 20,
        })
        .unwrap();
    (world, session)
}

fn visit(world: &mut World, session: &str, url: &str) {
    let r = world
        .step(
            session,
            vec![ActionEnvelope::new(
                "browser.v1",
                "navigate",
                MACHINE,
                json!({ "url": url }),
            )],
        )
        .unwrap();
    assert!(r.outcomes[0].success, "{:?}", r.outcomes[0]);
}

fn injected(tick: u64) -> serde_json::Value {
    json!({"posts": {"p-owner-1": {
        "id": "p-owner-1", "author": "alicechen", "text": "Injected by the owner.",
        "tick": tick, "likes": 0, "reposts": 0
    }}})
}

#[test]
fn a_merge_patch_changes_one_service_and_the_page_shows_it() {
    let (mut world, session) = world();
    let before = world.service_state(SITE).unwrap();
    assert_eq!(
        before,
        world.inspect()["state"]["services"][SITE]["state"],
        "service_state is inspect's view of one service"
    );
    world
        .patch_service_state(SITE, &injected(world.tick()))
        .unwrap();
    let after = world.service_state(SITE).unwrap();
    assert_eq!(
        after["posts"]["p-owner-1"]["text"],
        "Injected by the owner."
    );
    assert_eq!(
        after["accounts"], before["accounts"],
        "only what the patch names changes"
    );
    let last = world.trajectory().pop().unwrap();
    assert_eq!(last.kind, "owner.service_state_patched");
    assert_eq!(last.data["service"], SITE);

    visit(&mut world, &session, "http://x.com/explore");
    let page = world.observe(&session).unwrap().channels["semantic.v1"][MACHINE].to_string();
    assert!(
        page.contains("Injected by the owner."),
        "the site serves the patched state"
    );

    // `null` removes, as RFC 7386 says.
    world
        .patch_service_state(SITE, &json!({"posts": {"p-owner-1": null}}))
        .unwrap();
    assert!(world.service_state(SITE).unwrap()["posts"]
        .get("p-owner-1")
        .is_none());
    assert!(world
        .patch_service_state("no-such-service", &json!({}))
        .is_err());
}

#[test]
fn a_replay_of_the_journal_makes_the_same_write_at_the_same_tick() {
    let (mut world, session) = world();
    let initial = world.snapshot();
    visit(&mut world, &session, "http://x.com/");
    world.advance(3_600_000_000).unwrap();
    world
        .patch_service_state(SITE, &injected(world.tick()))
        .unwrap();
    visit(&mut world, &session, "http://x.com/");

    let (mut again, _) = self::world();
    again
        .interfaces_mut()
        .replay(&initial, world.interfaces().action_journal())
        .unwrap();
    assert_eq!(
        again.service_state(SITE).unwrap(),
        world.service_state(SITE).unwrap()
    );
    let kinds = |w: &World| -> Vec<(String, u64)> {
        w.trajectory()
            .into_iter()
            .filter(|e| e.kind.starts_with("owner."))
            .map(|e| (e.kind, e.tick))
            .collect()
    };
    assert_eq!(kinds(&again), kinds(&world), "made at the same tick");
    assert_eq!(again.state_hash().unwrap(), world.state_hash().unwrap());
}
