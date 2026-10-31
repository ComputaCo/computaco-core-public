//! Editing a field in a web page through the keyboard, as a person does: Ctrl+A selects
//! the field, Shift with the arrows and Home/End grows a selection, typing replaces it,
//! and Ctrl+C/X/V move text through the machine's clipboard — the same clipboard the
//! desktop's own applications use.
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
            observations: vec!["browser.v1".into()],
            action_budget: 1 << 20,
        })
        .unwrap();
    (world, session)
}
fn act(world: &mut World, session: &str, channel: &str, op: &str, payload: Value) {
    let result = world
        .step(
            session,
            vec![ActionEnvelope::new(channel, op, MACHINE, payload)],
        )
        .unwrap();
    assert!(result.outcomes[0].success, "{op}: {:?}", result.outcomes[0]);
}
fn key(world: &mut World, session: &str, key: &str) {
    act(world, session, "keyboard.v1", "key", json!({ "key": key }));
}
fn typed(world: &mut World, session: &str, text: &str) {
    act(
        world,
        session,
        "keyboard.v1",
        "type",
        json!({ "text": text }),
    );
}
fn field(world: &World, session: &str) -> String {
    let s = world.interfaces().session(session).unwrap();
    s.machines[MACHINE].browser.tab().fields["q"].clone()
}
fn clipboard(world: &World, session: &str) -> Option<String> {
    let s = world.interfaces().session(session).unwrap();
    s.machines[MACHINE].desktop.clipboard_text.clone()
}

#[test]
fn ctrl_a_selects_a_field_and_the_clipboard_carries_text_between_fields() {
    let (mut world, session) = world();
    act(
        &mut world,
        &session,
        "browser.v1",
        "navigate",
        json!({"url":"http://google.com/"}),
    );
    act(
        &mut world,
        &session,
        "browser.v1",
        "click",
        json!({"id":"q"}),
    );
    typed(&mut world, &session, "atlas maps");
    // Ctrl+A was an invalid action before the field had a selection.
    key(&mut world, &session, "Ctrl+a");
    key(&mut world, &session, "Ctrl+c");
    assert_eq!(clipboard(&world, &session).as_deref(), Some("atlas maps"));
    typed(&mut world, &session, "atlas");
    assert_eq!(field(&world, &session), "atlas");
    // Shift+Home selects what was typed; Ctrl+X takes it out and onto the clipboard.
    key(&mut world, &session, "Shift+Home");
    key(&mut world, &session, "Ctrl+x");
    assert_eq!(field(&world, &session), "");
    assert_eq!(clipboard(&world, &session).as_deref(), Some("atlas"));
    // Paste twice, select the second word with Ctrl+Shift+ArrowLeft and replace it.
    key(&mut world, &session, "Ctrl+v");
    typed(&mut world, &session, " ");
    key(&mut world, &session, "Ctrl+v");
    assert_eq!(field(&world, &session), "atlas atlas");
    key(&mut world, &session, "Ctrl+Shift+ArrowLeft");
    typed(&mut world, &session, "maps");
    assert_eq!(field(&world, &session), "atlas maps");
    // A copy with nothing selected leaves the clipboard as it was.
    key(&mut world, &session, "Ctrl+c");
    assert_eq!(clipboard(&world, &session).as_deref(), Some("atlas"));
    key(&mut world, &session, "Enter");
    let url = world.observe(&session).unwrap().channels["browser.v1"][MACHINE]["url"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(url, "http://google.com/search?q=atlas+maps");
}
