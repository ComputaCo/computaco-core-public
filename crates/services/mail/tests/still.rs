//! Renders each webmail's main page through the engine to `research/studies/site-stills/<site>.png`.
//! Ignored by default: pictures for the record, not a gate.
//! `cargo test -p cw-service-mail --test still -- --ignored`.
use cw_protocol::HttpRequest;
use cw_sdk::{Service, ServiceContext};
use cw_service_mail::MailService;
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
fn webmail_stills() {
    let viewport = Viewport {
        width: 1280,
        height: 800,
        scale: 1,
        zoom: 100,
    };
    for (name, actor, pages) in [
        (
            "google-mail",
            "alice",
            vec![
                ("/", ""),
                ("/?folder=inbox&thread=mail-1", "-thread"),
                ("/?folder=inbox&compose=1", "-compose"),
            ],
        ),
        (
            "outlook-mail",
            "bob",
            vec![
                ("/?folder=inbox&thread=mail-1", ""),
                ("/?folder=inbox&compose=1", "-compose"),
            ],
        ),
        (
            "mail-com",
            "carol",
            vec![("/?folder=inbox&thread=mail-2", ""), ("/", "-list")],
        ),
    ] {
        let ctx = ServiceContext {
            actor: actor.into(),
            source: "pc".into(),
            tick: 1,
            seed: 1,
            instance: name.into(),
        };
        let mut state = MailService
            .initialize(site(name)["initial_state"].clone(), &ctx)
            .unwrap();
        for (path, suffix) in pages {
            let response = MailService
                .handle(
                    &mut state,
                    &ctx,
                    &HttpRequest::get(format!("http://mail{path}")),
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

/// Renders `$SCRATCH_HTML` to `$SCRATCH_PNG`: a bench for isolating an engine behaviour.
#[test]
#[ignore]
fn scratch() {
    let (Ok(html), Ok(png)) = (std::env::var("SCRATCH_HTML"), std::env::var("SCRATCH_PNG")) else {
        return;
    };
    let width = std::env::var("SCRATCH_W")
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or(640);
    let viewport = Viewport {
        width,
        height: width * 5 / 8,
        scale: 1,
        zoom: 100,
    };
    render(&std::fs::read_to_string(html).unwrap(), &png, viewport);
}
