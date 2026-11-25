//! Native applications that live in another crate and register here: any crate that carries
//! native applications of its own, as the base applications of this one are carried.
//!
//! A registered application is one more [`NativeApp`] variant, [`NativeApp::External`],
//! holding the application as an [`External`] trait object. It answers every method the
//! enum answers, and it serializes to the JSON its own type writes: an internally tagged
//! `{"app": "<tag>", …}` object, exactly what a variant of the enum writes. So a world, a
//! snapshot and a state hash are the same bytes whether an application is a variant of
//! the enum or registered here, and a snapshot taken in one build reads in the other.
//!
//! Registration is explicit and global: an embedding calls [`register`] (through its
//! crate's own install function) before it boots a world.
//! Rust has no code that runs before `main` on every target, Wasm included, so nothing
//! registers itself.
use super::declare::Declaration;
use super::{AppEnv, NativeApp, Painter};
use crate::desktop_scene::DesktopTheme;
use crate::AppEffect;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::RwLock;

/// One application carried by another crate. Each method is the `NativeApp` method of the
/// same name; the documentation of the method there says what it means.
#[allow(clippy::too_many_arguments)]
pub trait External: erased_serde::Serialize + std::fmt::Debug + Send + Sync + 'static {
    fn kind(&self) -> &'static str;
    fn title(&self, theme: DesktopTheme) -> String;
    fn document(&self) -> String;
    fn caption(&self) -> String;
    fn modified(&self) -> bool;
    fn text(&mut self, text: &str) -> Result<(), String>;
    fn key(&mut self, window: u64, key: &str, clock_us: u64) -> Result<Vec<AppEffect>, String>;
    fn click(&mut self, window: u64, target: &str, clock_us: u64)
        -> Result<Vec<AppEffect>, String>;
    fn http(
        &mut self,
        window: u64,
        tag: &str,
        status: u16,
        body: &str,
    ) -> Result<Vec<AppEffect>, String>;
    fn offline(&mut self, tag: &str, reason: &str);
    fn page(&self, page: &mut cw_protocol::Page);
    fn render(&self, p: &mut Painter, env: &AppEnv<'_>);
    fn image(&mut self, path: &str, width: u32, height: u32, rgba: Vec<u8>) -> Result<(), String>;
    fn image_failed(&mut self, path: &str, reason: &str);
    fn click_at(
        &mut self,
        window: u64,
        target: &str,
        dx: i32,
        dy: i32,
        clock_us: u64,
    ) -> Result<Vec<AppEffect>, String>;
    fn press_at(&mut self, target: &str, dx: i32, dy: i32) -> Result<(), String>;
    fn activate(
        &mut self,
        window: u64,
        target: &str,
        clock_us: u64,
    ) -> Result<Vec<AppEffect>, String>;
    fn text_effects(&mut self, window: u64, text: &str) -> Result<Vec<AppEffect>, String>;
    fn paste(&mut self, window: u64, text: &str) -> Result<Vec<AppEffect>, String>;
    fn dark_chrome(&self) -> bool;
    fn chrome(&self) -> Vec<(String, String)>;
    fn accepts_text(&self) -> bool;
    fn drags(&self, target: &str) -> bool;
    fn bytes(
        &mut self,
        window: u64,
        path: &str,
        result: Result<Vec<u8>, String>,
        clock_us: u64,
    ) -> Result<Vec<AppEffect>, String>;
    fn bytes_saved(
        &mut self,
        window: u64,
        path: &str,
        result: Result<(), String>,
    ) -> Result<Vec<AppEffect>, String>;
    fn pointer_button(&mut self, button: u8);
    fn takes_secondary(&self, target: &str) -> bool;
    fn wheel(&mut self, target: &str, x: i32, y: i32, wheel: crate::Wheel) -> Result<bool, String>;
    fn hovers(&self, target: &str) -> bool;
    fn hover(&mut self, window: u64, target: &str, x: i32, y: i32) -> Vec<AppEffect>;
    fn hover_cursor(&self, target: &str) -> &'static str;
    fn pointer_modifiers(&mut self, modifiers: u8);
    fn pointer(
        &mut self,
        window: u64,
        target: &str,
        phase: crate::PointerPhase,
        x: i32,
        y: i32,
        clock_us: u64,
    ) -> Result<Vec<AppEffect>, String>;
    fn listed(&mut self, entries: Vec<String>) -> Result<(), String>;
    fn image_saved(&mut self, window: u64, path: &str) -> Result<Vec<AppEffect>, String>;
    fn text_rasterized(&mut self, width: u32, height: u32, alpha: Vec<u8>) -> Result<(), String>;
    fn busy(&self) -> bool;
    fn background(&mut self, window: u64) -> Vec<AppEffect>;
    fn phone_back(&self, theme: DesktopTheme) -> Option<String>;
    fn phone_nav(&self, theme: DesktopTheme) -> Option<(&'static str, String, String)>;
    fn pull_to_refresh(&self) -> Option<&'static str>;
    fn text_field(&self, mobile: bool) -> Option<String>;
    fn client_size(&self) -> Option<(u32, u32)>;
    fn window_origin(&self) -> Option<(i32, i32)>;
    fn opens_maximized(&self) -> bool;
    fn link_key(&self) -> Option<&'static str>;
    fn link_revision(&self) -> u64;
    /// Take the shared document from a sibling window, when `other` is one of this
    /// application's own (see [`External::as_any`]).
    fn share_from(&mut self, other: &dyn External);
    fn instance_key(&self) -> Option<String>;
    fn reopen(&mut self, window: u64, argument: &str) -> Vec<AppEffect>;
    fn files_read(
        &mut self,
        window: u64,
        tag: &str,
        files: Vec<(String, Result<String, String>)>,
    ) -> Result<Vec<AppEffect>, String>;
    fn tree_listed(
        &mut self,
        window: u64,
        path: &str,
        result: Result<Vec<String>, String>,
    ) -> Result<Vec<AppEffect>, String>;
    fn written_content(&mut self, window: u64, path: &str, content: &str)
        -> Option<Vec<AppEffect>>;
    fn tree_listed_at(
        &mut self,
        window: u64,
        path: &str,
        depth: u32,
        result: Result<Vec<String>, String>,
    ) -> Result<Vec<AppEffect>, String>;
    fn attach(
        &mut self,
        home: &str,
        trash: &str,
        window: u64,
        theme: Option<DesktopTheme>,
    ) -> Vec<AppEffect>;
    fn is_code_editor(&self) -> bool;
    fn shell_ran(
        &mut self,
        window: u64,
        tag: &str,
        outcome: crate::ShellOutcome,
    ) -> Result<Vec<AppEffect>, String>;
    fn debug_reply(
        &mut self,
        window: u64,
        tag: &str,
        reply: Result<cw_protocol::debug::Reply, String>,
    ) -> Result<Vec<AppEffect>, String>;
    /// A copy, for `Clone` of the enum.
    fn clone_box(&self) -> Box<dyn External>;
    /// Equality with another registered application, for `PartialEq` of the enum.
    fn equals(&self, other: &dyn External) -> bool;
    /// The application as its own type, for the crate that registered it.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// A registered application launched, with what launching it asks of the machine.
pub type Launched = Result<(Box<dyn External>, Vec<AppEffect>), String>;
/// How a registration launches application `kind` for a machine painted as `theme`: `None`
/// when it is not one of its kinds.
pub type Launch = fn(
    kind: &str,
    argument: &str,
    window: u64,
    clock_us: u64,
    theme: DesktopTheme,
) -> Option<Launched>;
/// How a registration reads back the application whose state `text` is, written under tag
/// `tag`: `None` when the tag is not one of its own.
pub type FromJson = fn(tag: &str, text: &str) -> Option<Result<Box<dyn External>, String>>;

/// What a crate registers: the applications it carries, how to launch one and how to read
/// one back from the JSON it writes.
pub struct Registration {
    /// The kinds it launches, each with the tag its state is written under (`app`).
    pub apps: &'static [(&'static str, &'static str)],
    /// Launch an application of one of these kinds.
    pub launch: Launch,
    /// Read one back from its state.
    pub from_json: FromJson,
    /// What an application of `kind` declares for the rules, as `apps::declared` answers.
    pub declared: fn(kind: &str) -> Option<&'static Declaration>,
}

static REGISTERED: RwLock<Vec<&'static Registration>> = RwLock::new(Vec::new());

/// Carry `registration`'s applications from now on. Registering the same registration twice
/// changes nothing; a kind another registration already carries is refused.
pub fn register(registration: &'static Registration) -> Result<(), String> {
    let mut all = REGISTERED.write().unwrap_or_else(|e| e.into_inner());
    if all.iter().any(|r| std::ptr::eq(*r, registration)) {
        return Ok(());
    }
    for (kind, tag) in registration.apps {
        if NativeApp::KINDS.contains(kind)
            || all
                .iter()
                .any(|r| r.apps.iter().any(|(k, t)| k == kind || t == tag))
        {
            return Err(format!(
                "application {kind} is already carried by this build"
            ));
        }
    }
    all.push(registration);
    Ok(())
}

fn each<T>(f: impl FnMut(&'static Registration) -> Option<T>) -> Option<T> {
    REGISTERED
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .copied()
        .find_map(f)
}

/// Whether a registered crate carries application `kind`.
pub fn carries(kind: &str) -> bool {
    each(|r| r.apps.iter().any(|(k, _)| *k == kind).then_some(())).is_some()
}
/// The kinds registered crates carry, in registration order.
pub fn kinds() -> Vec<&'static str> {
    REGISTERED
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .flat_map(|r| r.apps.iter().map(|(k, _)| *k))
        .collect()
}
pub(crate) fn declared(kind: &str) -> Option<&'static Declaration> {
    each(|r| (r.declared)(kind))
}
pub(crate) fn launch(
    kind: &str,
    argument: &str,
    window: u64,
    clock_us: u64,
    theme: DesktopTheme,
) -> Option<Result<(ExternalApp, Vec<AppEffect>), String>> {
    each(|r| (r.launch)(kind, argument, window, clock_us, theme))
        .map(|launched| launched.map(|(app, effects)| (ExternalApp(app), effects)))
}
/// The registered application `text` is the state of, if a registered crate writes tag
/// `tag`.
pub(crate) fn from_json(tag: &str, text: &str) -> Option<Result<ExternalApp, String>> {
    each(|r| (r.from_json)(tag, text)).map(|read| read.map(ExternalApp))
}

/// A registered application, as the enum holds it.
pub struct ExternalApp(pub Box<dyn External>);
impl ExternalApp {
    /// The application as its own type `T`, when it is one.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.0.as_any().downcast_ref()
    }
    pub fn downcast_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.0.as_any_mut().downcast_mut()
    }
}
impl std::ops::Deref for ExternalApp {
    type Target = dyn External;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
impl std::ops::DerefMut for ExternalApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.0
    }
}
impl Clone for ExternalApp {
    fn clone(&self) -> Self {
        Self(self.0.clone_box())
    }
}
impl PartialEq for ExternalApp {
    fn eq(&self, other: &Self) -> bool {
        self.0.equals(&*other.0)
    }
}
impl Eq for ExternalApp {}
impl std::fmt::Debug for ExternalApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl Serialize for ExternalApp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        erased_serde::serialize(&*self.0, serializer)
    }
}
impl<'de> Deserialize<'de> for ExternalApp {
    /// Read through the JSON text, whose `app` tag names the registration: a world's
    /// applications are read with [`NativeApp::from_json`], and this path only serves a
    /// reader that deserializes the enum whole.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let tag = value
            .get("app")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| serde::de::Error::custom("an application's state names no app"))?
            .to_owned();
        let text = serde_json::to_string(&value).map_err(serde::de::Error::custom)?;
        from_json(&tag, &text)
            .ok_or_else(|| {
                serde::de::Error::custom(format!("this build has no application {tag:?}"))
            })?
            .map_err(serde::de::Error::custom)
    }
}
