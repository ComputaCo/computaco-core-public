//! Renders the three sites' main pages (and one article page each) through the engine to
//! `research/studies/site-stills/`. Ignored by default: pictures for the record, not a gate.
//! `cargo test -p cw-service-wiki --test still -- --ignored`.
use cw_protocol::HttpRequest;
use cw_sdk::{Service, ServiceContext};
use cw_service_wiki::WikiService;
use cw_web::Viewport;
use serde_json::Value;
use std::path::PathBuf;

fn render(html: &str, path: &str, viewport: Viewport) {
    let out = cw_service_common::testkit::still_png(html, viewport);
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../research/studies/site-stills")
        .join(path);
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, out).unwrap_or_else(|e| panic!("write {}: {e}", target.display()));
    println!("wrote {}", target.display());
}

#[test]
#[ignore]
fn wikipedia_imdb_and_archive_stills() {
    let sites = [
        (
            "wikipedia",
            cw_service_common::reference::reference_site_json("wikipedia"),
            "wikipedia.org",
            "Deterministic_simulation",
        ),
        (
            "imdb",
            cw_service_common::reference::reference_site_json("imdb"),
            "imdb.com",
            "Northbound_Signal",
        ),
        (
            "archive",
            cw_service_common::reference::reference_site_json("archive"),
            "archive.org",
            "Cavern_Runner_98",
        ),
    ];
    let viewport = Viewport {
        width: 1280,
        height: 800,
        scale: 1,
        zoom: 100,
    };
    for (name, seed, host, article) in sites {
        let site: Value = serde_json::from_str(seed).unwrap();
        let ctx = ServiceContext {
            actor: "alice".into(),
            source: "alice-mac".into(),
            tick: 1,
            seed: 1,
            instance: name.into(),
        };
        let mut state = WikiService
            .initialize(site["initial_state"].clone(), &ctx)
            .unwrap();
        // The main page is the still of record; the article page is where the skin earns its name.
        let shots = [
            (format!("http://{host}/"), format!("{name}.png")),
            (
                format!("http://{host}/wiki/{article}"),
                format!("{name}-article.png"),
            ),
        ];
        for (url, file) in shots {
            let response = WikiService
                .handle(&mut state, &ctx, &HttpRequest::get(&url))
                .unwrap();
            assert_eq!(response.status, 200);
            render(&String::from_utf8(response.body).unwrap(), &file, viewport);
        }
    }
}
