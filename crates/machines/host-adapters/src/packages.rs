//! Serving the engine's data files from the host's disk: the node-app packages packed
//! from their sources under `packages/node/<id>/`, or any `data/<file>` from a
//! directory a build wrote (`pkg/web/data`). Native builds (tests, the Python wheel,
//! the fidelity tool) have no module host; this is what answers `cw_modules::data`
//! for them. Every answer is checked against the SHA-256 the engine asked for.
use cw_service_node_app::Package;
use sha2::Digest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

static SOURCE_ROOTS: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());
static DATA_DIRS: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// Answer `data/packages/<id>.bin` by packing `root/<id>/` the way
/// `scripts/pack-node-apps.mjs` does, so the catalog's pinned hash holds.
pub fn serve_node_app_sources(root: impl Into<PathBuf>) {
    SOURCE_ROOTS
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .push(root.into());
    cw_modules::set_data_host(Some(data_host));
}

/// Answer `data/<file>` from `dir/<file>` (a built package's `data/` directory).
pub fn serve_data_dir(dir: impl Into<PathBuf>) {
    DATA_DIRS
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .push(dir.into());
    cw_modules::set_data_host(Some(data_host));
}

fn walk(dir: &Path, rel: &str, out: &mut BTreeMap<String, Vec<u8>>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if e.file_type()?.is_dir() {
            walk(&e.path(), &path, out)?;
        } else {
            out.insert(path, std::fs::read(e.path())?);
        }
    }
    Ok(())
}

/// The archive of the package whose sources are in `dir`.
pub fn pack_sources(id: &str, dir: &Path) -> Result<Vec<u8>, String> {
    let mut files = BTreeMap::new();
    walk(dir, "", &mut files).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(Package::archive(id, &files))
}

fn data_host(file: &str, sha256: &str) -> Result<Vec<u8>, String> {
    let check = |bytes: Vec<u8>, from: String| {
        let got = format!("{:x}", sha2::Sha256::digest(&bytes));
        if got == sha256 {
            Ok(bytes)
        } else {
            Err(format!(
                "{from}: SHA-256 {got} is not the {sha256} this build pins"
            ))
        }
    };
    for dir in DATA_DIRS.read().unwrap_or_else(|e| e.into_inner()).iter() {
        let p = dir.join(file);
        if p.is_file() {
            let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            return check(bytes, p.display().to_string());
        }
    }
    if let Some(id) = file
        .strip_prefix("packages/")
        .and_then(|f| f.strip_suffix(".bin"))
    {
        for root in SOURCE_ROOTS
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
        {
            let dir = root.join(id);
            if dir.is_dir() {
                return check(pack_sources(id, &dir)?, dir.display().to_string());
            }
        }
    }
    Err(format!(
        "data/{file} is not under any directory this host serves"
    ))
}
