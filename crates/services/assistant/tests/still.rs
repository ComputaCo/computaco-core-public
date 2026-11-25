//! Renders chatgpt.com and claude.ai through the engine to `research/studies/site-stills/`:
//! `openai.png` and `anthropic.png` (the home pages), and `openai-chat.png` and
//! `anthropic-chat.png` (a seeded conversation). Ignored by default: pictures for the
//! record, not a gate. `cargo test -p cw-service-assistant --test still -- --ignored`.
use cw_protocol::HttpRequest;
use cw_sdk::{Service, ServiceContext};
use cw_service_assistant::{AssistantService, AssistantState};
use cw_web::Viewport;
use serde_json::Value;
use std::path::PathBuf;

static SITES: std::sync::LazyLock<[(&'static str, &'static str, &'static str); 2]> =
    std::sync::LazyLock::new(|| {
        [
            (
                "openai",
                "http://chatgpt.com",
                cw_service_common::reference::reference_site_json("openai"),
            ),
            (
                "anthropic",
                "http://claude.ai",
                cw_service_common::reference::reference_site_json("anthropic"),
            ),
        ]
    });

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
fn assistant_home_and_conversation_stills() {
    let viewport = Viewport {
        width: 1280,
        height: 800,
        scale: 1,
        zoom: 100,
    };
    for (site, origin, source) in *SITES {
        let seed: Value = serde_json::from_str(source).unwrap();
        let mut ctx = ServiceContext {
            actor: "alice".into(),
            source: "alice-mac".into(),
            tick: 1,
            seed: 1,
            instance: site.into(),
        };
        let mut state = AssistantService
            .initialize(seed["initial_state"].clone(), &ctx)
            .unwrap();
        let typed: AssistantState = serde_json::from_value(state.clone()).unwrap();
        // The person with the most history on this site, so the sidebar shows what it is for.
        let owner = typed
            .conversations
            .values()
            .map(|c| c.owner.clone())
            .max_by_key(|o| typed.mine(o).count())
            .unwrap_or_else(|| "alice".into());
        ctx.actor = owner.clone();
        let chat = typed
            .mine(&owner)
            .max_by_key(|c| c.messages.len())
            .map(|c| c.id.clone());
        let mut pages = vec![(format!("{origin}/"), format!("{site}.png"))];
        if let Some(id) = chat {
            pages.push((format!("{origin}/c/{id}"), format!("{site}-chat.png")));
        }
        for (url, file) in pages {
            let response = AssistantService
                .handle(&mut state, &ctx, &HttpRequest::get(url))
                .unwrap();
            assert_eq!(response.status, 200);
            render(&String::from_utf8(response.body).unwrap(), &file, viewport);
        }
    }
}
