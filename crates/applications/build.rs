//! Compile-time tables from the application catalog (`packages/catalog.json`,
//! crates/core/packages): the native kinds, the modules and the applications each
//! serves, every platform's labels and its shelf in display order. One file is the source
//! for the engine's dispatch, the shells' launchers and the hosted API's host selection,
//! so none of them can disagree about what an application is or is called.
use std::fmt::Write;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/catalog.json");
    println!("cargo:rerun-if-changed={}", root.display());
    let text = std::fs::read_to_string(&root).unwrap_or_else(|e| panic!("{}: {e}", root.display()));
    let catalog: serde_json::Value = serde_json::from_str(&text).expect("catalog.json is JSON");
    let packages = catalog["packages"].as_array().expect("packages");
    let mut out = String::new();
    let s = |v: &str| format!("{v:?}");
    // Native kinds in catalog order.
    let native: Vec<&str> = packages
        .iter()
        .filter(|p| p["runtime"]["kind"] == "native" || p["runtime"]["kind"] == "web")
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    writeln!(
        out,
        "/// Every application kind the engine launches itself (native and web), in catalog order."
    )
    .unwrap();
    writeln!(
        out,
        "pub const NATIVE_KINDS: &[&str] = &[{}];",
        native.iter().map(|k| s(k)).collect::<Vec<_>>().join(", ")
    )
    .unwrap();
    // Modules: name -> [(kind, tag)].
    let mut modules: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for p in packages {
        let r = &p["runtime"];
        if r["kind"] != "native" {
            continue;
        }
        let Some(module) = r["module"].as_str() else {
            continue;
        };
        let kind = p["id"].as_str().unwrap().to_owned();
        let tag = r["tag"].as_str().unwrap_or(&kind).to_owned();
        match modules.iter_mut().find(|(m, _)| m == module) {
            Some((_, apps)) => apps.push((kind, tag)),
            None => modules.push((module.to_owned(), vec![(kind, tag)])),
        }
    }
    writeln!(
        out,
        "/// The application modules and what each serves, as (module, [(kind, tag)])."
    )
    .unwrap();
    write!(out, "pub const MODULES: &[(&str, &[(&str, &str)])] = &[").unwrap();
    for (m, apps) in &modules {
        write!(
            out,
            "({}, &[{}]), ",
            s(m),
            apps.iter()
                .map(|(k, t)| format!("({}, {})", s(k), s(t)))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
    // Labels and shelves per platform.
    for platform in ["macos", "windows", "ubuntu", "ios", "android"] {
        let upper = platform.to_uppercase();
        let shelf: Vec<&str> = catalog["shelves"][platform]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap()).collect())
            .unwrap_or_default();
        let facts = |kind: &str| -> Option<(String, String)> {
            let p = packages.iter().find(|p| p["id"] == kind)?;
            let f = p["platforms"].get(platform)?;
            Some((
                f["label"].as_str()?.to_owned(),
                f["icon"].as_str().unwrap_or(kind).to_owned(),
            ))
        };
        writeln!(out, "/// {platform}'s shelf: the applications its shell shows, in its order, as (kind, icon, label).").unwrap();
        write!(out, "pub const SHELF_{upper}: &[(&str, &str, &str)] = &[").unwrap();
        for kind in &shelf {
            let (label, icon) = facts(kind).unwrap_or_else(|| {
                panic!("shelf {platform} lists {kind}, which has no {platform} facts")
            });
            write!(out, "({}, {}, {}), ", s(kind), s(&icon), s(&label)).unwrap();
        }
        writeln!(out, "];").unwrap();
        writeln!(
            out,
            "/// {platform}'s shelf as (kind, label), for shells that draw each kind's own icon."
        )
        .unwrap();
        write!(out, "pub const SHELF2_{upper}: &[(&str, &str)] = &[").unwrap();
        for kind in &shelf {
            let (label, _) = facts(kind).unwrap();
            write!(out, "({}, {}), ", s(kind), s(&label)).unwrap();
        }
        writeln!(out, "];").unwrap();
        writeln!(
            out,
            "/// What {platform} calls every application it offers, as (kind, label)."
        )
        .unwrap();
        write!(out, "pub const LABELS_{upper}: &[(&str, &str)] = &[").unwrap();
        for p in packages {
            if let Some(f) = p["platforms"].get(platform) {
                write!(
                    out,
                    "({}, {}), ",
                    s(p["id"].as_str().unwrap()),
                    s(f["label"].as_str().unwrap())
                )
                .unwrap();
            }
        }
        writeln!(out, "];").unwrap();
    }
    // Casual names for every package.
    write!(out, "/// Every package's casual, platform-free name, as (kind, name).\npub const NAMES: &[(&str, &str)] = &[").unwrap();
    for p in packages {
        write!(
            out,
            "({}, {}), ",
            s(p["id"].as_str().unwrap()),
            s(p["name"].as_str().unwrap())
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("catalog_gen.rs");
    std::fs::write(&dest, out).unwrap();
}
