//! Renders the Google home page through the engine to `research/studies/google-ceiling/engine.png`,
//! the companion of `mock.html` (Chromium) in that directory. Ignored by default: it is a
//! picture for the record, not a gate. `cargo test -p cw-service-search --test still -- --ignored`.
use cw_protocol::{HttpRequest, ServiceDefinition};
use cw_sdk::{Service, ServiceContext};
use cw_service_search::SearchService;
use cw_web::Viewport;
use serde_json::Value;
use std::path::PathBuf;

fn render(html: &str, file: &str, viewport: Viewport) {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../research/studies/google-ceiling")
        .join(file);
    cw_service_common::testkit::render_still(html, &target, viewport);
}

#[test]
#[ignore]
fn google_home_and_results_stills() {
    let services: Vec<ServiceDefinition> = cw_service_common::reference::reference_services();
    let service = services.iter().find(|s| s.id == "google-search").unwrap();
    let ctx = ServiceContext {
        actor: "alice".into(),
        source: "alice-mac".into(),
        tick: 1,
        seed: 1,
        instance: "google-search".into(),
    };
    let mut state = SearchService
        .initialize_in(service.initial_state.clone(), &ctx, &services)
        .unwrap();
    // The still matches the mock: a first visit, no history of alice's under the box.
    state["history"] = Value::Object(Default::default());
    let viewport = Viewport {
        width: 1280,
        height: 800,
        scale: 1,
        zoom: 100,
    };
    for (url, file) in [
        ("http://google.com/", "engine.png"),
        ("http://google.com/search?q=atlas", "engine-results.png"),
    ] {
        let response = SearchService
            .handle(&mut state, &ctx, &HttpRequest::get(url))
            .unwrap();
        assert_eq!(response.status, 200);
        render(&String::from_utf8(response.body).unwrap(), file, viewport);
    }
}
