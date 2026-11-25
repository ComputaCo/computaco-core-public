//! Optional bundled service registry. The kernel does not depend on this crate.
//!
//! It carries the generic kinds (mail, chat, docs, calendar, search, a wiki, a forum, a
//! drive, messages, an assistant, static sites and node apps). Another crate adds kinds of
//! its own with [`extend`], and
//! [`registry`] then carries them too.
pub use cw_service_assistant as assistant;
pub use cw_service_calendar as calendar;
pub use cw_service_chat as chat;
pub use cw_service_docs as docs;
pub use cw_service_drive as drive;
pub use cw_service_forum as forum;
pub use cw_service_mail as mail;
pub use cw_service_messages as messages;
pub use cw_service_search as search;
pub use cw_service_static_site as static_site;
pub use cw_service_wiki as wiki;
/// This crate's kinds of the simulated public web; the content packages bind sites to these.
pub const WEB_KINDS: [&str; 5] = ["assistant", "drive", "forum", "search", "wiki"];
/// The company's internal messaging kinds: plain chat, and texting between handles.
pub const MESSAGING_KINDS: [&str; 2] = ["chat", "messages"];
/// This crate's kinds that gained an additive `skin`; each crate owns its own skin vocabulary.
pub const SKINNED_KINDS: [&str; 4] = ["calendar", "chat", "docs", "mail"];
/// Web kinds whose default `skin` is `plain`, i.e. the unthemed original rendering. A
/// themed page is what a *skinned* instance owes; plain deliberately has no theme.
pub const PLAIN_DEFAULT_KINDS: [&str; 1] = ["drive"];
/// Register this crate's kinds.
pub fn register(registry: &mut cw_sdk::Registry) -> cw_protocol::Result<()> {
    static_site::register(registry)?;
    mail::register(registry)?;
    chat::register(registry)?;
    messages::register(registry)?;
    docs::register(registry)?;
    calendar::register(registry)?;
    assistant::register(registry)?;
    drive::register(registry)?;
    forum::register(registry)?;
    search::register(registry)?;
    wiki::register(registry)?;
    cw_service_node_app::register(registry)?;
    Ok(())
}
/// What another crate registers: its kinds, added to every [`registry`] from now on.
pub type Extension = fn(&mut cw_sdk::Registry) -> cw_protocol::Result<()>;
static EXTENSIONS: std::sync::RwLock<Vec<Extension>> = std::sync::RwLock::new(Vec::new());
/// Add `register`'s kinds to every registry [`registry`] makes from now on. Adding the same
/// function twice changes nothing.
pub fn extend(register: Extension) {
    let mut all = EXTENSIONS.write().unwrap_or_else(|e| e.into_inner());
    if !all.iter().any(|f| std::ptr::fn_addr_eq(*f, register)) {
        all.push(register);
    }
}
/// This crate's kinds and every extension's. A registry is keyed by kind, so the order
/// kinds are added in changes nothing.
pub fn registry() -> cw_protocol::Result<cw_sdk::Registry> {
    let mut registry = cw_sdk::Registry::new();
    register(&mut registry)?;
    let extensions = EXTENSIONS.read().unwrap_or_else(|e| e.into_inner()).clone();
    for extend in extensions {
        extend(&mut registry)?;
    }
    Ok(registry)
}

/// What every kind owes, checked by this crate's tests and by an extension's.
#[cfg(any(test, feature = "checks"))]
#[doc(hidden)]
pub mod checks {
    pub fn context() -> cw_sdk::ServiceContext {
        cw_sdk::ServiceContext {
            actor: "alice".into(),
            source: "alice-mac".into(),
            tick: 0,
            seed: 1,
            instance: "site".into(),
        }
    }
    /// A web kind serves a valid themed page at `/` from empty seed state, so a site can be
    /// bound to a node and browsed before the package that owns its content has landed. A
    /// kind that answers with HTML (the engine's design notes) must parse, carry a
    /// title and pass the strict validator; the rest serve a valid themed page, or an
    /// unthemed one when `plain` is their default skin.
    pub fn serves_a_valid_page_from_empty_state(
        registry: &cw_sdk::Registry,
        kind: &str,
        plain: bool,
    ) {
        let service = registry.service(kind).unwrap();
        let mut state = service
            .initialize(serde_json::json!({}), &context())
            .unwrap();
        let response = service
            .handle(
                &mut state,
                &context(),
                &cw_protocol::HttpRequest::get("http://site.example/"),
            )
            .unwrap();
        assert_eq!(response.status, 200, "{kind}");
        if response
            .header("content-type")
            .is_some_and(|t| t.starts_with("text/html"))
        {
            let html = String::from_utf8(response.body).unwrap();
            cw_service_common::html::validate_strict(&html)
                .unwrap_or_else(|e| panic!("{kind}: {e}"));
            let dom = cw_web::html::parse(&html);
            let title = dom
                .descendants(cw_web::dom::Document::ROOT)
                .find(|n| dom.is(*n, "title"))
                .map(|n| dom.text_content(n))
                .unwrap_or_default();
            assert!(!title.trim().is_empty(), "{kind} serves an untitled page");
            return;
        }
        let page: cw_protocol::Page = serde_json::from_slice(&response.body).unwrap();
        page.validate().unwrap_or_else(|e| panic!("{kind}: {e}"));
        assert!(
            page.theme.is_some() || plain,
            "{kind} serves an unthemed page but does not default to a plain skin"
        );
    }
    /// Skins are additive: a default state carries no `skin` key, so worlds and checkpoints
    /// written before skins existed round-trip byte-identically, and a typo still fails loudly.
    pub fn plain_skin_is_invisible_and_unknown_skins_are_rejected(
        registry: &cw_sdk::Registry,
        kind: &str,
    ) {
        let ctx = context();
        let service = registry.service(kind).unwrap();
        let state = service.initialize(serde_json::json!({}), &ctx).unwrap();
        assert!(
            state.get("skin").is_none(),
            "{kind} serialises a default skin"
        );
        assert!(
            service
                .initialize(serde_json::json!({"skin": "nonesuch"}), &ctx)
                .is_err(),
            "{kind} accepts an unknown skin"
        );
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn registry_has_independent_optional_services() {
        let r = super::registry().unwrap();
        assert_eq!(r.service_kinds().count(), 12);
        for kind in super::MESSAGING_KINDS {
            assert!(r.service(kind).is_ok(), "{kind} must be registered");
        }
        assert!(r.service("static-site").is_ok());
        for kind in super::WEB_KINDS {
            assert!(r.service(kind).is_ok(), "{kind} must be registered");
        }
    }
    #[test]
    fn web_kinds_serve_a_valid_page_from_empty_state() {
        let r = super::registry().unwrap();
        for kind in super::WEB_KINDS {
            super::checks::serves_a_valid_page_from_empty_state(
                &r,
                kind,
                super::PLAIN_DEFAULT_KINDS.contains(&kind),
            );
        }
    }
    #[test]
    fn plain_skin_is_invisible_and_unknown_skins_are_rejected() {
        let r = super::registry().unwrap();
        for kind in super::SKINNED_KINDS {
            super::checks::plain_skin_is_invisible_and_unknown_skins_are_rejected(&r, kind);
        }
    }
}
