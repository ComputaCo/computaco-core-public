//! Object conversion and ownership only; simulation lives in the Rust facade.
//!
//! The handles are safe to hand to another thread. A [`World`] and every [`Environment`]
//! minted from it share one `Mutex` around the Rust world, so calls from several threads
//! are serialized rather than racing, and a call that arrives while another is running
//! waits for it. What is *not* safe is two threads driving one world expecting two
//! independent simulations: the world is one state machine, and the order its actions
//! land in is the order the threads acquired the lock. See `docs/python.md`.
use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
    types::{PyBytes, PyDict},
};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{Arc, Mutex, MutexGuard};

/// mimalloc rather than the system allocator: the engine makes and frees small
/// objects at a high rate (a VM heap, DOM and style structures), and the
/// determinism corpus runs 15-23% faster on it with the same hashes.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
fn err(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}
/// The shared world, or a Python exception explaining why it is unusable. A panic
/// inside the simulation poisons the lock; every later call then raises instead of
/// aborting the interpreter, which is what an `unsendable` class used to do.
fn hold(world: &Arc<Mutex<cw::World>>) -> PyResult<MutexGuard<'_, cw::World>> {
    world.lock().map_err(|_| {
        PyRuntimeError::new_err(
            "the simulation panicked during an earlier call and this world can no longer \
             be used; build a new World, or restore one from an exported snapshot",
        )
    })
}
fn decode<T: DeserializeOwned>(value: &Bound<'_, PyAny>) -> PyResult<T> {
    let json: String = value
        .py()
        .import("json")?
        .call_method1("dumps", (value,))?
        .extract()?;
    serde_json::from_str(&json).map_err(err)
}
fn encode(py: Python<'_>, value: &impl Serialize) -> PyResult<PyObject> {
    Ok(py
        .import("json")?
        .call_method1("loads", (serde_json::to_string(value).map_err(err)?,))?
        .unbind())
}
#[pyclass]
struct World {
    inner: Arc<Mutex<cw::World>>,
}
#[pymethods]
impl World {
    #[new]
    #[pyo3(signature = (definition, seed=0))]
    fn new(definition: &Bound<'_, PyAny>, seed: u64) -> PyResult<Self> {
        Ok(Self {
            inner: Arc::new(Mutex::new(
                cw::World::new(decode(definition)?, seed).map_err(err)?,
            )),
        })
    }
    fn environment(&self, config: &Bound<'_, PyAny>) -> PyResult<Environment> {
        let config = decode(config)?;
        let session = hold(&self.inner)?.environment(config).map_err(err)?;
        Ok(Environment {
            inner: self.inner.clone(),
            session,
        })
    }
    fn session(&self, id: &str) -> PyResult<Environment> {
        hold(&self.inner)?.validate_session(id).map_err(err)?;
        Ok(Environment {
            inner: self.inner.clone(),
            session: id.to_owned(),
        })
    }
    fn snapshot(&self) -> PyResult<Snapshot> {
        Ok(Snapshot {
            inner: hold(&self.inner)?.snapshot(),
        })
    }
    fn restore(&self, snapshot: &Snapshot) -> PyResult<()> {
        hold(&self.inner)?.restore(&snapshot.inner).map_err(err)
    }
    fn fork(&self, snapshot: &Snapshot) -> PyResult<Self> {
        let forked = hold(&self.inner)?.fork(&snapshot.inner).map_err(err)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(forked)),
        })
    }
    #[pyo3(signature = (seed=0))]
    fn reset(&self, seed: u64) -> PyResult<()> {
        hold(&self.inner)?.reset(seed).map_err(err)
    }
    fn export_snapshot(&self) -> PyResult<String> {
        hold(&self.inner)?.export_snapshot().map_err(err)
    }
    fn import_snapshot(&self, json: &str) -> PyResult<()> {
        hold(&self.inner)?.import_snapshot(json).map_err(err)
    }
    /// Let `ticks` (microseconds) of world time pass with nobody acting.
    fn advance(&self, py: Python<'_>, ticks: u64) -> PyResult<()> {
        py.allow_threads(|| hold(&self.inner)?.advance(ticks).map_err(err))
    }
    /// The world clock in ticks (microseconds).
    fn tick(&self) -> PyResult<u64> {
        Ok(hold(&self.inner)?.tick())
    }
    fn state_hash(&self) -> PyResult<String> {
        hold(&self.inner)?.state_hash().map_err(err)
    }
    fn trajectory(&self, py: Python<'_>) -> PyResult<PyObject> {
        let trajectory = hold(&self.inner)?.trajectory().clone();
        encode(py, &trajectory)
    }
    fn definition(&self, py: Python<'_>) -> PyResult<PyObject> {
        let definition = hold(&self.inner)?.definition().clone();
        encode(py, &definition)
    }
    fn add_computer(
        &self,
        computer: &Bound<'_, PyAny>,
        node: &Bound<'_, PyAny>,
        links: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let (computer, node, links) = (decode(computer)?, decode(node)?, decode(links)?);
        hold(&self.inner)?
            .add_computer(computer, node, links)
            .map_err(err)
    }
    fn remove_computer(&self, id: &str) -> PyResult<()> {
        hold(&self.inner)?.remove_computer(id).map_err(err)
    }
    fn inspect(&self, py: Python<'_>) -> PyResult<PyObject> {
        let inspected = hold(&self.inner)?.inspect();
        encode(py, &inspected)
    }
    /// One service's state, without copying the rest of the world.
    fn service_state(&self, py: Python<'_>, service: &str) -> PyResult<PyObject> {
        let state = hold(&self.inner)?.service_state(service).map_err(err)?;
        encode(py, &state)
    }
    /// Apply an RFC 7386 JSON merge patch to one service's state (objects merge key by
    /// key, None removes a key); journaled, so a replay makes the same write at the same
    /// tick.
    fn patch_service_state(&self, service: &str, patch: &Bound<'_, PyAny>) -> PyResult<()> {
        let patch: serde_json::Value = decode(patch)?;
        hold(&self.inner)?
            .patch_service_state(service, &patch)
            .map_err(err)
    }
    /// Tell one service something under a token (a model's reply to one of its `asks`,
    /// or `wake`); journaled with the data, so a replay tells it again and asks nothing.
    fn tell_service(&self, service: &str, token: &str, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let data: serde_json::Value = decode(data)?;
        hold(&self.inner)?
            .tell_service(service, token, &data)
            .map_err(err)
    }
    /// Write a procedure instance's setup files (docs/expert-procedures.md).
    fn stage(&self, instance: &Bound<'_, PyAny>) -> PyResult<()> {
        let instance = decode(instance)?;
        hold(&self.inner)?.stage(&instance).map_err(err)
    }
    /// Judge an instance's checks; `captures` (a demonstration's) fills their
    /// `{{@name}}` placeholders.
    #[pyo3(signature = (instance, captures=None))]
    fn verify(
        &self,
        py: Python<'_>,
        instance: &Bound<'_, PyAny>,
        captures: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let instance = decode(instance)?;
        let captures = captures.map(decode).transpose()?.unwrap_or_default();
        let verification = hold(&self.inner)?.verify_with(&instance, &captures);
        encode(py, &verification)
    }
    /// Perform one gesture (`{"do": "click", "target": {"label": "Post"}}`) through
    /// `session` and return the steps it took.
    #[pyo3(signature = (session, gesture, options=None))]
    fn perform(
        &self,
        py: Python<'_>,
        session: &str,
        gesture: &Bound<'_, PyAny>,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let gesture = decode(gesture)?;
        let options = options.map(decode).transpose()?.unwrap_or_default();
        let steps = py.allow_threads(|| {
            hold(&self.inner)?
                .perform(session, &gesture, options)
                .map_err(err)
        })?;
        encode(py, &steps)
    }
    /// Perform an instance and return the demonstration. `sessions` is one session id,
    /// or `{role: session id}` for a team procedure.
    #[pyo3(signature = (sessions, instance, options=None))]
    fn project(
        &self,
        py: Python<'_>,
        sessions: &Bound<'_, PyAny>,
        instance: &Bound<'_, PyAny>,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyObject> {
        let instance = decode(instance)?;
        let options = options.map(decode).transpose()?.unwrap_or_default();
        let mut world = hold(&self.inner)?;
        let demonstration = match sessions.extract::<String>() {
            Ok(session) => world.project(&session, &instance, options),
            Err(_) => world.project_team(&decode(sessions)?, &instance, options),
        }
        .map_err(err)?;
        encode(py, &demonstration)
    }
}
/// Install an application package at run time from its manifest (a dict or JSON text);
/// its bytes come from `CW_DATA_DIR` or `CW_PACKAGES_DIR` (docs/app-packages.md).
#[pyfunction]
fn install_package(manifest: &Bound<'_, PyAny>) -> PyResult<()> {
    let text = match manifest.extract::<String>() {
        Ok(s) => s,
        Err(_) => serde_json::to_string(&decode::<serde_json::Value>(manifest)?)
            .map_err(|e| err(cw::SimError::invalid(e.to_string())))?,
    };
    cw::install_package_json(&text).map_err(err)
}

/// Draw a procedure's parameters from `seed` and substitute them: one concrete episode.
#[pyfunction]
#[pyo3(signature = (procedure, seed=0, overrides=None))]
fn instantiate(
    py: Python<'_>,
    procedure: &Bound<'_, PyAny>,
    seed: u64,
    overrides: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let procedure: cw::procedure::Procedure = decode(procedure)?;
    let overrides = overrides.map(decode).transpose()?.unwrap_or_default();
    let instance = procedure.instantiate(seed, &overrides).map_err(err)?;
    encode(py, &instance)
}
#[pyclass]
struct Snapshot {
    inner: cw::Snapshot,
}
#[pyclass]
struct Environment {
    inner: Arc<Mutex<cw::World>>,
    session: String,
}
#[pymethods]
impl Environment {
    #[getter]
    fn id(&self) -> &str {
        &self.session
    }

    fn step(&self, py: Python<'_>, actions: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let actions = decode(actions)?;
        let result =
            py.allow_threads(|| hold(&self.inner)?.step(&self.session, actions).map_err(err))?;
        encode(py, &result)
    }
    fn observe(&self, py: Python<'_>) -> PyResult<PyObject> {
        let observation =
            py.allow_threads(|| hold(&self.inner)?.observe(&self.session).map_err(err))?;
        encode(py, &observation)
    }
    #[pyo3(signature = (width=1024, height=768))]
    fn scene(&self, py: Python<'_>, width: u32, height: u32) -> PyResult<PyObject> {
        let scene = py.allow_threads(|| {
            hold(&self.inner)?
                .scene(&self.session, width, height)
                .map_err(err)
        })?;
        encode(py, &scene)
    }
    #[pyo3(signature = (width=1024, height=768))]
    fn render(&self, py: Python<'_>, width: u32, height: u32) -> PyResult<PyObject> {
        let frame = py.allow_threads(|| {
            hold(&self.inner)?
                .render(&self.session, width, height)
                .map_err(err)
        })?;
        let result = PyDict::new(py);
        result.set_item("width", frame.width)?;
        result.set_item("height", frame.height)?;
        result.set_item("rgba", PyBytes::new(py, &frame.rgba))?;
        Ok(result.into_any().unbind())
    }
}
#[pymodule]
fn computerworld(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Native builds have no module host: `CW_DATA_DIR` (a build's `data/` directory) or
    // `CW_PACKAGES_DIR` (`packages/node/` sources) is where the node-app packages come from.
    if let Ok(dir) = std::env::var("CW_DATA_DIR") {
        cw_host_adapters::serve_data_dir(dir);
    }
    if let Ok(dir) = std::env::var("CW_PACKAGES_DIR") {
        cw_host_adapters::serve_node_app_sources(dir);
    }
    // The classes are only sound to share across threads because the world they hold
    // is; if that ever stops being true this line stops compiling rather than turning
    // into an abort at runtime. The heavy calls (step, perform, observe, scene, render,
    // advance) release the GIL, so worlds on different threads run in parallel.
    const fn shareable<T: Send>() {}
    shareable::<cw::World>();
    shareable::<cw::Snapshot>();
    m.add(
        "__version__",
        env!("CARGO_PKG_VERSION").replace("-alpha.", "a"),
    )?;
    m.add("engine_version", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<World>()?;
    m.add_class::<Environment>()?;
    m.add_class::<Snapshot>()?;
    m.add_function(wrap_pyfunction!(instantiate, m)?)?;
    m.add_function(wrap_pyfunction!(install_package, m)?)?;
    Ok(())
}
