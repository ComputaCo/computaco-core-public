//! Thin JavaScript bindings. All semantics execute in the shared Rust facade.
use serde::{de::DeserializeOwned, Serialize};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::prelude::*;

fn error(e: impl std::fmt::Display) -> JsValue {
    js_sys::Error::new(&e.to_string()).into()
}
fn seed_value(value: JsValue) -> Result<u64, JsValue> {
    if let Some(text) = value.as_string() {
        text.parse::<u64>().map_err(error)
    } else {
        serde_wasm_bindgen::from_value(value).map_err(error)
    }
}
fn decode<T: DeserializeOwned>(v: JsValue) -> Result<T, JsValue> {
    serde_json::from_value(decode_value(&v, 0)?).map_err(error)
}
/// Exact language conversion only: preserve JSON's integer semantics and reject
/// JavaScript values that have no portable serialized counterpart.
fn decode_value(value: &JsValue, depth: usize) -> Result<serde_json::Value, JsValue> {
    use serde_json::{Number, Value};
    if depth > 128 {
        return Err(error("value exceeds maximum nesting depth 128"));
    }
    if value.is_null() {
        return Ok(Value::Null);
    }
    if let Some(value) = value.as_bool() {
        return Ok(Value::Bool(value));
    }
    if let Some(value) = value.as_string() {
        return Ok(Value::String(value));
    }
    if value.is_bigint() {
        let bigint = value.unchecked_ref::<js_sys::BigInt>();
        let text = bigint
            .to_string(10)?
            .as_string()
            .ok_or_else(|| error("invalid BigInt"))?;
        let number = if text.starts_with('-') {
            Number::from(text.parse::<i64>().map_err(error)?)
        } else {
            Number::from(text.parse::<u64>().map_err(error)?)
        };
        return Ok(Value::Number(number));
    }
    if let Some(value) = value.as_f64() {
        if !value.is_finite() {
            return Err(error("non-finite Number is not serializable"));
        }
        if value.fract() == 0.0 {
            if value.abs() > 9_007_199_254_740_991.0 {
                return Err(error(
                    "unsafe integral Number; use BigInt for exact integers",
                ));
            }
            return Ok(Value::Number(if value < 0.0 {
                Number::from(value as i64)
            } else {
                Number::from(value as u64)
            }));
        }
        return Ok(Value::Number(
            Number::from_f64(value).ok_or_else(|| error("invalid Number"))?,
        ));
    }
    if js_sys::Array::is_array(value) {
        let array = value.unchecked_ref::<js_sys::Array>();
        let mut values = Vec::with_capacity(array.length() as usize);
        for index in 0..array.length() {
            values.push(decode_value(
                &js_sys::Reflect::get(value, &JsValue::from_f64(index.into()))?,
                depth + 1,
            )?);
        }
        return Ok(Value::Array(values));
    }
    if value.is_object() {
        let prototype = js_sys::Reflect::get_prototype_of(value)?;
        let plain_prototype = js_sys::Object::get_prototype_of(&js_sys::Object::new());
        if !prototype.is_null() && !js_sys::Object::is(&prototype, &plain_prototype) {
            return Err(error("expected a plain object or array"));
        }
        let mut values = serde_json::Map::new();
        for key in js_sys::Reflect::own_keys(value)?.iter() {
            let name = key
                .as_string()
                .ok_or_else(|| error("symbol keys are not serializable"))?;
            values.insert(
                name,
                decode_value(&js_sys::Reflect::get(value, &key)?, depth + 1)?,
            );
        }
        return Ok(Value::Object(values));
    }
    Err(error(
        "undefined, functions and symbols are not serializable",
    ))
}
fn encode<T: Serialize>(v: &T) -> Result<JsValue, JsValue> {
    encode_value(&serde_json::to_value(v).map_err(error)?)
}
fn encode_value(value: &serde_json::Value) -> Result<JsValue, JsValue> {
    use serde_json::Value;
    Ok(match value {
        Value::Null => JsValue::NULL,
        Value::Bool(v) => JsValue::from_bool(*v),
        Value::String(v) => JsValue::from_str(v),
        Value::Number(v) => {
            const MAX_SAFE: u64 = 9_007_199_254_740_991;
            let large = v.as_u64().is_some_and(|n| n > MAX_SAFE)
                || v.as_i64().is_some_and(|n| n < -(MAX_SAFE as i64));
            if large {
                js_sys::BigInt::new(&JsValue::from_str(&v.to_string()))?.into()
            } else {
                JsValue::from_f64(v.as_f64().ok_or_else(|| error("invalid number"))?)
            }
        }
        Value::Array(values) => {
            let array = js_sys::Array::new();
            for value in values {
                array.push(&encode_value(value)?);
            }
            array.into()
        }
        Value::Object(values) => {
            // Null prototype preserves arbitrary serialized keys, including
            // "__proto__", as ordinary data without prototype setters.
            let object = js_sys::Object::create(JsValue::NULL.unchecked_ref::<js_sys::Object>());
            for (key, value) in values {
                js_sys::Reflect::set(&object, &JsValue::from_str(key), &encode_value(value)?)?;
            }
            object.into()
        }
    })
}

/// Privileged owner handle. Give agents only the Environment returned by environment().
#[wasm_bindgen(js_name = World)]
pub struct JsWorld {
    inner: Rc<RefCell<computerworld::World>>,
}
#[wasm_bindgen(js_class = World)]
impl JsWorld {
    #[wasm_bindgen(constructor)]
    pub fn new(definition: JsValue, seed: JsValue) -> Result<JsWorld, JsValue> {
        Ok(Self {
            inner: Rc::new(RefCell::new(
                computerworld::World::new(decode(definition)?, seed_value(seed)?).map_err(error)?,
            )),
        })
    }
    pub fn environment(&self, config: JsValue) -> Result<JsEnvironment, JsValue> {
        let config = decode(config)?;
        let session = self.inner.borrow_mut().environment(config).map_err(error)?;
        Ok(JsEnvironment {
            inner: self.inner.clone(),
            session,
        })
    }
    /// Reconnect an actor stored in a restored/forked checkpoint.
    pub fn session(&self, id: &str) -> Result<JsEnvironment, JsValue> {
        self.inner.borrow().validate_session(id).map_err(error)?;
        Ok(JsEnvironment {
            inner: self.inner.clone(),
            session: id.to_owned(),
        })
    }
    pub fn snapshot(&self) -> JsSnapshot {
        JsSnapshot {
            inner: self.inner.borrow().snapshot(),
        }
    }
    pub fn restore(&self, snapshot: &JsSnapshot) -> Result<(), JsValue> {
        self.inner
            .borrow_mut()
            .restore(&snapshot.inner)
            .map_err(error)
    }
    pub fn fork(&self, snapshot: &JsSnapshot) -> Result<JsWorld, JsValue> {
        Ok(Self {
            inner: Rc::new(RefCell::new(
                self.inner.borrow().fork(&snapshot.inner).map_err(error)?,
            )),
        })
    }
    pub fn reset(&self, seed: JsValue) -> Result<(), JsValue> {
        self.inner
            .borrow_mut()
            .reset(seed_value(seed)?)
            .map_err(error)
    }
    #[wasm_bindgen(js_name = exportSnapshot)]
    pub fn export_snapshot(&self) -> Result<String, JsValue> {
        self.inner.borrow().export_snapshot().map_err(error)
    }
    #[wasm_bindgen(js_name = importSnapshot)]
    pub fn import_snapshot(&self, json: &str) -> Result<(), JsValue> {
        self.inner.borrow_mut().import_snapshot(json).map_err(error)
    }
    /// Let `ticks` (microseconds) of world time pass with nobody acting.
    pub fn advance(&self, ticks: JsValue) -> Result<(), JsValue> {
        self.inner
            .borrow_mut()
            .advance(seed_value(ticks)?)
            .map_err(error)
    }
    /// The world clock in ticks (microseconds), as a BigInt-safe string.
    pub fn tick(&self) -> String {
        self.inner.borrow().tick().to_string()
    }
    #[wasm_bindgen(js_name = stateHash)]
    pub fn state_hash(&self) -> Result<String, JsValue> {
        self.inner.borrow().state_hash().map_err(error)
    }
    pub fn trajectory(&self) -> Result<JsValue, JsValue> {
        encode(&self.inner.borrow().trajectory())
    }
    /// The application modules this world's steps ran, as `{ name: sha256 }`: what a
    /// replay of its journal loads.
    #[wasm_bindgen(js_name = modulesUsed)]
    pub fn modules_used(&self) -> Result<JsValue, JsValue> {
        encode(&self.inner.borrow().modules_used())
    }
    pub fn definition(&self) -> Result<JsValue, JsValue> {
        encode(self.inner.borrow().definition())
    }
    /// Owner-only topology mutation. Wiring is explicit; no implicit host access.
    #[wasm_bindgen(js_name = addComputer)]
    pub fn add_computer(
        &self,
        computer: JsValue,
        node: JsValue,
        links: JsValue,
    ) -> Result<(), JsValue> {
        self.inner
            .borrow_mut()
            .add_computer(decode(computer)?, decode(node)?, decode(links)?)
            .map_err(error)
    }
    #[wasm_bindgen(js_name = removeComputer)]
    pub fn remove_computer(&self, id: &str) -> Result<(), JsValue> {
        self.inner.borrow_mut().remove_computer(id).map_err(error)
    }
    pub fn inspect(&self) -> Result<JsValue, JsValue> {
        encode(&self.inner.borrow().inspect())
    }
    /// One service's state, without copying the rest of the world.
    #[wasm_bindgen(js_name = serviceState)]
    pub fn service_state(&self, service: &str) -> Result<JsValue, JsValue> {
        encode(&self.inner.borrow().service_state(service).map_err(error)?)
    }
    /// Apply an RFC 7386 JSON merge patch to one service's state; journaled, so a
    /// replay makes the same write at the same tick.
    #[wasm_bindgen(js_name = patchServiceState)]
    pub fn patch_service_state(&self, service: &str, patch: JsValue) -> Result<(), JsValue> {
        let patch: serde_json::Value = decode(patch)?;
        self.inner
            .borrow_mut()
            .patch_service_state(service, &patch)
            .map_err(error)
    }
    /// Write a procedure instance's setup files (docs/expert-procedures.md).
    pub fn stage(&self, instance: JsValue) -> Result<(), JsValue> {
        let instance = decode(instance)?;
        self.inner.borrow_mut().stage(&instance).map_err(error)
    }
    /// Judge an instance's checks; `captures` (a demonstration's, optional) fills their
    /// `{{@name}}` placeholders.
    pub fn verify(&self, instance: JsValue, captures: JsValue) -> Result<JsValue, JsValue> {
        let instance = decode(instance)?;
        let captures: std::collections::BTreeMap<String, String> =
            if captures.is_undefined() || captures.is_null() {
                Default::default()
            } else {
                decode(captures)?
            };
        encode(&self.inner.borrow().verify_with(&instance, &captures))
    }
    /// Perform one gesture (`{do: "click", target: {label: "Post"}}`) through `session`
    /// and return the steps it took.
    pub fn perform(
        &self,
        session: &str,
        gesture: JsValue,
        options: JsValue,
    ) -> Result<JsValue, JsValue> {
        let gesture = decode(gesture)?;
        let options = if options.is_undefined() || options.is_null() {
            Default::default()
        } else {
            decode(options)?
        };
        let steps = self
            .inner
            .borrow_mut()
            .perform(session, &gesture, options)
            .map_err(error)?;
        encode(&steps)
    }
    /// Perform an instance and return the demonstration. `sessions` is one session id,
    /// or `{role: session id}` for a team procedure.
    pub fn project(
        &self,
        sessions: JsValue,
        instance: JsValue,
        options: JsValue,
    ) -> Result<JsValue, JsValue> {
        let instance = decode(instance)?;
        let options = if options.is_undefined() || options.is_null() {
            Default::default()
        } else {
            decode(options)?
        };
        let mut world = self.inner.borrow_mut();
        let demonstration = match sessions.as_string() {
            Some(session) => world.project(&session, &instance, options),
            None => world.project_team(&decode(sessions)?, &instance, options),
        }
        .map_err(error)?;
        encode(&demonstration)
    }
}
/// Draw a procedure's parameters from `seed` and substitute them: one concrete episode.
#[wasm_bindgen]
pub fn instantiate(
    procedure: JsValue,
    seed: JsValue,
    overrides: JsValue,
) -> Result<JsValue, JsValue> {
    let procedure: computerworld::procedure::Procedure = decode(procedure)?;
    let overrides = if overrides.is_undefined() || overrides.is_null() {
        Default::default()
    } else {
        decode(overrides)?
    };
    encode(
        &procedure
            .instantiate(seed_value(seed)?, &overrides)
            .map_err(error)?,
    )
}

#[wasm_bindgen(js_name = Snapshot)]
pub struct JsSnapshot {
    inner: computerworld::Snapshot,
}

/// Restricted actor handle: no privileged inspection, topology export, or snapshots.
#[wasm_bindgen(js_name = Environment)]
pub struct JsEnvironment {
    inner: Rc<RefCell<computerworld::World>>,
    session: String,
}
#[wasm_bindgen(js_class = Environment)]
impl JsEnvironment {
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.session.clone()
    }

    pub fn step(&self, actions: JsValue) -> Result<JsValue, JsValue> {
        let actions = decode(actions)?;
        encode(
            &self
                .inner
                .borrow_mut()
                .step(&self.session, actions)
                .map_err(error)?,
        )
    }
    pub fn observe(&self) -> Result<JsValue, JsValue> {
        encode(&self.inner.borrow().observe(&self.session).map_err(error)?)
    }
    pub fn scene(&self, width: u32, height: u32) -> Result<JsValue, JsValue> {
        encode(
            &self
                .inner
                .borrow()
                .scene(&self.session, width, height)
                .map_err(error)?,
        )
    }
    pub fn render(&self, width: u32, height: u32) -> Result<JsFrame, JsValue> {
        let frame = self
            .inner
            .borrow_mut()
            .render(&self.session, width, height)
            .map_err(error)?;
        Ok(JsFrame {
            width: frame.width,
            height: frame.height,
            rgba: frame.rgba,
        })
    }
    /// Render into a buffer the caller already holds — an `ImageData`'s `data`, in
    /// practice — so the pixels cross into JavaScript exactly once. `render` copies the
    /// frame out of the renderer, then again into a fresh `Uint8Array`, and a canvas
    /// wants a `Uint8ClampedArray` after that; a screen redrawn on every pointer move is
    /// three copies of four megabytes that nothing reads.
    #[wasm_bindgen(js_name = renderInto)]
    pub fn render_into(
        &self,
        width: u32,
        height: u32,
        out: &js_sys::Uint8ClampedArray,
    ) -> Result<(), JsValue> {
        self.inner
            .borrow_mut()
            .render_with(&self.session, width, height, |frame| {
                if out.length() as usize != frame.rgba.len() {
                    return Err(error(format!(
                        "the buffer holds {} bytes; a {width}x{height} frame is {}",
                        out.length(),
                        frame.rgba.len()
                    )));
                }
                out.copy_from(&frame.rgba);
                Ok(())
            })
            .map_err(error)?
    }
}
#[wasm_bindgen(js_name = Frame)]
pub struct JsFrame {
    pub width: u32,
    pub height: u32,
    rgba: Vec<u8>,
}
#[wasm_bindgen(js_class = Frame)]
impl JsFrame {
    #[wasm_bindgen(getter)]
    pub fn rgba(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.rgba.as_slice())
    }
}
#[wasm_bindgen(js_name = createWorld)]
pub fn create_world(definition: JsValue, seed: JsValue) -> Result<JsWorld, JsValue> {
    JsWorld::new(definition, seed)
}

/// Standalone retained renderer for synthetic applications and measurement.
#[wasm_bindgen(js_name = SceneRenderer)]
pub struct JsSceneRenderer {
    scene: Option<cw_scene::Scene>,
    renderer: cw_render::Renderer,
}
#[wasm_bindgen(js_class = SceneRenderer)]
impl JsSceneRenderer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            scene: None,
            renderer: cw_render::Renderer::new(),
        }
    }
    pub fn render(&mut self, scene: JsValue) -> Result<JsFrame, JsValue> {
        let scene: cw_scene::Scene = serde_wasm_bindgen::from_value(scene).map_err(error)?;
        scene.validate().map_err(error)?;
        let frame = self.renderer.render(&scene);
        self.scene = Some(scene);
        Ok(JsFrame {
            width: frame.width,
            height: frame.height,
            rgba: frame.rgba,
        })
    }
    pub fn patch(&mut self, patch: JsValue) -> Result<JsFrame, JsValue> {
        let scene = self
            .scene
            .as_mut()
            .ok_or_else(|| error("render a scene before applying patches"))?;
        let damage = scene
            .patch(serde_wasm_bindgen::from_value(patch).map_err(error)?)
            .map_err(error)?;
        let frame = self.renderer.render_incremental(scene, &damage);
        Ok(JsFrame {
            width: frame.width,
            height: frame.height,
            rgba: frame.rgba.clone(),
        })
    }
}
impl Default for JsSceneRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Install one on-demand font file (`fonts/<file>` beside this module): a file of the
/// CJK/emoji pack, or a web family's face in a `lazy-fonts` build. Without a module
/// host, a pack face draws as `.notdef` boxes until installed and a frame that needs a
/// missing web face is refused; layout is final either way. Bytes are identified by
/// SHA-256, so only the exact files this build was made with are accepted, and every
/// renderer drops its cached text on the next frame. Returns the file name.
#[wasm_bindgen(js_name = installFont)]
pub fn install_font(bytes: &[u8]) -> Result<String, JsValue> {
    cw_render::install_font(bytes)
        .map(str::to_owned)
        .map_err(error)
}

/// The font pack: every file with its SHA-256 and size, which are installed, and
/// which renderers have needed but not had (fetch exactly those, then re-render).
#[wasm_bindgen(js_name = fontPackStatus)]
pub fn font_pack_status() -> Result<JsValue, JsValue> {
    let status = cw_render::font_pack_status();
    let files: Vec<serde_json::Value> = cw_render::FONT_PACK
        .iter()
        .map(|file| {
            serde_json::json!({
                "file": file.file,
                "path": format!("fonts/{}", file.file),
                "sha256": file.sha256,
                "bytes": file.bytes,
                "installed": status.installed.contains(&file.file),
            })
        })
        .collect();
    encode(&serde_json::json!({
        "files": files,
        "installed": status.installed,
        "missing": status.missing,
    }))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}
/// A panic aborts the module with no message of its own (`unreachable`); this says why
/// on the console first.
#[wasm_bindgen(start)]
fn start() {
    std::panic::set_hook(Box::new(|info| {
        console_error(&format!("computerworld: {info}"))
    }));
}

thread_local! {
    static MODULE_HOST: RefCell<Option<JsValue>> = const { RefCell::new(None) };
}
/// Call `method` of the module host with `args`; the host's exception is the error.
fn host_call(method: &str, args: &js_sys::Array) -> Result<JsValue, String> {
    MODULE_HOST.with(|host| {
        let host = host.borrow();
        let host = host
            .as_ref()
            .ok_or("no module host: call setModuleHost first")?;
        let function: js_sys::Function = js_sys::Reflect::get(host, &JsValue::from_str(method))
            .ok()
            .and_then(|f| f.dyn_into().ok())
            .ok_or_else(|| format!("the module host has no {method}()"))?;
        function.apply(host, args).map_err(|e| {
            e.dyn_ref::<js_sys::Error>()
                .map(|e| String::from(e.message()))
                .unwrap_or_else(|| format!("{e:?}"))
        })
    })
}
fn call_module(name: &str, sha256: &str, input: &[u8]) -> Result<Vec<u8>, String> {
    let args = js_sys::Array::of3(
        &JsValue::from_str(name),
        &JsValue::from_str(sha256),
        &js_sys::Uint8Array::from(input),
    );
    Ok(js_sys::Uint8Array::new(&host_call("call", &args)?).to_vec())
}
/// What a module asks the core while one of its calls runs, by channel (the first
/// byte): 0, the metric tables and faces text is measured with (`cw_scene::tables`);
/// 1, a host call of a program run by a language runtime's module (`cw_modules::script`).
/// A question this build cannot answer gets an empty answer.
fn module_core(request: &[u8]) -> Vec<u8> {
    match request.split_first() {
        Some((0, key)) => cw_scene::tables::export(key).unwrap_or_default(),
        Some((1, call)) => cw_modules::script::answer(call),
        _ => Vec::new(),
    }
}
fn load_data(file: &str, sha256: &str) -> Result<Vec<u8>, String> {
    let args = js_sys::Array::of2(&JsValue::from_str(file), &JsValue::from_str(sha256));
    Ok(js_sys::Uint8Array::new(&host_call("data", &args)?).to_vec())
}
fn load_font(file: &str, sha256: &str) -> Option<Vec<u8>> {
    load_file("font", file, sha256)
}
fn load_asset(file: &str, sha256: &str) -> Option<Vec<u8>> {
    load_file("asset", file, sha256)
}
fn load_file(method: &str, file: &str, sha256: &str) -> Option<Vec<u8>> {
    let args = js_sys::Array::of2(&JsValue::from_str(file), &JsValue::from_str(sha256));
    let bytes = host_call(method, &args).ok()?;
    (!bytes.is_null() && !bytes.is_undefined()).then(|| js_sys::Uint8Array::new(&bytes).to_vec())
}

/// Hand the engine the host that fetches what this build leaves out:
/// `{ call(name, sha256, request) -> Uint8Array, font(file,
/// sha256) -> Uint8Array, data(file, sha256) -> Uint8Array, asset(file, sha256) ->
/// Uint8Array }`, each answering at once
/// (the engine waits). `call` runs one request in module `name` (`modules/<name>.wasm`,
/// which must hash to `sha256`: an application, `python`, a service kind), instantiating
/// it the first time; `font` returns one on-demand font file (`fonts/<file>`) and `data`
/// one data file (`data/<file>`, the internet's world), `asset` one image the renderer
/// draws with (`assets/<file>`, a `lazy-render` build). The engine sets `host.core`, which
/// the host calls with a module's questions to the core (its import `cw.core`).
/// A build split into code modules (`code-modules`) also needs `link(name, sha256,
/// engine)`: the engine's glue calls it the first time a function of code module `name`
/// runs, and it links `modules/<name>.wasm` into this instance (`engine` is the
/// instance's exports) and returns the table the module's functions are in; the engine
/// sets `host.code`, the SHA-256 of each code module by name.
/// `computerworld/modules.cjs` and `modules.js` make one. `null` removes it: a frame that
/// needs a font that is not installed is then refused, and what a module serves is
/// unavailable.
#[wasm_bindgen(js_name = setModuleHost)]
pub fn set_module_host(host: JsValue) {
    let set = !host.is_null() && !host.is_undefined();
    if set {
        // A module asks the core while one of its calls runs (`cw.core`): the host
        // passes those questions to `host.core`.
        let core =
            wasm_bindgen::closure::Closure::<dyn Fn(js_sys::Uint8Array) -> js_sys::Uint8Array>::new(
                |request: js_sys::Uint8Array| {
                    js_sys::Uint8Array::from(&module_core(&request.to_vec())[..])
                },
            );
        let _ = js_sys::Reflect::set(&host, &JsValue::from_str("core"), &core.into_js_value());
    }
    #[cfg(feature = "code-modules")]
    if set {
        // The code modules this engine was split into, by name, with the SHA-256 each
        // must hash to: the glue asks `host.link` for one the first time it is called.
        let code = js_sys::Object::new();
        for (name, sha256) in code_modules() {
            let _ = js_sys::Reflect::set(
                &code,
                &JsValue::from_str(&name),
                &JsValue::from_str(&sha256),
            );
        }
        let _ = js_sys::Reflect::set(&host, &JsValue::from_str("code"), &code);
    }
    MODULE_HOST.with(|h| *h.borrow_mut() = set.then_some(host));
    cw_modules::set_host(set.then_some(call_module as cw_modules::ModuleHost));
    cw_modules::set_data_host(set.then_some(load_data as cw_modules::DataHost));
    cw_render::set_font_loader(set.then_some(load_font as cw_render::FontLoader));
    cw_render::set_asset_loader(set.then_some(load_asset as cw_render::FontLoader));
    #[cfg(feature = "lazy-fonts")]
    cw_scene::lazy_faces::set_face_loader(
        set.then_some(load_font as cw_scene::lazy_faces::FaceLoader),
    );
}

/// What this build leaves out and fetches when first needed: the modules it serves
/// (name, file, SHA-256, and `kind`: `call` for a module the engine calls, `code` for
/// a part of the engine's own code linked into it), the fonts fetched on first use (file, SHA-256, size, and
/// whether it is a web family, another platform's UI family, a script face or stub, a web
/// family's metric table (`metrics`, the first time web text is measured), the
/// CJK/emoji pack, or `render`: a face every frame draws with), the data files (file,
/// SHA-256) and the images the renderer draws with (`assets`, all of set `render`). The
/// `render` code module is of set `render` too. A client that drives a world through its
/// scene alone never needs a `render` file; one that draws fetches them all before its
/// first frame, as one file: the build packs the set into `render.bin`
/// (`preload(manifest, ['render'])`). A module host serves exactly
/// these files.
#[wasm_bindgen(js_name = lazyManifest)]
pub fn lazy_manifest() -> Result<JsValue, JsValue> {
    #[allow(unused_mut)]
    let mut modules: Vec<serde_json::Value> = computerworld::served_modules()
        .into_iter()
        .map(|(name, sha256)| {
            serde_json::json!({
                "name": name,
                "path": format!("modules/{name}.wasm"),
                "sha256": sha256,
                "kind": "call",
            })
        })
        .collect();
    #[cfg(feature = "code-modules")]
    modules.extend(code_modules().into_iter().map(|(name, sha256)| {
        let mut entry = serde_json::json!({
            "name": name,
            "path": format!("modules/{name}.wasm"),
            "sha256": sha256,
            "kind": "code",
        });
        // The renderer's own code is what the first frame needs before any file it draws
        // with, so it travels in the render bundle with them.
        if name == "render" {
            entry["set"] = "render".into();
        }
        entry
    }));
    let mut fonts: Vec<serde_json::Value> = Vec::new();
    let installed = cw_render::font_pack_status().installed;
    if cfg!(feature = "lazy-fonts") {
        for face in cw_render::WEB_FACES.iter() {
            fonts.push(serde_json::json!({
                "file": face.file, "path": format!("fonts/{}", face.file), "sha256": face.sha256,
                "bytes": face.bytes, "set": "web", "installed": installed.contains(&face.file),
            }));
        }
    }
    #[cfg(feature = "lazy-fonts")]
    for face in cw_render::fetched_ui_faces() {
        let set = if cw_render::draws_ui_slot(face.slot) {
            "render"
        } else {
            "ui"
        };
        fonts.push(serde_json::json!({
            "file": face.file, "path": format!("fonts/{}", face.file), "sha256": face.sha256,
            "bytes": face.bytes, "set": set, "installed": installed.contains(&face.file),
        }));
    }
    let mut assets: Vec<serde_json::Value> = Vec::new();
    if cw_render::fetches_render_files() {
        for file in cw_render::RENDER_FILES.iter() {
            let entry = serde_json::json!({
                "file": file.file.trim_start_matches("fonts/"), "path": file.path(),
                "sha256": file.sha256, "bytes": file.bytes, "set": "render",
            });
            if file.is_font() {
                fonts.push(entry)
            } else {
                assets.push(entry)
            }
        }
    }
    #[cfg(feature = "lazy-metrics")]
    for table in cw_scene::lazy_faces::METRIC_FILES.iter() {
        fonts.push(serde_json::json!({
            "file": table.file, "path": format!("fonts/{}", table.file), "sha256": table.sha256,
            "bytes": table.bytes, "set": "metrics",
        }));
    }
    #[cfg(feature = "lazy-fonts")]
    for face in cw_scene::lazy_faces::LAZY_FACES.iter() {
        fonts.push(serde_json::json!({
            "file": face.file, "path": format!("fonts/{}", face.file), "sha256": face.sha256,
            "bytes": face.bytes, "set": "script",
        }));
    }
    for file in cw_render::FONT_PACK.iter() {
        fonts.push(serde_json::json!({
            "file": file.file, "path": format!("fonts/{}", file.file), "sha256": file.sha256,
            "bytes": file.bytes, "set": "pack", "installed": installed.contains(&file.file),
        }));
    }
    #[allow(unused_mut)]
    let mut data: Vec<serde_json::Value> = Vec::new();
    // The node-app packages (docs/app-packages.md), fetched the first time a world's site
    // names one; set `packages`, so a host preloads them only for such a world.
    for p in &cw_packages::global().packages {
        if let cw_packages::Runtime::NodeApp {
            package,
            sha256: Some(sha256),
        } = &p.runtime
        {
            data.push(serde_json::json!({
                "file": format!("packages/{package}.bin"),
                "path": format!("data/packages/{package}.bin"),
                "sha256": sha256,
                "kind": "data",
                "set": "packages",
            }));
        }
    }
    #[cfg(feature = "internet-on-demand")]
    data.push(serde_json::json!({
        "file": computerworld::internet::DATA_FILE,
        "path": format!("data/{}", computerworld::internet::DATA_FILE),
        "sha256": computerworld::internet::DATA_SHA256,
    }));
    encode(
        &serde_json::json!({ "modules": modules, "fonts": fonts, "data": data, "assets": assets }),
    )
}

/// Install an application package at run time from its manifest's JSON
/// (docs/app-packages.md); its bytes come through the module host.
#[wasm_bindgen(js_name = installPackage)]
pub fn install_package(manifest: &str) -> Result<(), JsValue> {
    computerworld::install_package_json(manifest).map_err(|e| JsValue::from_str(&e.message))
}
/// The ids of the catalog's own packages, as a JSON array.
#[wasm_bindgen(js_name = catalogIds)]
pub fn catalog_ids() -> String {
    serde_json::to_string(&computerworld::catalog_ids()).unwrap_or_else(|_| "[]".into())
}

/// Version of the canonical engine embedded in this binding.
#[wasm_bindgen(js_name = engineVersion)]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// The code modules this engine was split into (`scripts/split-modules.mjs`), one
/// `<name> <sha256>` line each after the marker. The build writes them into the binary
/// after splitting it, so the engine pins exactly the files it was split into; the code
/// modules themselves hold none of this data, so writing it does not change their hashes.
#[cfg(feature = "code-modules")]
static CODE_MODULES: [u8; CODE_MODULES_SPACE] = code_modules_marker();
#[cfg(feature = "code-modules")]
const CODE_MODULES_MARKER: &[u8] = b"computerworld code modules:\n";
#[cfg(feature = "code-modules")]
const CODE_MODULES_SPACE: usize = 8192;
#[cfg(feature = "code-modules")]
const fn code_modules_marker() -> [u8; CODE_MODULES_SPACE] {
    let mut space = [0u8; CODE_MODULES_SPACE];
    let mut i = 0;
    while i < CODE_MODULES_MARKER.len() {
        space[i] = CODE_MODULES_MARKER[i];
        i += 1;
    }
    space
}
#[cfg(feature = "code-modules")]
fn code_modules() -> Vec<(String, String)> {
    // Read through volatile loads: the compiler must not fold the bytes it saw at build
    // time, which the split rewrites.
    let text: Vec<u8> = (CODE_MODULES_MARKER.len()..CODE_MODULES_SPACE)
        // SAFETY: an in-bounds read of an immutable static.
        .map(|i| unsafe { std::ptr::read_volatile(&CODE_MODULES[i]) })
        .take_while(|&b| b != 0)
        .collect();
    String::from_utf8_lossy(&text)
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(name, sha256)| (name.to_owned(), sha256.to_owned()))
        .collect()
}
