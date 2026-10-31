//! The application catalog as this build compiled it in (build.rs reads
//! `packages/catalog.json`): native kinds, modules, every platform's labels and shelf.
//! Packages installed at run time are in `cw_packages::global()`; these tables are the
//! built-in ones, constant so the shells and the dispatch pay nothing for them.
include!(concat!(env!("OUT_DIR"), "/catalog_gen.rs"));

/// What `platform` calls `kind`, when it offers it.
pub fn label(platform: &str, kind: &str) -> Option<&'static str> {
    let table: &[(&str, &str)] = match platform {
        "macos" => LABELS_MACOS,
        "windows" => LABELS_WINDOWS,
        "ubuntu" => LABELS_UBUNTU,
        "ios" => LABELS_IOS,
        "android" => LABELS_ANDROID,
        _ => return None,
    };
    table.iter().find(|(k, _)| *k == kind).map(|(_, l)| *l)
}
/// The casual name of `kind`.
pub fn name(kind: &str) -> Option<&'static str> {
    NAMES.iter().find(|(k, _)| *k == kind).map(|(_, n)| *n)
}
/// A kind as a `&'static str`: the catalog's are static already; one from a package
/// installed at run time is leaked once and reused (there are a handful, ever).
pub fn static_kind(kind: &str) -> &'static str {
    if let Some(k) = NATIVE_KINDS.iter().find(|k| **k == kind) {
        return k;
    }
    static INTERNED: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut interned = INTERNED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(k) = interned.iter().find(|k| **k == kind) {
        return k;
    }
    let leaked: &'static str = Box::leak(kind.to_owned().into_boxed_str());
    interned.push(leaked);
    leaked
}

/// Install a package at run time (docs/app-packages.md): into the catalog, and its code
/// where the engine looks for it — a module's name and hash into the dispatch table, a
/// web application's script into the web catalog. The engine's own packages cannot be
/// replaced this way.
pub fn install_package(package: &cw_packages::Package) -> Result<(), String> {
    if NATIVE_KINDS.contains(&package.id.as_str())
        || matches!(
            package.id.as_str(),
            "files" | "browser" | "terminal" | "editor"
        )
    {
        return Err(format!(
            "{} is one of the engine's own applications and cannot be replaced",
            package.id
        ));
    }
    match &package.runtime {
        cw_packages::Runtime::Native {
            module: Some(module),
            tag,
            sha256: Some(sha256),
            ..
        } => {
            #[cfg(feature = "app-modules")]
            super::module::register_module(
                module,
                vec![(
                    package.id.clone(),
                    tag.clone().unwrap_or_else(|| package.id.clone()),
                )],
                sha256,
            );
            #[cfg(not(feature = "app-modules"))]
            {
                let _ = (module, tag, sha256);
                return Err(format!(
                    "{}: this engine serves no application modules",
                    package.id
                ));
            }
        }
        cw_packages::Runtime::Native { .. } => {
            return Err(format!(
                "{}: a native package installed at run time names its module and its sha256",
                package.id
            ));
        }
        #[cfg(feature = "base-apps")]
        cw_packages::Runtime::Web {
            script: Some(script),
            style,
            react,
        } => {
            let titles = package
                .platforms
                .iter()
                .map(|(p, f)| (p.clone(), f.label.clone()))
                .collect();
            crate::web_app::define(cw_sdk::WebApplication {
                kind: package.id.clone(),
                version: 1,
                titles,
                source: cw_sdk::WebSource::Script {
                    script: script.clone(),
                    style: style.clone().unwrap_or_default(),
                    react: *react,
                },
            })?;
        }
        cw_packages::Runtime::Web { .. } => {
            return Err(format!(
                "{}: a web package installed at run time carries its script",
                package.id
            ));
        }
        cw_packages::Runtime::NodeApp { sha256: None, .. } => {
            return Err(format!(
                "{}: a node_app package names the sha256 of its archive",
                package.id
            ));
        }
        cw_packages::Runtime::NodeApp { .. } | cw_packages::Runtime::Engine => {}
    }
    cw_packages::install(package.clone())
}

/// The shelf of `platform`: (kind, icon, label) in display order.
pub fn shelf(platform: &str) -> &'static [(&'static str, &'static str, &'static str)] {
    match platform {
        "macos" => SHELF_MACOS,
        "windows" => SHELF_WINDOWS,
        "ubuntu" => SHELF_UBUNTU,
        "ios" => SHELF_IOS,
        "android" => SHELF_ANDROID,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_macro_kind_is_in_the_catalog_and_back() {
        // The code's applications and the catalog's native packages are one list.
        let mut code: Vec<&str> = crate::apps::NativeApp::KINDS.to_vec();
        code.sort_unstable();
        for k in &code {
            assert!(
                NATIVE_KINDS.contains(k),
                "{k} is compiled in but not in packages/catalog.json"
            );
        }
        assert!(!MODULES.is_empty());
        assert_eq!(label("ubuntu", "mail"), Some("Thunderbird Mail"));
        assert_eq!(name("kicad"), Some("KiCad"));
    }
}
