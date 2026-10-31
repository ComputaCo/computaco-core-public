//! The application catalog.
//!
//! Every application a simulated machine can install is a **package**: a manifest
//! saying what real product it imitates, how it runs (compiled into the engine, served
//! from a Wasm module, a web app, a Node app), and what each platform calls it. The
//! engine, the shells, the hosted API's host selection and the fidelity tooling all read
//! this one table instead of each keeping a list of their own.
//!
//! **Names.** A package's short id is the application kind worlds and procedures use
//! (`kicad`). Its qualified id adds what it imitates, as suffixes that are correct but
//! ignorable, the way a casual label drops a model's variant:
//! `kicad-8.0-ubuntu-desktop`. [`Reference::parse`] takes either, and
//! [`Catalog::resolve`] finds the package whichever a world wrote.
//!
//! The built-in catalog is `packages/catalog.json` at the repository root, embedded here;
//! a host may [`install`] further packages at run time (an account's own application),
//! and every reader goes through [`global`].
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};

pub const SCHEMA_VERSION: u32 = 1;
pub const BUILTIN_JSON: &str = include_str!("../../../../packages/catalog.json");

/// The platforms a package can be installed on; the shells' ids.
pub const PLATFORMS: [&str; 5] = ["macos", "windows", "ubuntu", "ios", "android"];

/// How a package's code reaches the engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Runtime {
    /// Part of the engine itself: the four applications the compositor implements
    /// (files, browser, terminal, editor).
    Engine,
    /// A native Rust application compiled into the engine when its feature is on, and
    /// otherwise served from `module` (`modules/<module>.wasm`) as the application
    /// serialized under `tag`.
    Native {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        module: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tag: Option<String>,
        /// The Cargo feature that carries it natively; none means the base set.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        feature: Option<String>,
        /// The module's SHA-256, for a package installed at run time (a build pins the
        /// catalog's own modules at build time).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
    /// A web application opened in its own window: a script that renders into `#root`
    /// (compiled from TSX ahead of time), with its stylesheet, on React 18 when `react`.
    /// The catalog's own web applications carry their code in the engine and leave
    /// `script` empty.
    Web {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        script: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        react: bool,
    },
    /// A published web application with a Node backend, served by the `node-app`
    /// service from a package of files fetched by hash.
    NodeApp {
        package: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
}

/// What a package looks like on one platform.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformFacts {
    /// The product it is on this platform: `Finder`, `Thunderbird Mail`, `KiCad 8.0`.
    pub label: String,
    /// The product's version on this platform, when the imitation targets one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The icon drawn for it, when another package's icon is reused (iOS's TextEdit
    /// draws the Pages icon).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// The real product a package imitates.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Imitates {
    pub product: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    /// Major.minor of the product the imitation targets, for the qualified id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    /// The application kind: what worlds install and procedures launch.
    pub id: String,
    /// The casual label, platform-free: `KiCad`, `Mail`, `Visual Studio Code`.
    pub name: String,
    pub runtime: Runtime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imitates: Option<Imitates>,
    /// Facts per platform; a platform absent here does not offer the package.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub platforms: BTreeMap<String, PlatformFacts>,
    /// `eda`, `cad`, `office`, `media`, `communication`, `system`, `utility`, `web`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    /// Who publishes it: `computaco` for ours.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub publisher: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    /// Glob of the interaction ids its pages expose (`window:*:content:kicad:*`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactions: Option<String>,
    /// Paths of the fidelity tooling's data for it, relative to the repository.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fidelity: BTreeMap<String, String>,
    /// Served by the hosted API only: an open-core build lists it but does not carry it,
    /// and refuses a world that installs it, saying so.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hosted_only: bool,
}
impl Package {
    pub fn label_on(&self, platform: &str) -> Option<&str> {
        self.platforms.get(platform).map(|p| p.label.as_str())
    }
    pub fn offered_on(&self, platform: &str) -> bool {
        self.platforms.contains_key(platform)
    }
    /// The module serving it when the engine does not carry it, and its tag.
    pub fn module(&self) -> Option<(&str, &str)> {
        match &self.runtime {
            Runtime::Native {
                module: Some(m),
                tag,
                ..
            } => Some((m.as_str(), tag.as_deref().unwrap_or(self.id.as_str()))),
            _ => None,
        }
    }
    pub fn feature(&self) -> Option<&str> {
        match &self.runtime {
            Runtime::Native { feature, .. } => feature.as_deref(),
            _ => None,
        }
    }
    /// The qualified id: short id, then the imitated version, the platform and the
    /// device, each only when known. `kicad-8.0-ubuntu-desktop`.
    pub fn qualified(&self, platform: Option<&str>, device: Option<&str>) -> String {
        let mut out = self.id.clone();
        let version = platform
            .and_then(|p| self.platforms.get(p))
            .and_then(|f| f.version.clone())
            .or_else(|| self.imitates.as_ref().and_then(|i| i.version.clone()));
        if let Some(v) = version {
            out.push('-');
            out.push_str(&v);
        }
        if let Some(p) = platform {
            out.push('-');
            out.push_str(p);
        }
        if let Some(d) = device {
            out.push('-');
            out.push_str(d);
        }
        out
    }
}

/// A reference to a package as a world writes it: the short id with any of the
/// ignorable suffixes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reference {
    pub id: String,
    pub version: Option<String>,
    pub platform: Option<String>,
    pub device: Option<String>,
}
pub const DEVICES: [&str; 4] = ["desktop", "laptop", "phone", "server"];
impl Reference {
    /// Suffixes are read from the right: a device, a platform, then a version
    /// (digits and dots). What is left is the id, which may itself contain dashes
    /// (`react-admin`). With `known` ids, the longest known prefix wins the split.
    pub fn parse(text: &str, known: &[&str]) -> Self {
        let text = text.trim();
        if known.contains(&text) {
            return Self {
                id: text.to_owned(),
                ..Default::default()
            };
        }
        let parts: Vec<&str> = text.split('-').collect();
        let mut end = parts.len();
        let mut device = None;
        let mut platform = None;
        let mut version = None;
        if end > 1 && DEVICES.contains(&parts[end - 1]) {
            device = Some(parts[end - 1].to_owned());
            end -= 1;
        }
        if end > 1 && PLATFORMS.contains(&parts[end - 1]) {
            platform = Some(parts[end - 1].to_owned());
            end -= 1;
        }
        if end > 1
            && !parts[end - 1].is_empty()
            && parts[end - 1]
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.')
            && parts[end - 1].chars().any(|c| c.is_ascii_digit())
        {
            version = Some(parts[end - 1].to_owned());
            end -= 1;
        }
        let id = parts[..end].join("-");
        // A known id that is a longer prefix than the split found keeps its own dashes.
        let mut best = id;
        for k in known {
            if (text == *k || text.starts_with(&format!("{k}-"))) && k.len() > best.len() {
                best = (*k).to_owned();
                // Re-read the suffixes after it.
                let rest = &text[k.len()..];
                let r = Self::parse_suffixes(rest.trim_start_matches('-'));
                return Self {
                    id: best,
                    version: r.0,
                    platform: r.1,
                    device: r.2,
                };
            }
        }
        Self {
            id: best,
            version,
            platform,
            device,
        }
    }
    fn parse_suffixes(rest: &str) -> (Option<String>, Option<String>, Option<String>) {
        let mut version = None;
        let mut platform = None;
        let mut device = None;
        for part in rest.split('-').filter(|p| !p.is_empty()) {
            if DEVICES.contains(&part) {
                device = Some(part.to_owned());
            } else if PLATFORMS.contains(&part) {
                platform = Some(part.to_owned());
            } else if part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                version = Some(part.to_owned());
            }
        }
        (version, platform, device)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub schema_version: u32,
    pub packages: Vec<Package>,
    /// The applications each platform's shell shows, in its own order: its dock, app
    /// grid or home screen pages. Only kinds the catalog has.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shelves: BTreeMap<String, Vec<String>>,
}
impl Catalog {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let c: Catalog = serde_json::from_str(text).map_err(|e| e.to_string())?;
        c.validate()?;
        Ok(c)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "catalog schema_version {} is not {SCHEMA_VERSION}",
                self.schema_version
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.packages {
            if p.id.is_empty()
                || !p
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
            {
                return Err(format!(
                    "package id {:?} is not lowercase letters, digits, dashes and underscores",
                    p.id
                ));
            }
            if !seen.insert(p.id.clone()) {
                return Err(format!("package {} is listed twice", p.id));
            }
            for platform in p.platforms.keys() {
                if !PLATFORMS.contains(&platform.as_str()) {
                    return Err(format!(
                        "package {} names unknown platform {platform}",
                        p.id
                    ));
                }
            }
        }
        for (platform, kinds) in &self.shelves {
            if !PLATFORMS.contains(&platform.as_str()) {
                return Err(format!("shelf for unknown platform {platform}"));
            }
            for k in kinds {
                if !seen.contains(k) {
                    return Err(format!("shelf {platform} names unknown package {k}"));
                }
            }
        }
        Ok(())
    }
    pub fn get(&self, id: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.id == id)
    }
    pub fn ids(&self) -> Vec<&str> {
        self.packages.iter().map(|p| p.id.as_str()).collect()
    }
    /// The package a world's reference names, whichever suffixes it carries.
    pub fn resolve(&self, reference: &str) -> Option<(&Package, Reference)> {
        let known = self.ids();
        let r = Reference::parse(reference, &known);
        self.get(&r.id).map(|p| (p, r))
    }
    /// The short id of a reference, or the reference itself when unknown.
    pub fn short<'a>(&self, reference: &'a str) -> std::borrow::Cow<'a, str> {
        match self.resolve(reference) {
            Some((p, _)) => std::borrow::Cow::Owned(p.id.clone()),
            None => std::borrow::Cow::Borrowed(reference),
        }
    }
    /// What `platform` calls package `id`, falling back to the casual name.
    pub fn label(&self, id: &str, platform: Option<&str>) -> Option<String> {
        let p = self.get(id)?;
        Some(
            platform
                .and_then(|pl| p.label_on(pl))
                .unwrap_or(&p.name)
                .to_owned(),
        )
    }
    /// The packages served from Wasm modules, as module name → (kind, tag) list, in
    /// catalog order.
    pub fn modules(&self) -> Vec<(String, Vec<(String, String)>)> {
        let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for p in &self.packages {
            if let Some((module, tag)) = p.module() {
                match out.iter_mut().find(|(m, _)| m == module) {
                    Some((_, apps)) => apps.push((p.id.clone(), tag.to_owned())),
                    None => out.push((module.to_owned(), vec![(p.id.clone(), tag.to_owned())])),
                }
            }
        }
        out
    }
    /// The kinds the engine launches itself (native and web applications), in catalog
    /// order.
    pub fn native_kinds(&self) -> Vec<&str> {
        self.packages
            .iter()
            .filter(|p| matches!(p.runtime, Runtime::Native { .. } | Runtime::Web { .. }))
            .map(|p| p.id.as_str())
            .collect()
    }
    pub fn shelf(&self, platform: &str) -> Vec<&Package> {
        self.shelves
            .get(platform)
            .into_iter()
            .flatten()
            .filter_map(|k| self.get(k))
            .collect()
    }
}

static GLOBAL: OnceLock<RwLock<Arc<Catalog>>> = OnceLock::new();
fn cell() -> &'static RwLock<Arc<Catalog>> {
    GLOBAL.get_or_init(|| {
        RwLock::new(Arc::new(
            Catalog::from_json(BUILTIN_JSON).expect("the built-in catalog is valid"),
        ))
    })
}
/// The catalog in force: the built-in one plus whatever was installed.
pub fn global() -> Arc<Catalog> {
    cell().read().unwrap_or_else(|e| e.into_inner()).clone()
}
/// Add or replace a package at run time (an account's own application, a host's
/// package store). A package of the built-in catalog is replaced by id.
pub fn install(package: Package) -> Result<(), String> {
    let mut guard = cell().write().unwrap_or_else(|e| e.into_inner());
    let mut next = (**guard).clone();
    next.packages.retain(|p| p.id != package.id);
    next.packages.push(package);
    next.validate()?;
    *guard = Arc::new(next);
    Ok(())
}
/// Forget installed packages: the built-in catalog alone.
pub fn reset() {
    let mut guard = cell().write().unwrap_or_else(|e| e.into_inner());
    *guard = Arc::new(Catalog::from_json(BUILTIN_JSON).expect("the built-in catalog is valid"));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_catalog_is_valid_and_complete() {
        let c = Catalog::from_json(BUILTIN_JSON).unwrap();
        for id in [
            "files",
            "browser",
            "terminal",
            "editor",
            "kicad",
            "freecad",
            "code",
            "spreadsheet",
            "excel",
            "database",
            "mail",
            "calendar",
            "notes",
            "music",
            "maps",
            "clipchamp",
            "imovie",
            "kdenlive",
            "videoeditor",
        ] {
            assert!(c.get(id).is_some(), "{id}");
        }
        let modules = c.modules();
        let names: Vec<&str> = modules.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(
            names,
            ["music", "maps", "code", "freecad", "office", "kicad", "video"]
        );
        for p in PLATFORMS {
            assert!(!c.shelf(p).is_empty(), "shelf {p}");
        }
    }
    #[test]
    fn references_parse_with_ignorable_suffixes() {
        let c = Catalog::from_json(BUILTIN_JSON).unwrap();
        let (p, r) = c.resolve("kicad-8.0-ubuntu-desktop").unwrap();
        assert_eq!(p.id, "kicad");
        assert_eq!(r.version.as_deref(), Some("8.0"));
        assert_eq!(r.platform.as_deref(), Some("ubuntu"));
        assert_eq!(r.device.as_deref(), Some("desktop"));
        assert_eq!(c.resolve("kicad").unwrap().0.id, "kicad");
        assert_eq!(
            c.resolve("kicad-windows").unwrap().1.platform.as_deref(),
            Some("windows")
        );
        assert_eq!(
            c.resolve("react-admin").map(|(p, _)| p.id.as_str()),
            Some("react-admin")
        );
        assert_eq!(c.resolve("react-admin-ubuntu").unwrap().0.id, "react-admin");
        assert!(c.resolve("nonesuch").is_none());
        assert_eq!(c.short("kicad-8.0-ubuntu"), "kicad");
        assert_eq!(
            c.get("kicad")
                .unwrap()
                .qualified(Some("ubuntu"), Some("desktop")),
            "kicad-8.0-ubuntu-desktop"
        );
        assert_eq!(c.get("kicad").unwrap().qualified(None, None), "kicad-8.0");
    }
    #[test]
    fn labels_follow_the_platform() {
        let c = Catalog::from_json(BUILTIN_JSON).unwrap();
        assert_eq!(c.label("mail", Some("ubuntu")).unwrap(), "Thunderbird Mail");
        assert_eq!(c.label("mail", Some("windows")).unwrap(), "Outlook");
        assert_eq!(c.label("mail", None).unwrap(), "Mail");
        assert_eq!(c.label("files", Some("macos")).unwrap(), "Finder");
        assert_eq!(c.label("kicad", Some("windows")).unwrap(), "KiCad 8.0");
    }
    #[test]
    fn install_and_reset() {
        let mut p = global().get("kicad").unwrap().clone();
        p.id = "myapp".into();
        p.runtime = Runtime::Web {
            script: Some("document.body.textContent = 'hi'".into()),
            style: None,
            react: false,
        };
        p.name = "My App".into();
        install(p).unwrap();
        assert!(global().get("myapp").is_some());
        reset();
        assert!(global().get("myapp").is_none());
    }
}
