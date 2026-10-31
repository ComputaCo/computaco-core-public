//! Worlds, and forks of them, move between threads and run there.
//!
//! A `World` is `Send`, and the society runner forks one world into an experiment's
//! arms and runs each arm on its own thread. A page that runs script (x.com's home
//! timeline carries the ad network's tag) puts a JS realm into the browser's state,
//! and the realm's VM keeps per-thread tables (the cycle collector's object table,
//! the canonical-string interner, the compiled-code cache): a realm built on one
//! thread and used or dropped on another indexed the wrong thread's tables and
//! aborted in the collector. These drive exactly that: a scripted page loaded on one
//! thread, then the world, and forks of it, stepped and dropped on others, in
//! parallel, with results identical to the same steps run on one thread.
use computerworld::{reference_world, World};
use cw_protocol::{ActionEnvelope, EnvironmentConfig};
use serde_json::{json, Value};

const MACHINE: &str = "alice-mac";

fn world() -> (World, String) {
    let mut world = World::new(reference_world(), 42).unwrap();
    let session = world
        .environment(EnvironmentConfig {
            actor: "alice".into(),
            machines: vec![MACHINE.into()],
            actions: vec!["browser.v1".into(), "keyboard.v1".into()],
            observations: vec!["semantic.v1".into(), "browser.v1".into()],
            action_budget: 1 << 20,
        })
        .unwrap();
    (world, session)
}

fn act(world: &mut World, session: &str, op: &str, payload: Value) -> Value {
    let result = world
        .step(
            session,
            vec![ActionEnvelope::new("browser.v1", op, MACHINE, payload)],
        )
        .unwrap();
    assert!(result.outcomes[0].success, "{op}: {:?}", result.outcomes[0]);
    result.outcomes[0].value.clone()
}

/// What a run of an arm leaves behind that another run of it must reproduce.
fn outcome(world: &World, session: &str) -> (String, String) {
    let page = world.observe(session).unwrap().channels["semantic.v1"][MACHINE].to_string();
    (world.state_hash().unwrap(), page)
}

/// One arm: a few gestures on the scripted page, each entering its realm.
fn arm(world: &mut World, session: &str, n: usize) -> (String, String) {
    act(
        world,
        session,
        "scroll",
        json!({ "y": 200 + 100 * n as i64 }),
    );
    act(
        world,
        session,
        "navigate",
        json!({ "url": "http://x.com/explore" }),
    );
    act(world, session, "back", json!({}));
    act(world, session, "reload", json!({}));
    outcome(world, session)
}

/// x.com's home timeline, with its ad tag run, on this thread.
fn booted() -> (World, String) {
    let (mut world, session) = world();
    act(
        &mut world,
        &session,
        "navigate",
        json!({ "url": "http://x.com/" }),
    );
    (world, session)
}

#[test]
fn a_world_with_a_live_realm_runs_on_another_thread_and_comes_back() {
    let (world, session) = booted();
    let (mut world, first) = std::thread::spawn(move || {
        let mut world = world;
        let out = arm(&mut world, &session, 0);
        (world, (session, out))
    })
    .join()
    .expect("the world runs on a second thread");
    let (session, _) = first;
    // Back on the thread that built the realm, and on to a third.
    arm(&mut world, &session, 1);
    std::thread::spawn(move || {
        let mut world = world;
        arm(&mut world, &session, 2);
        drop(world);
    })
    .join()
    .expect("the world runs, and drops, on a third thread");
}

#[test]
fn forks_run_in_parallel_threads_and_match_a_sequential_run() {
    const ARMS: usize = 3;
    let (base, session) = booted();
    let snapshot = base.snapshot();

    let sequential: Vec<_> = (0..ARMS)
        .map(|n| {
            let mut fork = base.fork(&snapshot).unwrap();
            arm(&mut fork, &session, n)
        })
        .collect();

    let forks: Vec<World> = (0..ARMS).map(|_| base.fork(&snapshot).unwrap()).collect();
    let parallel: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = forks
            .into_iter()
            .enumerate()
            .map(|(n, mut fork)| {
                let session = session.clone();
                scope.spawn(move || {
                    let out = arm(&mut fork, &session, n);
                    // Half the forks come back to be dropped by the base's thread.
                    (out, (n % 2 == 0).then_some(fork))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("an arm runs on its own thread"))
            .collect()
    });
    let (parallel, returned): (Vec<_>, Vec<_>) = parallel.into_iter().unzip();
    assert_eq!(
        parallel, sequential,
        "parallel arms reproduce sequential ones"
    );
    drop(returned);

    // The base, whose realm the forks shared, still runs where it was built.
    let mut base = base;
    arm(&mut base, &session, 0);
}
