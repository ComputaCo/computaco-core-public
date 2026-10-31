//! Optional bundled service registry. The kernel does not depend on this crate.
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
/// The simulated public web's kinds; the content packages bind sites to these. Slack and
/// Discord are public sites in their own right, each with its own semantics.
pub const WEB_KINDS: [&str; 23] = [
    "ads",
    "agency",
    "assistant",
    "bank",
    "cardnet",
    "clearing",
    "discord",
    "drive",
    "forum",
    "geo",
    "hosting",
    "ledger",
    "media",
    "payments",
    "podcast",
    "postal",
    "press",
    "search",
    "shop",
    "slack",
    "social",
    "stock",
    "wiki",
];
/// The company's internal messaging kinds: plain chat, and texting between handles.
pub const MESSAGING_KINDS: [&str; 2] = ["chat", "messages"];
/// Existing kinds that gained an additive `skin`; each crate owns its own skin vocabulary.
pub const SKINNED_KINDS: [&str; 6] = ["calendar", "chat", "docs", "git", "issues", "mail"];
/// Web kinds whose default `skin` is `plain`, i.e. the unthemed original rendering. A
/// themed page is what a *skinned* instance owes; plain deliberately has no theme.
pub const PLAIN_DEFAULT_KINDS: [&str; 1] = ["drive"];
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
pub fn registry() -> cw_protocol::Result<cw_sdk::Registry> {
    let mut registry = cw_sdk::Registry::new();
    register(&mut registry)?;
    Ok(registry)
}
