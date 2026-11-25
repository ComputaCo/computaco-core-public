//! Renders chat.internal through the engine to `research/studies/site-stills/chat.png`.
//! Ignored by default: a picture for the record, not a gate.
//! `cargo test -p cw-service-chat --test still -- --ignored`.
use cw_protocol::HttpRequest;
use cw_sdk::{Service, ServiceContext};
use cw_service_chat::ChatService;
use cw_web::Viewport;
use serde_json::json;
use std::path::PathBuf;

fn render(html: &str, path: &str, viewport: Viewport) {
    let out = cw_service_common::testkit::still_png(html, viewport);
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../research/studies/site-stills")
        .join(path);
    std::fs::write(&target, out).unwrap_or_else(|e| panic!("write {}: {e}", target.display()));
    println!("wrote {}", target.display());
}

#[test]
#[ignore]
fn chat_stills() {
    let ctx = ServiceContext {
        actor: "alice".into(),
        source: "alice-mac".into(),
        tick: 40,
        seed: 1,
        instance: "chat".into(),
    };
    // The shipped seed is one empty channel, so the picture uses a small conversation.
    let mut state = ChatService
        .initialize(
            json!({"next_id":6,"channels":{
                "general":{"title":"General","members":["alice","bob","carol","admin"],"messages":[]},
                "eng":{"title":"Engineering","members":["alice","bob","carol","admin"],"messages":[
                    {"id":"chat-1","author":"bob","text":"BFS path test fails on Windows only. Anyone seen this before?","time":30,"reactions":{"eyes":["alice","carol"]}},
                    {"id":"chat-2","author":"bob","text":"Linux and macOS runners are green.","time":30,"reactions":{}},
                    {"id":"chat-3","author":"alice","text":"Priya answered it on Stack Overflow: the traversal depends on HashMap order.","time":31,"reactions":{"tada":["carol"]}},
                    {"id":"chat-4","author":"carol","text":"I'll switch it to a BTreeMap and re-run CI.","time":33,"reactions":{},"parent":"chat-1"},
                    {"id":"chat-5","author":"admin","text":"Reminder: the launch review is at 2 today.","time":36,"reactions":{"+1":["alice","bob","carol"]}}]},
                "random":{"title":"Random","members":["alice","bob","carol"],"messages":[]}},
              "dms":{"alice|bob":{"title":"alice and bob","members":["alice","bob"],"messages":[]}}}),
            &ctx,
        )
        .unwrap();
    let viewport = Viewport {
        width: 1280,
        height: 800,
        scale: 1,
        zoom: 100,
    };
    for (url, file) in [
        ("http://chat.internal/channels/eng", "chat.png"),
        ("http://chat.internal/", "chat-home.png"),
    ] {
        let response = ChatService
            .handle(&mut state, &ctx, &HttpRequest::get(url))
            .unwrap();
        assert_eq!(response.status, 200);
        render(&String::from_utf8(response.body).unwrap(), file, viewport);
    }
}
