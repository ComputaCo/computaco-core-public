//! What a lazy build fetches the first time it needs it (docs/app-modules.md): the
//! module host the engine asks, the record of which modules each step ran, and the
//! interface a language runtime served by a module calls the machine through
//! ([`script`]).
//!
//! **Replay loads the same code.** A module is asked for by name and the SHA-256 the
//! engine was built with; the host must answer with exactly those bytes. Every call goes
//! through [`call`], which notes the module, so a step records what it ran
//! ([`take_used`]) and a replay or a restore refuses a build that serves other code.
use std::cell::RefCell;
use std::collections::BTreeMap;

pub mod script;
pub mod wire;

/// Runs one call in a module: `(name, sha256, request) -> reply`, instantiating the
/// module (and fetching it, waiting for it) the first time. The host must refuse bytes
/// whose SHA-256 is not `sha256`. A request whose first byte is [`REENTER`] goes to
/// the instance of that module whose call is running (a call back into it while it
/// waits on the core); any other request to an instance that is free, or to a new one.
pub type ModuleHost = fn(name: &str, sha256: &str, input: &[u8]) -> Result<Vec<u8>, String>;
/// The first byte of a request for the module's running call (see [`ModuleHost`]).
pub const REENTER: u8 = 0;

static HOST: std::sync::RwLock<Option<ModuleHost>> = std::sync::RwLock::new(None);
/// Install (or remove) the host modules are fetched and called through.
pub fn set_host(host: Option<ModuleHost>) {
    *HOST.write().unwrap_or_else(|e| e.into_inner()) = host;
}

thread_local! {
    static USED: RefCell<BTreeMap<String, String>> = const { RefCell::new(BTreeMap::new()) };
}
/// The modules called on this thread since the last call, as (name, SHA-256).
pub fn take_used() -> BTreeMap<String, String> {
    USED.with(|u| std::mem::take(&mut *u.borrow_mut()))
}

/// One call to module `name`, built into this engine with `sha256`.
pub fn call(name: &str, sha256: Option<&str>, input: &[u8]) -> Result<Vec<u8>, String> {
    let sha256 = sha256.ok_or_else(|| format!("this build pins no module {name}"))?;
    let host = (*HOST.read().unwrap_or_else(|e| e.into_inner())).ok_or_else(|| {
        format!("{name} is a module of its own and this engine has no module host to load it")
    })?;
    USED.with(|u| u.borrow_mut().insert(name.to_owned(), sha256.to_owned()));
    host(name, sha256, input)
}

static DECLARED: std::sync::Mutex<BTreeMap<String, String>> =
    std::sync::Mutex::new(BTreeMap::new());
/// Note a module a crate serves from here on (the service kinds, when their registry is
/// made), so a snapshot or a replay can check this build serves what a run ran.
pub fn declare(name: &str, sha256: &str) {
    DECLARED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(name.to_owned(), sha256.to_owned());
}
/// Every module declared with [`declare`], as (name, SHA-256).
pub fn declared() -> BTreeMap<String, String> {
    DECLARED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Answers one data file by name and SHA-256 (`data/<file>`), or why it cannot.
pub type DataHost = fn(file: &str, sha256: &str) -> Result<Vec<u8>, String>;
static DATA: std::sync::RwLock<Option<DataHost>> = std::sync::RwLock::new(None);
/// Install (or remove) the host data files are fetched through.
pub fn set_data_host(host: Option<DataHost>) {
    *DATA.write().unwrap_or_else(|e| e.into_inner()) = host;
}
/// Data file `file`, which this engine was built with as `sha256`. The host must refuse
/// other bytes; they are checked here as well.
pub fn data(file: &str, sha256: &str) -> Result<Vec<u8>, String> {
    let host = (*DATA.read().unwrap_or_else(|e| e.into_inner())).ok_or_else(|| {
        format!("{file} is a file of its own and this engine has no host to load it")
    })?;
    host(file, sha256)
}
