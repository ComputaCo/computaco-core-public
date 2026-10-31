//! Applications built as Wasm modules of their own, fetched and instantiated the first
//! time one is launched (ops/engineering/app-modules.md).
//!
//! A build with the `app-modules` feature and without an application's own feature
//! (`kicad`, `freecad`, `office`, `video`, `music`, `maps`, `code`) does not carry that
//! application. It carries the module's name and the SHA-256 of the module it was built
//! with ([`MODULES`]), and serves the application through [`ModuleApp`]: the same
//! `NativeApp` methods, each one a call into the module across one interface
//! ([`Call`]), answered by the module's copy of the same application code
//! (`crate::apps::module::guest`, built by `cw-app-module`).
//!
//! **State stays here.** The application's state is held in the core as the JSON its
//! `NativeApp` variant serializes to, so a snapshot, a fork and a state hash see
//! exactly what a build carrying the application sees: `canonical_hash` of a world is
//! the same in both. The module keeps a parsed copy per state it has seen (keyed by a
//! number the core gives each state, never part of the world), and asks for the JSON
//! again when it has not seen one, so a fork or a restored snapshot needs nothing from
//! it.
//!
//! **Replay loads the same code.** The host's loader ([`set_module_host`]) is asked
//! for a module by name and SHA-256 and must answer with exactly those bytes. Each
//! step records the modules it used (`cw_modules::take_used`), and the environment keeps
//! them in its journal and snapshots, so a replay refuses a build whose module
//! differs instead of running other code.
use crate::apps::NativeApp;
use crate::desktop_scene::{DesktopTheme, Painter};
use crate::{AppEffect, AppEnv, Clipboard, FilesEnv, PointerPhase, Scroll, SystemSettings, Wheel};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::value::RawValue;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// One application module: its name (`modules/<name>.wasm`), the applications it
/// serves as (kind, serialized tag), and the SHA-256 of the module this build pins
/// (`CW_APP_MODULE_<NAME>` at build time for the catalog's modules; what
/// [`register_module`] was told for a package installed at run time).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppModule {
    pub name: String,
    pub apps: Vec<(String, String)>,
    pub sha256: Option<String>,
    /// This build carries the applications itself (their feature is on).
    pub native: bool,
}

/// The SHA-256 this build was made with for one of the catalog's modules.
fn pinned_sha(name: &str) -> Option<&'static str> {
    match name {
        "kicad" => option_env!("CW_APP_MODULE_KICAD"),
        "freecad" => option_env!("CW_APP_MODULE_FREECAD"),
        "office" => option_env!("CW_APP_MODULE_OFFICE"),
        "video" => option_env!("CW_APP_MODULE_VIDEO"),
        "music" => option_env!("CW_APP_MODULE_MUSIC"),
        "maps" => option_env!("CW_APP_MODULE_MAPS"),
        "code" => option_env!("CW_APP_MODULE_CODE"),
        _ => None,
    }
}

/// Modules a host installed at run time, by name: the applications each serves and its
/// SHA-256. A name the catalog has replaces the catalog's pin.
static REGISTERED: std::sync::RwLock<Vec<AppModule>> = std::sync::RwLock::new(Vec::new());

/// Make `name` (`modules/<name>.wasm`, with SHA-256 `sha256`) serve `apps` as (kind,
/// tag) from now on: a package installed at run time. Replaces an earlier registration
/// of the same name.
pub fn register_module(name: &str, apps: Vec<(String, String)>, sha256: &str) {
    let mut reg = REGISTERED.write().unwrap_or_else(|e| e.into_inner());
    reg.retain(|m| m.name != name);
    let native = apps
        .iter()
        .any(|(k, _)| NativeApp::KINDS.contains(&k.as_str()));
    reg.push(AppModule {
        name: name.to_owned(),
        apps,
        sha256: Some(sha256.to_owned()),
        native,
    });
}
/// Forget every module registered at run time.
pub fn reset_registered() {
    REGISTERED
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
}

/// Every module this build knows: the catalog's, then the registered ones.
pub fn modules() -> Vec<Arc<AppModule>> {
    let registered = REGISTERED.read().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<Arc<AppModule>> = Vec::new();
    for (name, apps) in super::catalog::MODULES {
        if registered.iter().any(|m| m.name == *name) {
            continue;
        }
        let apps: Vec<(String, String)> = apps
            .iter()
            .map(|(k, t)| ((*k).to_owned(), (*t).to_owned()))
            .collect();
        let native = apps
            .iter()
            .any(|(k, _)| NativeApp::KINDS.contains(&k.as_str()));
        out.push(Arc::new(AppModule {
            name: (*name).to_owned(),
            apps,
            sha256: pinned_sha(name).map(str::to_owned),
            native,
        }));
    }
    out.extend(registered.iter().cloned().map(Arc::new));
    out
}

impl AppModule {
    /// This build serves the module's applications through it.
    pub fn served(&self) -> bool {
        cfg!(feature = "app-modules") && !self.native && self.sha256.is_some()
    }
}
/// The module serving application `kind` in this build, if one does.
pub fn serving(kind: &str) -> Option<Arc<AppModule>> {
    modules()
        .into_iter()
        .find(|m| m.served() && m.apps.iter().any(|(k, _)| k == kind))
}
/// The module serving the application serialized with tag `tag`, and its kind.
fn serving_tag(tag: &str) -> Option<(Arc<AppModule>, String)> {
    modules().into_iter().filter(|m| m.served()).find_map(|m| {
        let kind = m
            .apps
            .iter()
            .find(|(_, t)| t == tag)
            .map(|(kind, _)| kind.clone())?;
        Some((m, kind))
    })
}
/// Every module this build serves, as (name, SHA-256).
pub fn served_modules() -> BTreeMap<String, String> {
    modules()
        .iter()
        .filter(|m| m.served())
        .map(|m| (m.name.clone(), m.sha256.clone().unwrap_or_default()))
        .collect()
}

/// The host modules are fetched and called through (`cw_modules::ModuleHost`).
pub use cw_modules::ModuleHost;
pub fn set_module_host(host: Option<ModuleHost>) {
    cw_modules::set_host(host);
}

/// Keys name a state for the module's cache; unique in this process, never part of a
/// world (two equal states may have different keys).
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);
fn fresh_key() -> u64 {
    NEXT_KEY.fetch_add(1, Ordering::Relaxed)
}

/// An application served by its module. Serializes to exactly what its `NativeApp`
/// variant serializes to.
#[derive(Clone, Debug)]
pub struct ModuleApp {
    module: Arc<AppModule>,
    kind: &'static str,
    state: Arc<str>,
    key: u64,
}
use super::catalog::static_kind;

impl PartialEq for ModuleApp {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.state == other.state
    }
}
impl Eq for ModuleApp {}
impl Serialize for ModuleApp {
    /// Field by field in the order the application wrote them, so the bytes are the
    /// ones a build carrying the application writes (state hashes serialize sessions
    /// as text, in order).
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        transcode::Transcoder::new(&mut serde_json::Deserializer::from_str(&self.state))
            .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ModuleApp {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Written back out as JSON in the order it arrives, for the same reason.
        let mut text = Vec::new();
        transcode::Transcoder::new(deserializer)
            .serialize(&mut serde_json::Serializer::new(&mut text))
            .map_err(serde::de::Error::custom)?;
        let text = String::from_utf8(text).map_err(serde::de::Error::custom)?;
        #[derive(Deserialize)]
        struct Tag {
            app: String,
        }
        let tag: Tag = serde_json::from_str(&text).map_err(serde::de::Error::custom)?;
        let (module, kind) = serving_tag(&tag.app).ok_or_else(|| {
            serde::de::Error::custom(format!("this build has no application {:?}", tag.app))
        })?;
        Ok(Self {
            module,
            kind: static_kind(&kind),
            state: text.into(),
            key: fresh_key(),
        })
    }
}

/// Streams what a deserializer reads into a serializer, value by value, keeping the
/// order of map entries (serde_json's `Value` would sort them).
mod transcode {
    use serde::{de, ser, Serialize, Serializer};
    use std::cell::RefCell;
    use std::fmt;

    pub struct Transcoder<D>(RefCell<Option<D>>);
    impl<D> Transcoder<D> {
        pub fn new(deserializer: D) -> Self {
            Self(RefCell::new(Some(deserializer)))
        }
    }
    impl<'de, D: de::Deserializer<'de>> Serialize for Transcoder<D> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let deserializer = self
                .0
                .borrow_mut()
                .take()
                .ok_or_else(|| ser::Error::custom("a value is transcoded once"))?;
            deserializer
                .deserialize_any(Visitor(serializer))
                .map_err(ser::Error::custom)
        }
    }
    struct Visitor<S>(S);
    impl<'de, S: Serializer> de::Visitor<'de> for Visitor<S> {
        type Value = S::Ok;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("any value")
        }
        fn visit_bool<E: de::Error>(self, v: bool) -> Result<S::Ok, E> {
            self.0.serialize_bool(v).map_err(E::custom)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<S::Ok, E> {
            self.0.serialize_i64(v).map_err(E::custom)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<S::Ok, E> {
            self.0.serialize_u64(v).map_err(E::custom)
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<S::Ok, E> {
            self.0.serialize_f64(v).map_err(E::custom)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<S::Ok, E> {
            self.0.serialize_str(v).map_err(E::custom)
        }
        fn visit_unit<E: de::Error>(self) -> Result<S::Ok, E> {
            self.0.serialize_unit().map_err(E::custom)
        }
        fn visit_none<E: de::Error>(self) -> Result<S::Ok, E> {
            self.0.serialize_none().map_err(E::custom)
        }
        fn visit_some<D: de::Deserializer<'de>>(self, d: D) -> Result<S::Ok, D::Error> {
            self.0
                .serialize_some(&Transcoder::new(d))
                .map_err(de::Error::custom)
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<S::Ok, A::Error> {
            use ser::SerializeSeq;
            let mut out = self
                .0
                .serialize_seq(seq.size_hint())
                .map_err(de::Error::custom)?;
            while seq.next_element_seed(Element(&mut out))?.is_some() {}
            out.end().map_err(de::Error::custom)
        }
        fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<S::Ok, A::Error> {
            use ser::SerializeMap;
            let mut out = self
                .0
                .serialize_map(map.size_hint())
                .map_err(de::Error::custom)?;
            while map.next_key_seed(Key(&mut out))?.is_some() {
                map.next_value_seed(Value(&mut out))?;
            }
            out.end().map_err(de::Error::custom)
        }
    }
    struct Element<'a, S>(&'a mut S);
    impl<'de, S: ser::SerializeSeq> de::DeserializeSeed<'de> for Element<'_, S> {
        type Value = ();
        fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            self.0
                .serialize_element(&Transcoder::new(d))
                .map_err(de::Error::custom)
        }
    }
    struct Key<'a, S>(&'a mut S);
    impl<'de, S: ser::SerializeMap> de::DeserializeSeed<'de> for Key<'_, S> {
        type Value = ();
        fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            self.0
                .serialize_key(&Transcoder::new(d))
                .map_err(de::Error::custom)
        }
    }
    struct Value<'a, S>(&'a mut S);
    impl<'de, S: ser::SerializeMap> de::DeserializeSeed<'de> for Value<'_, S> {
        type Value = ();
        fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            self.0
                .serialize_value(&Transcoder::new(d))
                .map_err(de::Error::custom)
        }
    }
}

/// What a window's renderer hands an application: the facts of `AppEnv`, owned.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Env {
    pub theme: DesktopTheme,
    pub width: u32,
    pub height: u32,
    pub clock_us: u64,
    pub settings: SystemSettings,
    pub clipboard: Option<Clipboard>,
    pub share_to: Option<String>,
    pub home: String,
    pub folders: Vec<String>,
    pub trash: String,
    pub starred: Vec<String>,
    pub editor: Option<String>,
    pub pointer: Option<(i32, i32)>,
}
impl Env {
    fn of(env: &AppEnv<'_>) -> Self {
        Self {
            theme: env.theme,
            width: env.width,
            height: env.height,
            clock_us: env.clock_us,
            settings: *env.settings,
            clipboard: env.clipboard.cloned(),
            share_to: env.share_to.map(str::to_owned),
            home: env.files.home.to_owned(),
            folders: env.files.folders.clone(),
            trash: env.files.trash.clone(),
            starred: env.files.starred.to_vec(),
            editor: env.editor.map(str::to_owned),
            pointer: env.pointer,
        }
    }
    #[cfg(feature = "module-guest")]
    fn with<T>(&self, f: impl FnOnce(&AppEnv<'_>) -> T) -> T {
        f(&AppEnv {
            theme: self.theme,
            width: self.width,
            height: self.height,
            clock_us: self.clock_us,
            settings: &self.settings,
            clipboard: self.clipboard.as_ref(),
            share_to: self.share_to.as_deref().map(intern),
            files: FilesEnv {
                home: &self.home,
                folders: self.folders.clone(),
                trash: self.trash.clone(),
                starred: &self.starred,
            },
            editor: self.editor.as_deref().map(intern),
            pointer: self.pointer,
        })
    }
}
/// A painter as an application receives it and leaves it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Paint {
    pub scene: cw_scene::Scene,
    pub next: u64,
    pub z: i32,
    pub theme: DesktopTheme,
    pub scroll: Scroll,
}

use crate::effects::bytes;

/// The interface between the core and an application module: one call per
/// `NativeApp` method an application can be asked, with the method's arguments.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum Call {
    Launch {
        kind: String,
        argument: String,
        window: u64,
        clock_us: u64,
        theme: DesktopTheme,
    },
    Title {
        theme: DesktopTheme,
    },
    Document,
    Caption,
    Modified,
    Text {
        text: String,
    },
    Key {
        window: u64,
        key: String,
        clock_us: u64,
    },
    Click {
        window: u64,
        target: String,
        clock_us: u64,
    },
    Http {
        window: u64,
        tag: String,
        status: u16,
        body: String,
    },
    Offline {
        tag: String,
        reason: String,
    },
    Image {
        path: String,
        width: u32,
        height: u32,
        #[serde(with = "bytes")]
        rgba: Vec<u8>,
    },
    ImageFailed {
        path: String,
        reason: String,
    },
    ClickAt {
        window: u64,
        target: String,
        dx: i32,
        dy: i32,
        clock_us: u64,
    },
    PressAt {
        target: String,
        dx: i32,
        dy: i32,
    },
    Activate {
        window: u64,
        target: String,
        clock_us: u64,
    },
    TextEffects {
        window: u64,
        text: String,
    },
    Paste {
        window: u64,
        text: String,
    },
    DarkChrome,
    Chrome,
    Page {
        page: cw_protocol::Page,
    },
    Render {
        paint: Paint,
        env: Env,
    },
    AcceptsText,
    Drags {
        target: String,
    },
    Bytes {
        window: u64,
        path: String,
        #[serde(with = "bytes::result")]
        result: Result<Vec<u8>, String>,
        clock_us: u64,
    },
    BytesSaved {
        window: u64,
        path: String,
        result: Result<(), String>,
    },
    PointerButton {
        button: u8,
    },
    TakesSecondary {
        target: String,
    },
    Wheel {
        target: String,
        x: i32,
        y: i32,
        wheel: Wheel,
    },
    Hovers {
        target: String,
    },
    Hover {
        window: u64,
        target: String,
        x: i32,
        y: i32,
    },
    HoverCursor {
        target: String,
    },
    PointerModifiers {
        modifiers: u8,
    },
    Pointer {
        window: u64,
        target: String,
        phase: PointerPhase,
        x: i32,
        y: i32,
        clock_us: u64,
    },
    Listed {
        entries: Vec<String>,
    },
    ImageSaved {
        window: u64,
        path: String,
    },
    TextRasterized {
        width: u32,
        height: u32,
        #[serde(with = "bytes")]
        alpha: Vec<u8>,
    },
    Busy,
    Background {
        window: u64,
    },
    PhoneBack {
        theme: DesktopTheme,
    },
    PhoneNav {
        theme: DesktopTheme,
    },
    PullToRefresh,
    TextField {
        mobile: bool,
    },
    LinkKey,
    LinkRevision,
    ShareFrom {
        other: String,
    },
    InstanceKey,
    Reopen {
        window: u64,
        argument: String,
    },
    FilesRead {
        window: u64,
        tag: String,
        files: Vec<(String, Result<String, String>)>,
    },
    TreeListed {
        window: u64,
        path: String,
        depth: u32,
        result: Result<Vec<String>, String>,
    },
    Written {
        window: u64,
        path: String,
        content: String,
    },
    Attach {
        home: String,
        trash: String,
        window: u64,
        theme: Option<DesktopTheme>,
    },
    ShellRan {
        window: u64,
        tag: String,
        outcome: crate::ShellOutcome,
    },
    DebugReply {
        window: u64,
        tag: String,
        reply: Result<cw_protocol::debug::Reply, String>,
    },
    IsEditor,
}
impl Call {
    /// Whether the call may change the application's state (a `&mut self` method).
    pub fn mutates(&self) -> bool {
        !matches!(
            self,
            Self::Title { .. }
                | Self::Document
                | Self::Caption
                | Self::Modified
                | Self::DarkChrome
                | Self::Chrome
                | Self::Page { .. }
                | Self::Render { .. }
                | Self::AcceptsText
                | Self::Drags { .. }
                | Self::TakesSecondary { .. }
                | Self::Hovers { .. }
                | Self::HoverCursor { .. }
                | Self::Busy
                | Self::PhoneBack { .. }
                | Self::PhoneNav { .. }
                | Self::PullToRefresh
                | Self::TextField { .. }
                | Self::LinkKey
                | Self::LinkRevision
                | Self::InstanceKey
                | Self::IsEditor
        )
    }
}

/// What a module answers: `miss` when it has no state under the key (send the state
/// with the call), else the call's result, and the application's state when the call
/// changed it. Results and states travel as JSON inside the JSON, never re-escaped.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Reply {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub miss: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Box<RawValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<Box<RawValue>>,
}

/// The pixel buffers of a scene's images, in node order. A render's reply carries them
/// after its JSON as raw bytes: as JSON arrays of numbers a 3D view's picture costs
/// a hundred times more to write and read.
fn images(scene: &mut cw_scene::Scene) -> impl Iterator<Item = &mut Vec<u8>> {
    scene
        .nodes
        .iter_mut()
        .filter_map(|n| match &mut n.primitive {
            cw_scene::Primitive::Image { rgba, .. } => Some(rgba),
            _ => None,
        })
}
/// A reply is its JSON, then (when it has any) a zero byte, which JSON never holds, and
/// each buffer as a little-endian `u32` length and its bytes.
fn split(reply: &[u8]) -> Result<(&[u8], Vec<Vec<u8>>), String> {
    let Some(at) = reply.iter().position(|&b| b == 0) else {
        return Ok((reply, Vec::new()));
    };
    let (json, mut rest) = (&reply[..at], &reply[at + 1..]);
    let mut blobs = Vec::new();
    while !rest.is_empty() {
        let len = rest
            .get(..4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("four bytes")) as usize)
            .ok_or("a reply's buffer is cut short")?;
        let blob = rest
            .get(4..4 + len)
            .ok_or("a reply's buffer is cut short")?;
        blobs.push(blob.to_vec());
        rest = &rest[4 + len..];
    }
    Ok((json, blobs))
}
fn null() -> Box<RawValue> {
    RawValue::from_string("null".into()).expect("null is JSON")
}

/// Interned strings, for the methods that answer `&'static str`.
pub fn intern(text: &str) -> &'static str {
    static TABLE: std::sync::Mutex<std::collections::BTreeSet<&'static str>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());
    let mut table = TABLE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(found) = table.get(text) {
        return found;
    }
    let leaked: &'static str = Box::leak(text.to_owned().into_boxed_str());
    table.insert(leaked);
    leaked
}

fn host_call(module: &AppModule, input: &[u8]) -> Result<Vec<u8>, String> {
    cw_modules::call(&module.name, module.sha256.as_deref(), input)
}

impl ModuleApp {
    pub fn kind(&self) -> &'static str {
        self.kind
    }
    pub fn module(&self) -> &AppModule {
        &self.module
    }
    /// Launch application `kind` in its module. Fails (and changes nothing) when the
    /// module cannot be loaded.
    pub fn launch(
        module: &Arc<AppModule>,
        kind: &str,
        argument: &str,
        window: u64,
        clock_us: u64,
        theme: DesktopTheme,
    ) -> Result<(Self, Vec<AppEffect>), String> {
        let call = Call::Launch {
            kind: kind.into(),
            argument: argument.into(),
            window,
            clock_us,
            theme,
        };
        let request = serde_json::to_vec(&Request {
            key: 0,
            next_key: 0,
            state: None,
            call: (&call).into(),
        })
        .map_err(|e| e.to_string())?;
        let reply: Reply = serde_json::from_slice(&host_call(module, &request)?)
            .map_err(|e| format!("{}: {e}", module.name))?;
        let result = reply.result.unwrap_or_else(null);
        let effects: Result<Vec<AppEffect>, String> =
            serde_json::from_str(result.get()).map_err(|e| e.to_string())?;
        let effects = effects?;
        let state = reply
            .state
            .ok_or_else(|| format!("{} launched {kind} with no state", module.name))?;
        let kind = module
            .apps
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(k, _)| static_kind(k))
            .ok_or("not this module's application")?;
        Ok((
            Self {
                module: module.clone(),
                kind,
                state: state.get().into(),
                key: fresh_key(),
            },
            effects,
        ))
    }
    /// One call: the result, the next state when the call changed it, and the pixel
    /// buffers of a render (see `split`).
    #[allow(clippy::type_complexity)]
    fn exchange(
        &self,
        call: &Call,
    ) -> Result<(Box<RawValue>, Option<(Box<RawValue>, u64)>, Vec<Vec<u8>>), String> {
        let next_key = fresh_key();
        let mut state: Option<&RawValue> = None;
        loop {
            let request = serde_json::to_vec(&Request {
                key: self.key,
                next_key,
                state,
                call: call.into(),
            })
            .map_err(|e| e.to_string())?;
            let bytes = host_call(&self.module, &request)?;
            let (json, blobs) = split(&bytes)?;
            let reply: Reply =
                serde_json::from_slice(json).map_err(|e| format!("{}: {e}", self.module.name))?;
            if reply.miss {
                if state.is_some() {
                    return Err(format!("{} lost the state it was sent", self.module.name));
                }
                state = Some(serde_json::from_str(&self.state).map_err(|e| e.to_string())?);
                continue;
            }
            // A JSON `null` result (a `()` or `None` answer) reads back as no result.
            let result = reply.result.unwrap_or_else(null);
            return Ok((result, reply.state.map(|s| (s, next_key)), blobs));
        }
    }
    /// A call that may change the state (a `&mut self` method).
    pub fn call<R: DeserializeOwned>(&mut self, call: Call) -> Result<R, String> {
        let (result, next, _) = self.exchange(&call)?;
        if let Some((state, key)) = next {
            self.state = state.get().into();
            self.key = key;
        }
        serde_json::from_str(result.get()).map_err(|e| e.to_string())
    }
    /// A call that reads the state (a `&self` method).
    pub fn query<R: DeserializeOwned>(&self, call: Call) -> Result<R, String> {
        let (result, _, _) = self.exchange(&call)?;
        serde_json::from_str(result.get()).map_err(|e| e.to_string())
    }
    /// For methods with no way to report a failure: a module that answered its launch
    /// and then cannot be reached is a broken host, and nothing is drawn or decided
    /// in its place.
    pub fn must<R: DeserializeOwned>(&self, call: Call) -> R {
        self.query(call)
            .unwrap_or_else(|e| panic!("application module {}: {e}", self.module.name))
    }
    pub fn must_mut<R: DeserializeOwned>(&mut self, call: Call) -> R {
        let name = self.module.name.clone();
        self.call(call)
            .unwrap_or_else(|e| panic!("application module {name}: {e}"))
    }
    pub fn render(&self, p: &mut Painter, env: &AppEnv<'_>) {
        let paint = Paint {
            scene: std::mem::replace(&mut p.scene, cw_scene::Scene::new(0, 0)),
            next: p.next,
            z: p.z,
            theme: p.theme,
            scroll: std::mem::take(&mut p.scroll),
        };
        let call = Call::Render {
            paint,
            env: Env::of(env),
        };
        let name = self.module.name.clone();
        let (result, _, blobs) = self
            .exchange(&call)
            .unwrap_or_else(|e| panic!("application module {name}: {e}"));
        let mut out: Paint = serde_json::from_str(result.get())
            .unwrap_or_else(|e| panic!("application module {name}: {e}"));
        let mut blobs = blobs.into_iter();
        for rgba in images(&mut out.scene) {
            *rgba = blobs.next().unwrap_or_default();
        }
        p.scene = out.scene;
        p.next = out.next;
        p.z = out.z;
        p.theme = out.theme;
        p.scroll = out.scroll;
    }
    pub fn page(&self, page: &mut cw_protocol::Page) {
        *page = self.must(Call::Page { page: page.clone() });
    }
    pub fn state_json(&self) -> &str {
        &self.state
    }
    /// A `&mut self` method that answers `Result<T, String>`; a module that cannot be
    /// reached answers the error too.
    pub fn act<T: DeserializeOwned>(&mut self, call: Call) -> Result<T, String> {
        self.call::<Result<T, String>>(call).and_then(|r| r)
    }
    pub fn title(&self, theme: DesktopTheme) -> String {
        self.must(Call::Title { theme })
    }
    pub fn document(&self) -> String {
        self.must(Call::Document)
    }
    pub fn caption(&self) -> String {
        self.must(Call::Caption)
    }
    pub fn modified(&self) -> bool {
        self.must(Call::Modified)
    }
    pub fn text(&mut self, text: &str) -> Result<(), String> {
        self.act(Call::Text { text: text.into() })
    }
    pub fn key(&mut self, window: u64, key: &str, clock_us: u64) -> Result<Vec<AppEffect>, String> {
        self.act(Call::Key {
            window,
            key: key.into(),
            clock_us,
        })
    }
    pub fn click(
        &mut self,
        window: u64,
        target: &str,
        clock_us: u64,
    ) -> Result<Vec<AppEffect>, String> {
        self.act(Call::Click {
            window,
            target: target.into(),
            clock_us,
        })
    }
    pub fn http(
        &mut self,
        window: u64,
        tag: &str,
        status: u16,
        body: &str,
    ) -> Result<Vec<AppEffect>, String> {
        self.act(Call::Http {
            window,
            tag: tag.into(),
            status,
            body: body.into(),
        })
    }
    pub fn offline(&mut self, tag: &str, reason: &str) {
        self.must_mut(Call::Offline {
            tag: tag.into(),
            reason: reason.into(),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct Request<'a> {
    key: u64,
    next_key: u64,
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    state: Option<&'a RawValue>,
    call: CallRef<'a>,
}
/// The call as sent (borrowed) and as received (owned).
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
enum CallRef<'a> {
    #[serde(skip_deserializing)]
    Sent(&'a Call),
    Received(Call),
}
impl<'a> From<&'a Call> for CallRef<'a> {
    fn from(call: &'a Call) -> Self {
        Self::Sent(call)
    }
}

/// The module's side: one instance of the application code per module, called with a
/// request and answering a reply (`cw-app-module` exports this as `cw_call`).
#[cfg(feature = "module-guest")]
pub mod guest {
    use super::*;
    /// States this instance has parsed, newest last; small, since a window's state is
    /// asked for again on a miss.
    const CACHE: usize = 16;
    struct Entry {
        key: u64,
        app: NativeApp,
        json: Arc<str>,
    }
    thread_local! {
        static STATES: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
        /// A render's pixel buffers, sent after the reply's JSON.
        static BLOBS: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
    }
    fn keep(entry: Entry) {
        STATES.with(|s| {
            let mut s = s.borrow_mut();
            s.retain(|e| e.key != entry.key);
            if s.len() >= CACHE {
                s.remove(0);
            }
            s.push(entry);
        })
    }
    fn take(key: u64) -> Option<Entry> {
        STATES.with(|s| {
            let mut s = s.borrow_mut();
            let i = s.iter().position(|e| e.key == key)?;
            Some(s.remove(i))
        })
    }
    fn json<T: Serialize>(value: T) -> Box<RawValue> {
        RawValue::from_string(serde_json::to_string(&value).expect("an answer serializes"))
            .expect("serde_json writes JSON")
    }
    fn raw(text: String) -> Box<RawValue> {
        RawValue::from_string(text).expect("serde_json writes JSON")
    }
    fn done(result: Box<RawValue>, state: Option<Box<RawValue>>) -> Reply {
        Reply {
            miss: false,
            result: Some(result),
            state,
        }
    }
    pub fn serve(input: &[u8]) -> Vec<u8> {
        let reply = match serde_json::from_slice::<Request>(input) {
            Ok(request) => answer(request),
            Err(e) => done(json(Err::<(), String>(format!("bad request: {e}"))), None),
        };
        let mut out = serde_json::to_vec(&reply).expect("a reply serializes");
        let blobs = BLOBS.with(|b| std::mem::take(&mut *b.borrow_mut()));
        if !blobs.is_empty() {
            out.push(0);
            for blob in blobs {
                out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
                out.extend_from_slice(&blob);
            }
        }
        out
    }
    fn answer(request: Request<'_>) -> Reply {
        let CallRef::Received(call) = request.call else {
            unreachable!("a received request owns its call")
        };
        if let Call::Launch {
            kind,
            argument,
            window,
            clock_us,
            theme,
        } = &call
        {
            let launched = NativeApp::launch_on(kind, argument, *window, *clock_us, *theme)
                .unwrap_or_else(|| Err(format!("this module has no application {kind}")));
            return match launched {
                Ok((app, effects)) => done(
                    json(Ok::<_, String>(effects)),
                    Some(raw(
                        serde_json::to_string(&app).expect("an application serializes")
                    )),
                ),
                Err(e) => done(json(Err::<Vec<AppEffect>, _>(e)), None),
            };
        }
        let mut entry = match (take(request.key), request.state) {
            (Some(entry), _) => entry,
            (None, Some(state)) => Entry {
                key: request.key,
                app: serde_json::from_str(state.get()).expect("the core sends what a module wrote"),
                json: state.get().into(),
            },
            (None, None) => {
                return Reply {
                    miss: true,
                    ..Reply::default()
                }
            }
        };
        if !call.mutates() {
            // Only `&self` methods answer these: the state is left as it was.
            let result = dispatch(&mut entry.app, call);
            keep(entry);
            return done(result, None);
        }
        // The state under the request's key stays what it was (a fork of the world may
        // still hold it); a changed state is kept under the key the core reserved.
        let mut app = entry.app.clone();
        let result = dispatch(&mut app, call);
        let next = serde_json::to_string(&app).expect("an application serializes");
        if *next == *entry.json {
            keep(Entry {
                key: entry.key,
                app,
                json: entry.json,
            });
            return done(result, None);
        }
        keep(entry);
        keep(Entry {
            key: request.next_key,
            app,
            json: next.as_str().into(),
        });
        done(result, Some(raw(next)))
    }
    fn dispatch(app: &mut NativeApp, call: Call) -> Box<RawValue> {
        match call {
            Call::Launch { .. } => unreachable!("answered before a state is needed"),
            Call::Title { theme } => json(app.title(theme)),
            Call::Document => json(app.document()),
            Call::Caption => json(app.caption()),
            Call::Modified => json(app.modified()),
            Call::Text { text } => json(app.text(&text)),
            Call::Key {
                window,
                key,
                clock_us,
            } => json(app.key(window, &key, clock_us)),
            Call::Click {
                window,
                target,
                clock_us,
            } => json(app.click(window, &target, clock_us)),
            Call::Http {
                window,
                tag,
                status,
                body,
            } => json(app.http(window, &tag, status, &body)),
            Call::Offline { tag, reason } => {
                app.offline(&tag, &reason);
                json(())
            }
            Call::Image {
                path,
                width,
                height,
                rgba,
            } => json(app.image(&path, width, height, rgba)),
            Call::ImageFailed { path, reason } => {
                app.image_failed(&path, &reason);
                json(())
            }
            Call::ClickAt {
                window,
                target,
                dx,
                dy,
                clock_us,
            } => json(app.click_at(window, &target, dx, dy, clock_us)),
            Call::PressAt { target, dx, dy } => json(app.press_at(&target, dx, dy)),
            Call::Activate {
                window,
                target,
                clock_us,
            } => json(app.activate(window, &target, clock_us)),
            Call::TextEffects { window, text } => json(app.text_effects(window, &text)),
            Call::Paste { window, text } => json(app.paste(window, &text)),
            Call::DarkChrome => json(app.dark_chrome()),
            Call::Chrome => json(app.chrome()),
            Call::Page { mut page } => {
                app.page(&mut page);
                json(page)
            }
            Call::Render { paint, env } => {
                let mut p = Painter {
                    scene: paint.scene,
                    next: paint.next,
                    z: paint.z,
                    theme: paint.theme,
                    scroll: paint.scroll,
                };
                env.with(|env| app.render(&mut p, env));
                let blobs: Vec<Vec<u8>> = images(&mut p.scene).map(std::mem::take).collect();
                BLOBS.with(|b| *b.borrow_mut() = blobs);
                json(Paint {
                    scene: p.scene,
                    next: p.next,
                    z: p.z,
                    theme: p.theme,
                    scroll: p.scroll,
                })
            }
            Call::AcceptsText => json(app.accepts_text()),
            Call::Drags { target } => json(app.drags(&target)),
            Call::Bytes {
                window,
                path,
                result,
                clock_us,
            } => json(app.bytes(window, &path, result, clock_us)),
            Call::BytesSaved {
                window,
                path,
                result,
            } => json(app.bytes_saved(window, &path, result)),
            Call::PointerButton { button } => {
                app.pointer_button(button);
                json(())
            }
            Call::TakesSecondary { target } => json(app.takes_secondary(&target)),
            Call::Wheel {
                target,
                x,
                y,
                wheel,
            } => json(app.wheel(&target, x, y, wheel)),
            Call::Hovers { target } => json(app.hovers(&target)),
            Call::Hover {
                window,
                target,
                x,
                y,
            } => json(app.hover(window, &target, x, y)),
            Call::HoverCursor { target } => json(app.hover_cursor(&target)),
            Call::PointerModifiers { modifiers } => {
                app.pointer_modifiers(modifiers);
                json(())
            }
            Call::Pointer {
                window,
                target,
                phase,
                x,
                y,
                clock_us,
            } => json(app.pointer(window, &target, phase, x, y, clock_us)),
            Call::Listed { entries } => json(app.listed(entries)),
            Call::ImageSaved { window, path } => json(app.image_saved(window, &path)),
            Call::TextRasterized {
                width,
                height,
                alpha,
            } => json(app.text_rasterized(width, height, alpha)),
            Call::Busy => json(app.busy()),
            Call::Background { window } => json(app.background(window)),
            Call::PhoneBack { theme } => json(app.phone_back(theme)),
            Call::PhoneNav { theme } => json(app.phone_nav(theme)),
            Call::PullToRefresh => json(app.pull_to_refresh()),
            Call::TextField { mobile } => json(app.text_field(mobile)),
            Call::LinkKey => json(app.link_key()),
            Call::LinkRevision => json(app.link_revision()),
            Call::ShareFrom { other } => {
                let other: NativeApp =
                    serde_json::from_str(&other).expect("the core sends what a module wrote");
                {
                    app.share_from(&other);
                    json(())
                }
            }
            Call::InstanceKey => json(app.instance_key()),
            Call::Reopen { window, argument } => json(app.reopen(window, &argument)),
            Call::FilesRead { window, tag, files } => json(app.files_read(window, &tag, files)),
            Call::TreeListed {
                window,
                path,
                depth,
                result,
            } => json(app.tree_listed_at(window, &path, depth, result)),
            Call::Written {
                window,
                path,
                content,
            } => json(app.written_content(window, &path, &content)),
            Call::Attach {
                home,
                trash,
                window,
                theme,
            } => json(app.attach(&home, &trash, window, theme)),
            Call::ShellRan {
                window,
                tag,
                outcome,
            } => json(app.shell_ran(window, &tag, outcome)),
            Call::DebugReply { window, tag, reply } => json(app.debug_reply(window, &tag, reply)),
            Call::IsEditor => json(app.is_code_editor()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bytes_cross_as_base64_and_back() {
        for data in [
            &b""[..],
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            &[0, 255, 7, 128, 64],
        ] {
            let text = bytes::encode(data);
            assert_eq!(bytes::decode(&text).unwrap(), data);
        }
        assert_eq!(bytes::encode(b"Man"), "TWFu");
        assert_eq!(bytes::encode(b"Ma"), "TWE=");
    }
    /// The state of a module's application is written back byte for byte, in the
    /// order its fields were written, under a parent that is internally tagged too.
    #[test]
    fn transcoding_keeps_order_and_bytes() {
        let text = r#"{"app":"kicad","zeta":1,"alpha":[1.5,-2,{"b":null,"a":"x\"y"}],"m":0.1}"#;
        let mut out = Vec::new();
        transcode::Transcoder::new(&mut serde_json::Deserializer::from_str(text))
            .serialize(&mut serde_json::Serializer::new(&mut out))
            .unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), text);
        #[derive(Serialize)]
        #[serde(tag = "kind")]
        enum Parent<'a> {
            Native(
                &'a transcode::Transcoder<
                    &'a mut serde_json::Deserializer<serde_json::de::StrRead<'a>>,
                >,
            ),
        }
        let mut de = serde_json::Deserializer::from_str(text);
        let inner = transcode::Transcoder::new(&mut de);
        let tagged = serde_json::to_string(&Parent::Native(&inner)).unwrap();
        assert_eq!(tagged, format!(r#"{{"kind":"Native",{}"#, &text[1..]));
    }
    #[test]
    fn every_module_application_has_one_module() {
        let all = modules();
        let mut kinds: Vec<&str> = all
            .iter()
            .flat_map(|m| m.apps.iter().map(|a| a.0.as_str()))
            .collect();
        let n = kinds.len();
        kinds.sort();
        kinds.dedup();
        assert_eq!(kinds.len(), n);
    }
}
