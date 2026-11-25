//! Renders the Messages web view through the engine to `research/studies/site-stills/messages.png`.
//! Ignored by default: a picture for the record, not a gate.
//! `cargo test -p cw-service-messages --test still -- --ignored`.
use cw_protocol::HttpRequest;
use cw_sdk::{Service, ServiceContext};
use cw_service_messages::MessagesService;
use cw_web::Viewport;
use serde_json::Value;
use std::path::PathBuf;

fn render(html: &str, file: &str, viewport: Viewport) {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../research/studies/site-stills")
        .join(file);
    cw_service_common::testkit::render_still(html, &target, viewport);
}

#[test]
#[ignore]
fn messages_stills() {
    let raw =
        std::fs::read_to_string("../../../worlds/company-2026/services/messages/service.json")
            .unwrap();
    let site: Value = serde_json::from_str(&raw).unwrap();
    let ctx = ServiceContext {
        actor: "alice".into(),
        source: "alice-mac".into(),
        tick: 60,
        seed: 1,
        instance: "messages".into(),
    };
    let mut state = MessagesService
        .initialize(site["initial_state"].clone(), &ctx)
        .unwrap();
    let viewport = Viewport {
        width: 1280,
        height: 800,
        scale: 1,
        zoom: 100,
    };
    for (url, file) in [
        (
            "http://messages.internal/conversations/+14155550100|+14155550101|+14155550102",
            "messages.png",
        ),
        ("http://messages.internal/", "messages-inbox.png"),
        (
            "http://messages.internal/conversations/+14155550100|+14155550199",
            "messages-sms.png",
        ),
    ] {
        let response = MessagesService
            .handle(&mut state, &ctx, &HttpRequest::get(url))
            .unwrap();
        assert_eq!(response.status, 200);
        render(&String::from_utf8(response.body).unwrap(), file, viewport);
    }
}
