//! The expert procedures in every `worlds/*/procedures/` folder, projected onto the GUI.
//! Each must verify when performed on screen and when performed with its tool calls,
//! must fail verification when a state-changing phase is left out, and must replay
//! from its recorded actions alone to the same state hash. A team procedure gets one
//! session per role, each on that role's machine.
use computerworld::procedure::{Check, Instance, Mode, Procedure, ProjectOptions};
use computerworld::{EnvironmentConfig, World};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const WORLDS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../worlds");
/// What a role may do: launch from its dock, point, type, browse, read and write files,
/// and (for the tool path, which has no screen) run commands. The same set
/// `scripts/procedures/debug.py` grants.
const GRANTS: [&str; 6] = [
    "application.v1",
    "keyboard.v1",
    "pointer.v1",
    "browser.v1",
    "filesystem.v1",
    "terminal.v1",
];

/// Every procedure, with the world folder it runs in.
fn procedures() -> Vec<(PathBuf, Procedure)> {
    let mut found = Vec::new();
    for world in std::fs::read_dir(WORLDS).unwrap() {
        let world = world.unwrap().path();
        let Ok(entries) = std::fs::read_dir(world.join("procedures")) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|x| x == "json") {
                let text = std::fs::read_to_string(&path).unwrap();
                let procedure = Procedure::from_json(&text)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                found.push((world.clone(), procedure));
            }
        }
    }
    found.sort_by(|a, b| a.1.id.cmp(&b.1.id));
    assert!(found.len() >= 2, "expected the procedure corpus");
    // `CW_PROCEDURE_FILTER=office-` checks only the procedures whose id contains it,
    // for working on one domain while others are unfinished.
    if let Ok(filter) = std::env::var("CW_PROCEDURE_FILTER") {
        found.retain(|(_, p)| p.id.contains(&filter));
        assert!(!found.is_empty(), "no procedure id contains {filter:?}");
    }
    found
}
fn staged(folder: &Path, instance: &Instance) -> (World, BTreeMap<String, String>) {
    let text = std::fs::read_to_string(folder.join("world.json")).unwrap();
    let mut definition: computerworld::WorldDefinition = serde_json::from_str(&text).unwrap();
    instance.procedure.prepare(&mut definition);
    let mut world = World::new(definition, 7).unwrap();
    let sessions = sessions_for(&mut world, instance);
    (world, sessions)
}
/// Stage `instance` in a world that is already running and open a session per role.
fn sessions_for(world: &mut World, instance: &Instance) -> BTreeMap<String, String> {
    world.stage(instance).unwrap();
    let procedure = &instance.procedure;
    let mut roles: BTreeMap<String, (String, String)> = procedure
        .roles
        .iter()
        .map(|(name, role)| (name.clone(), (role.machine.clone(), role.user.clone())))
        .collect();
    if roles.is_empty() {
        let user = world
            .definition()
            .computers
            .iter()
            .find(|c| c.id == procedure.machine)
            .map(|c| c.user.clone())
            .unwrap();
        roles.insert(String::new(), (procedure.machine.clone(), user));
    }
    let mut sessions = BTreeMap::new();
    for (name, (machine, user)) in roles {
        let config: EnvironmentConfig = serde_json::from_value(serde_json::json!({
            "actor": user,
            "machines": [machine],
            "actions": GRANTS,
            "observations": ["semantic.v1"],
        }))
        .unwrap();
        sessions.insert(name, world.environment(config).unwrap());
    }
    if !sessions.contains_key("") {
        let first = sessions.values().next().unwrap().clone();
        sessions.insert(String::new(), first);
    }
    sessions
}
/// The files the instance's checks read, as they are now: what a failure has to show.
fn checked_files(world: &World, instance: &Instance) -> String {
    let mut out = String::new();
    for check in &instance.procedure.verify {
        if let Check::FileEquals { machine, path, .. }
        | Check::FileContains { machine, path, .. }
        | Check::FileHasLine { machine, path, .. }
        | Check::FileLacks { machine, path, .. }
        | Check::FileMatches { machine, path, .. }
        | Check::FileSha256 { machine, path, .. }
        | Check::FileJson { machine, path, .. }
        | Check::CadMatches { machine, path, .. } = check
        {
            let machine = machine.as_deref().unwrap_or(&instance.procedure.machine);
            let content = world.runtime().read_file(machine, path).map(|b| {
                String::from_utf8_lossy(&b)
                    .chars()
                    .take(2000)
                    .collect::<String>()
            });
            out += &format!("== {machine}:{path}\n{content:?}\n");
        }
    }
    out
}
fn gui() -> ProjectOptions {
    ProjectOptions::default()
}

#[test]
fn procedures_verify_on_screen_and_through_tools() {
    for (folder, procedure) in procedures() {
        for seed in [1, 2, 3] {
            let instance = procedure.instantiate(seed, &BTreeMap::new()).unwrap();
            let (mut world, sessions) = staged(&folder, &instance);
            assert!(
                !world.verify(&instance).passed,
                "{} seed {seed}: already verified before anything was done",
                procedure.id
            );
            let shown = world.project_team(&sessions, &instance, gui()).unwrap();
            assert!(
                shown.completed && shown.verification.passed,
                "{} seed {seed} on screen: {:?}\n{}",
                procedure.id,
                shown.failure,
                checked_files(&world, &instance)
            );
            assert_eq!(shown.counts.tool_actions, 0);
            assert!(shown.steps.iter().all(|s| s.success));

            let (mut world, sessions) = staged(&folder, &instance);
            let tools = ProjectOptions {
                mode: Mode::Tool,
                ..gui()
            };
            let called = world.project_team(&sessions, &instance, tools).unwrap();
            assert!(
                called.completed && called.verification.passed,
                "{} seed {seed} through tools: {:?}",
                procedure.id,
                called.failure
            );
            assert_eq!(
                called.counts.pointer_actions + called.counts.keyboard_actions,
                0
            );
            assert!(called.counts.actions < shown.counts.actions);
        }
    }
}

#[test]
fn leaving_out_a_phase_that_changes_state_fails_verification() {
    for (folder, procedure) in procedures() {
        let instance = procedure.instantiate(11, &BTreeMap::new()).unwrap();
        for (index, phase) in instance.procedure.phases.iter().enumerate() {
            if phase.tool.is_empty() {
                continue;
            }
            let mut broken = instance.clone();
            broken.procedure.phases.remove(index);
            let (mut world, sessions) = staged(&folder, &broken);
            let shown = world.project_team(&sessions, &broken, gui()).unwrap();
            assert!(
                !shown.verification.passed,
                "{} verified without phase {index} ({})",
                procedure.id, phase.label
            );
        }
    }
}

#[test]
fn a_demonstration_replays_from_its_actions_alone() {
    for (folder, procedure) in procedures() {
        let instance = procedure.instantiate(5, &BTreeMap::new()).unwrap();
        let (mut world, sessions) = staged(&folder, &instance);
        let shown = world.project_team(&sessions, &instance, gui()).unwrap();
        assert!(
            shown.verification.passed,
            "{}: {:?}",
            procedure.id, shown.failure
        );

        let (mut replay, _) = staged(&folder, &instance);
        for step in &shown.steps {
            if step.advance > 0 {
                replay.advance(step.advance).unwrap();
                continue;
            }
            let result = replay
                .step(&step.session, vec![step.action.clone()])
                .unwrap();
            assert!(result.outcomes[0].success);
        }
        assert_eq!(
            replay.state_hash().unwrap(),
            shown.state_hash,
            "{}",
            procedure.id
        );
        assert!(replay.verify_with(&instance, &shown.captures).passed);
    }
}

#[test]
fn one_gesture_is_performed_on_the_screen_and_recorded() {
    use computerworld::procedure::{Gesture, Target};
    let text = std::fs::read_to_string(format!("{WORLDS}/agent-desktop/world.json")).unwrap();
    let mut world = World::from_json(&text, 7).unwrap();
    let config: EnvironmentConfig = serde_json::from_value(serde_json::json!({
        "actor": "ada", "machines": ["workstation"],
        "actions": ["application.v1", "pointer.v1", "keyboard.v1"],
        "observations": ["semantic.v1"],
    }))
    .unwrap();
    let session = world.environment(config).unwrap();
    let files = Gesture::Click {
        target: Target {
            interaction: Some("shell:launch:files".into()),
            ..Target::default()
        },
        at: None,
    };
    let steps = world.perform(&session, &files, gui()).unwrap();
    assert_eq!(steps.len(), 1);
    assert!(steps[0].success);
    assert_eq!(steps[0].action.family, "pointer.v1");
    let scene = world.scene(&session, 1280, 800).unwrap();
    assert!(scene.windows.iter().any(|w| w.app == "files"));
    // A target that is not on screen takes no step and says why.
    let missing = Gesture::Click {
        target: Target {
            label: Some("No such control".into()),
            ..Target::default()
        },
        at: None,
    };
    let error = world.perform(&session, &missing, gui()).unwrap_err();
    assert!(error.message.contains("No such control"), "{error}");
}

#[test]
fn binary_files_are_seeded_written_and_checked_exactly() {
    let procedure = Procedure::from_json(
        r#"{"id":"binary","instruction":"copy","machine":"workstation",
            "setup":[{"path":"/home/ada/in/pixel.bin","base64":"AAEC/w=="}],
            "phases":[{"label":"Export","gui":[],"tool":[
                {"family":"filesystem.v1","op":"mkdir","payload":{"path":"/home/ada/out/deep"}},
                {"family":"filesystem.v1","op":"write","payload":{"path":"/home/ada/out/deep/pixel.bin","bytes":[0,1,2,255]}}]}],
            "verify":[{"file_sha256":{"path":"/home/ada/out/deep/pixel.bin",
                "sha256":"replaced below"}}]}"#,
    )
    .unwrap();
    let mut instance = procedure.instantiate(1, &BTreeMap::new()).unwrap();
    let (mut world, sessions) =
        staged(&PathBuf::from(format!("{WORLDS}/agent-desktop")), &instance);
    assert_eq!(
        world
            .runtime()
            .read_file("workstation", "/home/ada/in/pixel.bin")
            .unwrap(),
        vec![0, 1, 2, 255]
    );
    let tools = ProjectOptions {
        mode: Mode::Tool,
        ..gui()
    };
    // The hash of the four bytes, computed rather than trusted from the literal above.
    use sha2::Digest;
    let digest: String = sha2::Sha256::digest([0u8, 1, 2, 255])
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if let Check::FileSha256 { sha256, .. } = &mut instance.procedure.verify[0] {
        *sha256 = digest;
    }
    assert!(!world.verify(&instance).passed);
    let done = world.project_team(&sessions, &instance, tools).unwrap();
    assert!(
        done.completed && done.verification.passed,
        "{:?}",
        done.failure
    );
}

#[test]
fn a_wait_lets_world_time_pass_and_replays() {
    let procedure = Procedure::from_json(
        r#"{"id":"wait","instruction":"wait a day","machine":"workstation",
            "phases":[{"label":"Wait for the office","gui":[{"do":"wait","days":1,"hours":2}],
                "tool":[{"family":"clock","op":"advance","payload":{"days":1,"hours":2}}]}],
            "verify":[]}"#,
    )
    .unwrap();
    let instance = procedure.instantiate(1, &BTreeMap::new()).unwrap();
    let folder = PathBuf::from(format!("{WORLDS}/agent-desktop"));
    let (mut world, sessions) = staged(&folder, &instance);
    let before = world.tick();
    let shown = world.project_team(&sessions, &instance, gui()).unwrap();
    let day = 26 * 3_600 * 1_000_000;
    assert!(shown.completed, "{:?}", shown.failure);
    assert_eq!(world.tick() - before, day);
    assert_eq!(shown.counts.waited, day);
    assert_eq!(shown.steps.len(), 1);
    assert_eq!(shown.steps[0].advance, day);
    let (mut replay, _) = staged(&folder, &instance);
    replay.advance(shown.steps[0].advance).unwrap();
    assert_eq!(replay.state_hash().unwrap(), shown.state_hash);
    // The tool path passes the same time through a clock advance call.
    let (mut world, sessions) = staged(&folder, &instance);
    let tools = ProjectOptions {
        mode: Mode::Tool,
        ..gui()
    };
    let called = world.project_team(&sessions, &instance, tools).unwrap();
    assert!(called.completed, "{:?}", called.failure);
    assert_eq!(called.steps[0].advance, day);
    assert_eq!(called.state_hash, shown.state_hash);
}

#[test]
fn a_captured_value_carries_into_later_phases_and_checks() {
    // The first phase writes a ticket number the procedure cannot know in advance; the
    // second phase reads it back through a capture and writes it somewhere else, and
    // the checks name it the same way.
    let procedure = Procedure::from_json(
        r#"{"id":"capture","instruction":"copy the ticket","machine":"workstation",
            "params":[{"name":"n","kind":"integer","min":100,"max":999}],
            "phases":[
              {"label":"File the ticket","gui":[],"tool":[
                {"family":"filesystem.v1","op":"write","payload":{"path":"/home/ada/ticket.txt","content":"queue\nTicket OPS-{{n}} opened.\n"}}],
               "capture":[{"name":"ticket","from":{"file":{"path":"/home/ada/ticket.txt"}},"pattern":"Ticket {{?}} opened"}]},
              {"label":"Note it","gui":[],"tool":[
                {"family":"filesystem.v1","op":"write","payload":{"path":"/home/ada/note.txt","content":"see {{@ticket}}"}}],
               "capture":[{"name":"whole","from":{"file":{"path":"/home/ada/note.txt"}}}]}],
            "verify":[{"file_equals":{"path":"/home/ada/note.txt","content":"see {{@ticket}}"}},
                      {"file_contains":{"path":"/home/ada/note.txt","text":"{{@whole}}"}}]}"#,
    )
    .unwrap();
    let instance = procedure.instantiate(4, &BTreeMap::new()).unwrap();
    let n = &instance.values["n"];
    let folder = PathBuf::from(format!("{WORLDS}/agent-desktop"));
    let (mut world, sessions) = staged(&folder, &instance);
    let tools = ProjectOptions {
        mode: Mode::Tool,
        ..gui()
    };
    let done = world.project_team(&sessions, &instance, tools).unwrap();
    assert!(done.completed, "{:?}", done.failure);
    assert_eq!(done.captures["ticket"], format!("OPS-{n}"));
    assert_eq!(done.captures["whole"], format!("see OPS-{n}"));
    assert!(done.verification.passed, "{:?}", done.verification);
    // Without the captures the checks cannot be resolved, so they fail.
    assert!(!world.verify(&instance).passed);
    assert!(world.verify_with(&instance, &done.captures).passed);

    // A capture whose source holds nothing stops the procedure at that phase.
    let mut broken = instance.clone();
    broken.procedure.phases.remove(0);
    let (mut world, sessions) = staged(&folder, &broken);
    let stopped = world.project_team(&sessions, &broken, tools).unwrap();
    assert!(!stopped.completed && !stopped.verification.passed);
    assert!(
        stopped.failure.as_deref().unwrap_or("").contains("ticket"),
        "{:?}",
        stopped.failure
    );
}

#[test]
fn a_find_check_and_a_state_capture_select_by_content() {
    // The world's computers, found by what they hold rather than by their position.
    let procedure = Procedure::from_json(
        r#"{"id":"find","instruction":"look","machine":"workstation",
            "phases":[{"label":"Look","gui":[],"tool":[],
               "capture":[{"name":"at","from":{"state":{"pointer":"/definition/computers","where":{"/id":"workstation"}}}},
                          {"name":"user","from":{"state":{"pointer":"/definition/computers","where":{"/id":"work{{*}}"},"field":"/user"}}}]}],
            "verify":[{"find":{"pointer":"/definition/computers","where":{"/id":"{{*}}"}}},
                      {"find":{"pointer":"/definition/computers","where":{"/id":"workstation","/user":"{{@user}}"},"count":1}},
                      {"find":{"pointer":"/definition/computers","where":{"/id":"no such machine"},"count":0}},
                      {"state":{"Equals":{"pointer":"/definition/computers/{{@at}}/id","value":"workstation"}}}]}"#,
    )
    .unwrap();
    let instance = procedure.instantiate(1, &BTreeMap::new()).unwrap();
    let folder = PathBuf::from(format!("{WORLDS}/agent-desktop"));
    let (mut world, sessions) = staged(&folder, &instance);
    let tools = ProjectOptions {
        mode: Mode::Tool,
        ..gui()
    };
    let done = world.project_team(&sessions, &instance, tools).unwrap();
    assert!(done.completed, "{:?}", done.failure);
    let computers = &world.definition().computers;
    let at = computers
        .iter()
        .position(|c| c.id == "workstation")
        .unwrap();
    assert_eq!(done.captures["at"], at.to_string());
    assert_eq!(done.captures["user"], computers[at].user);
    assert!(done.verification.passed, "{:?}", done.verification);
    assert!(!world.verify(&instance).passed);
}

#[test]
fn procedures_compose_in_one_shared_world() {
    // Each of these makes records a fresh world would number the same way: the
    // requisition and the incident both open OPS-2, the meeting request and the invoice
    // both send mail-12 and mail-13, both changes open pull request 21. Run back to back
    // in one company-2026, each names what it made through its captures and verifies.
    let folder = PathBuf::from(format!("{WORLDS}/company-2026"));
    let chain = [
        "ops-purchase-requisition",
        "ops-incident-response",
        "ops-meeting-request",
        "ops-vendor-invoice",
        "swe-atlas-clock-tests",
        "swe-atlas-contributing-rule",
    ];
    let all = procedures_in(&folder);
    let mut world: Option<World> = None;
    let mut done: Vec<(Instance, BTreeMap<String, String>)> = Vec::new();
    for id in chain {
        let procedure = all
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("no procedure {id}"));
        let instance = procedure.instantiate(2, &BTreeMap::new()).unwrap();
        let world = world.get_or_insert_with(|| {
            let text = std::fs::read_to_string(folder.join("world.json")).unwrap();
            let mut definition: computerworld::WorldDefinition =
                serde_json::from_str(&text).unwrap();
            // The machines' desktops, as the last procedure to name each sets it.
            for p in chain
                .iter()
                .filter_map(|id| all.iter().find(|p| p.id == *id))
            {
                p.prepare(&mut definition);
            }
            World::new(definition, 7).unwrap()
        });
        let sessions = sessions_for(world, &instance);
        // Whoever acts closes what the last procedure left open, as a person would.
        for session in sessions.values() {
            let machine = world
                .interfaces()
                .session(session)
                .unwrap()
                .focused_machine
                .clone();
            let windows: Vec<u64> = world
                .scene(session, 1280, 800)
                .unwrap()
                .windows
                .iter()
                .map(|w| w.id)
                .collect();
            for window in windows {
                let close = computerworld::ActionEnvelope::new(
                    "application.v1",
                    "close",
                    machine.clone(),
                    serde_json::json!({"window": window}),
                );
                world.step(session, vec![close]).unwrap();
            }
        }
        let shown = world.project_team(&sessions, &instance, gui()).unwrap();
        assert!(
            shown.completed && shown.verification.passed,
            "{id} after {:?}: {:?} {:?}",
            done.iter()
                .map(|(i, _)| &i.procedure.id)
                .collect::<Vec<_>>(),
            shown.failure,
            shown
                .verification
                .checks
                .iter()
                .filter(|c| !c.passed)
                .collect::<Vec<_>>()
        );
        done.push((instance, shown.captures));
    }
    // Each earlier procedure still verifies at the end, against its own records.
    let world = world.unwrap();
    for (instance, captures) in &done {
        let verdict = world.verify_with(instance, captures);
        let failed: Vec<_> = verdict.checks.iter().filter(|c| !c.passed).collect();
        assert!(
            failed.is_empty(),
            "{} at the end: {failed:?}",
            instance.procedure.id
        );
    }
    // The two changes opened different pull requests, and the two tickets differ.
    let captured = |id: &str, name: &str| {
        done.iter()
            .find(|(i, _)| i.procedure.id == id)
            .map(|(_, c)| c[name].clone())
            .unwrap()
    };
    assert_ne!(
        captured("swe-atlas-clock-tests", "pr"),
        captured("swe-atlas-contributing-rule", "pr")
    );
    assert_ne!(
        captured("ops-purchase-requisition", "issue"),
        captured("ops-incident-response", "issue")
    );
}
fn procedures_in(folder: &Path) -> Vec<Procedure> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder.join("procedures")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|x| x == "json") {
            let text = std::fs::read_to_string(&path).unwrap();
            found.push(Procedure::from_json(&text).unwrap());
        }
    }
    found
}
