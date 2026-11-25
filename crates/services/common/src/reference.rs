//! A site as the reference world has it, for a service's tests.
//!
//! The internet's sites are neutral; the reference company's people and content on them are
//! its overlay, from `worlds/company-2026/services/<id>/overlay.json` by way of the built
//! `worlds/company-2026/world.json`. A test that pins that content reads the
//! site through here and gets both, merged exactly as the internet joins the world. One of
//! the company's own sites (its intranet, its blogs) is read as it is.
use serde_json::Value;
use std::path::PathBuf;

/// The open worlds (the internet), beside this crate.
fn worlds() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../worlds"))
}

/// The reference company world's folder. It is not in this repository: `CW_REFERENCE_WORLDS` names the folder that
/// holds it, or it is found above the running test's directory (a dependant whose tree has `worlds/company-2026`,
/// whether this crate is built from its checkout or from a git dependency).
fn company() -> PathBuf {
    if let Some(dir) = std::env::var_os("CW_REFERENCE_WORLDS") {
        return PathBuf::from(dir).join("company-2026");
    }
    let here = std::env::current_dir().expect("a working directory");
    here.ancestors()
        .map(|d| d.join("worlds/company-2026"))
        .find(|d| d.join("world.json").is_file())
        .unwrap_or_else(|| {
            panic!(
                "no worlds/company-2026 above {}: the reference company world is not public; set \
                 CW_REFERENCE_WORLDS to the folder that holds it",
                here.display()
            )
        })
}

fn read(path: PathBuf) -> Option<Value> {
    let text = std::fs::read_to_string(&path).ok()?;
    Some(serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
}

/// The reference company's own sites' folder.
pub fn company_services() -> PathBuf {
    company().join("services")
}

/// The internet's sites' folder (the open world).
pub fn internet_services() -> PathBuf {
    worlds().join("internet/services")
}

/// The reference company's overlays as its world file holds them: resolved, so an overlay
/// that reads its attachments `from_dir` arrives with the files' bytes and not the directive.
fn overlays() -> &'static serde_json::Map<String, Value> {
    static OVERLAYS: std::sync::OnceLock<serde_json::Map<String, Value>> =
        std::sync::OnceLock::new();
    OVERLAYS.get_or_init(|| {
        let world = read(company().join("world.json")).expect("the reference world");
        world["internet_overlays"]
            .as_object()
            .cloned()
            .unwrap_or_default()
    })
}

/// The site file with this id, with the reference company's overlay applied if it has one.
pub fn reference_site(id: &str) -> Value {
    let worlds = worlds();
    if let Some(own) = read(company().join(format!("services/{id}/service.json"))) {
        return own;
    }
    let base = read(worlds.join(format!("internet/services/{id}/service.json")))
        .unwrap_or_else(|| panic!("no site {id} in the internet or the reference company"));
    let Some(overlay) = overlays().get(id).cloned() else {
        return base;
    };
    let site: cw_protocol::ServiceDefinition =
        serde_json::from_value(base.clone()).expect("a site file is a service definition");
    let overlay: cw_protocol::SiteOverlay =
        serde_json::from_value(overlay).expect("an overlay file is a site overlay");
    let joined = cw_internet::overlaid(&site, &overlay);
    // Keep the file's own keys (`place` and the like) and take the joined service's.
    let mut out = base;
    for (key, value) in serde_json::to_value(joined)
        .expect("serialises")
        .as_object()
        .unwrap()
    {
        out[key] = value.clone();
    }
    out
}

/// [`reference_site`] as JSON text, for a test written against `include_str!`.
pub fn reference_site_json(id: &str) -> &'static str {
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    static CACHE: Mutex<BTreeMap<String, &'static str>> = Mutex::new(BTreeMap::new());
    let mut cache = CACHE.lock().unwrap();
    cache
        .entry(id.to_owned())
        .or_insert_with(|| Box::leak(reference_site(id).to_string().into_boxed_str()))
}

/// [`reference_site`] written to a file, for a test that reads its site from disk. The file
/// lives in this process's own temporary directory and is written once.
pub fn reference_site_path(id: impl AsRef<str>) -> String {
    let id = id.as_ref();
    let dir = std::env::temp_dir().join(format!("cw-reference-sites-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    let path = dir.join(format!("{id}.json"));
    if !path.exists() {
        // Written aside and renamed, so a test running beside this one never reads half a file.
        let partial = dir.join(format!("{id}.json.{:?}", std::thread::current().id()));
        std::fs::write(&partial, reference_site_json(id)).expect("the site is writable");
        std::fs::rename(&partial, &path).expect("the site is movable");
    }
    path.to_string_lossy().into_owned()
}

/// Every service of the reference world as it boots: the company's own, and the internet's
/// with the company's overlays on them.
pub fn reference_services() -> Vec<cw_protocol::ServiceDefinition> {
    let text = std::fs::read_to_string(company().join("world.json")).expect("the reference world");
    let mut world = cw_protocol::WorldDefinition::from_json(&text).expect("a valid world");
    cw_internet::join(&mut world).expect("the reference world joins the internet");
    world.services
}
