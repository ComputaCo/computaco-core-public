//! An app's installed packages read from disk, for a build on the host (the fixtures'
//! `build.rs`, the tests): the one place the compiler reads files.
use crate::{build_modules, load_with, Build, Diagnostic, LoadOptions};
use std::collections::BTreeMap;

/// [`crate::build_virtual`] with npm packages read from `packages` (a directory holding
/// them, as `node_modules` does), which `node_modules/…` paths name.
pub fn build_virtual_with(
    files: &[(String, String)],
    packages: Option<&std::path::Path>,
) -> Result<Build, Vec<Diagnostic>> {
    let map: BTreeMap<&str, &str> = files
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    let entry = files.first().map(|(n, _)| n.clone()).unwrap_or_default();
    let options = LoadOptions {
        aliases: Vec::new(),
        node_modules: packages.map(|_| "node_modules".to_owned()),
        env: Default::default(),
    };
    let mut read = |f: &str| {
        if let Some(s) = map.get(f) {
            return Some((*s).to_owned());
        }
        let rest = f.strip_prefix("node_modules/")?;
        std::fs::read_to_string(packages?.join(rest)).ok()
    };
    let (sources, errors) = load_with(&entry, &mut read, &options);
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(build_modules(&sources))
}
