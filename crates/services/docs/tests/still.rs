//! Renders docs.google.com and notion.so through the engine to `research/studies/site-stills/`.
//! Ignored by default: pictures for the record, not a gate.
//! `cargo test -p cw-service-docs --test still -- --ignored`.
use cw_protocol::HttpRequest;
use cw_sdk::{Service, ServiceContext};
use cw_service_docs::DocsService;
use cw_web::Viewport;
use serde_json::Value;
use std::path::PathBuf;

fn render(html: &str, file: &str, viewport: Viewport) {
    let out = cw_service_common::testkit::still_png(html, viewport);
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../research/studies/site-stills")
        .join(file);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, out).unwrap_or_else(|e| panic!("write {}: {e}", target.display()));
    println!("wrote {}", target.display());
}
fn site(name: &str) -> Value {
    let path = cw_service_common::reference::reference_site_path(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
#[ignore]
fn docs_stills() {
    let viewport = Viewport {
        width: 1280,
        height: 800,
        scale: 1,
        zoom: 100,
    };
    for (name, pages) in [
        (
            "google-docs",
            vec![
                ("/", ""),
                ("/documents/atlas-launch", "-document"),
                ("/documents/q3-metrics", "-sheet"),
                ("/documents/atlas-launch-review", "-slides"),
            ],
        ),
        (
            "notion",
            vec![("/documents/onboarding-checklist", ""), ("/", "-home")],
        ),
    ] {
        let ctx = ServiceContext {
            actor: "alice".into(),
            source: "pc".into(),
            tick: 1,
            seed: 1,
            instance: name.into(),
        };
        let mut state = DocsService
            .initialize(site(name)["initial_state"].clone(), &ctx)
            .unwrap();
        for (path, suffix) in pages {
            let response = DocsService
                .handle(
                    &mut state,
                    &ctx,
                    &HttpRequest::get(format!("http://docs{path}")),
                )
                .unwrap();
            assert_eq!(response.status, 200, "{name} {path}");
            let html = String::from_utf8(response.body).unwrap();
            if let Ok(dir) = std::env::var("DUMP_HTML") {
                std::fs::write(format!("{dir}/{name}{suffix}.html"), &html).unwrap();
            }
            render(&html, &format!("{name}{suffix}.png"), viewport);
        }
    }
}
