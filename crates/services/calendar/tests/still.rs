//! Renders the seeded Google Calendar week through the engine to
//! `research/studies/site-stills/google-calendar.png`. Ignored by default: it is a picture for the
//! record, not a gate. `cargo test -p cw-service-calendar --test still -- --ignored`.
use cw_protocol::HttpRequest;
use cw_sdk::{Service, ServiceContext};
use cw_service_calendar::{CalendarService, DAY_US, HOUR_US};
use cw_web::Viewport;
use serde_json::Value;
use std::path::PathBuf;

const WORLD: &str = include_str!("../../../../worlds/internet/world.json");

fn render(html: &str, file: &str, viewport: Viewport) {
    let out = cw_service_common::testkit::still_png(html, viewport);
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../research/studies/site-stills")
        .join(file);
    std::fs::write(&target, out).unwrap_or_else(|e| panic!("write {}: {e}", target.display()));
    println!("wrote {}", target.display());
}

#[test]
#[ignore]
fn google_calendar_stills() {
    let world: Value = serde_json::from_str(WORLD).unwrap();
    let service = world["services"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "google-calendar")
        .unwrap();
    // Wednesday 23 September 2026, 10:30: the busiest seeded week, with the red line on screen.
    let ctx = ServiceContext {
        actor: "alice".into(),
        source: "alice-mac".into(),
        tick: 6 * DAY_US + HOUR_US + HOUR_US / 2,
        seed: 1,
        instance: "google-calendar".into(),
    };
    let mut state = CalendarService
        .initialize(service["initial_state"].clone(), &ctx)
        .unwrap();
    let viewport = Viewport {
        width: 1280,
        height: 800,
        scale: 1,
        zoom: 100,
    };
    for (url, file) in [
        ("http://calendar.google.com/", "google-calendar.png"),
        (
            "http://calendar.google.com/?day=6&event=design-crit-scene-graph",
            "google-calendar-event.png",
        ),
        (
            "http://calendar.google.com/?view=month&day=6",
            "google-calendar-month.png",
        ),
        (
            "http://calendar.google.com/?day=12",
            "google-calendar-allday.png",
        ),
    ] {
        let response = CalendarService
            .handle(&mut state, &ctx, &HttpRequest::get(url))
            .unwrap();
        assert_eq!(response.status, 200);
        render(&String::from_utf8(response.body).unwrap(), file, viewport);
    }
}
