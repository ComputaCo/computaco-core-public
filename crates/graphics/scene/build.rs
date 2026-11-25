//! A lazy engine (`lazy-metrics`) measures web text with tables it fetches on first use:
//! the thirteen web families' advances and kern pairs and DejaVu's kern pairs, which only
//! web content uses. `examples/export_tables.rs` writes them, from the tables a full build
//! embeds, into the folder `CW_METRICS_DIR` names; this pins each file by SHA-256.
use sha2::{Digest, Sha256};
use std::fmt::Write;

fn main() {
    // The faces' folder, from cw-fonts (its `links` key): text measurement embeds faces from it.
    let fonts = std::env::var("DEP_CW_FONTS_ROOT").expect("cw-fonts says where its faces are");
    println!("cargo:rustc-env=CW_FONTS_DIR={fonts}");
    println!("cargo:rerun-if-env-changed=CW_METRICS_DIR");
    let mut out = String::new();
    if std::env::var_os("CARGO_FEATURE_LAZY_METRICS").is_some() {
        let dir = std::env::var("CW_METRICS_DIR").expect(
            "a lazy-metrics build pins the tables `cargo run -p cw-scene --example export_tables -- <dir>` writes: set CW_METRICS_DIR",
        );
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{dir}: {e}"))
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".bin"))
            .collect();
        files.sort();
        writeln!(
            out,
            "pub const METRIC_FILES: [MetricFile; {}] = [",
            files.len()
        )
        .unwrap();
        for file in files {
            let path = std::path::Path::new(&dir).join(&file);
            println!("cargo:rerun-if-changed={}", path.display());
            let bytes = std::fs::read(&path).unwrap();
            let sha256 = format!("{:x}", Sha256::digest(&bytes));
            writeln!(
                out,
                "    MetricFile {{ file: \"metrics/{file}\", sha256: {sha256:?}, bytes: {} }},",
                bytes.len()
            )
            .unwrap();
        }
        out.push_str("];\n");
    }
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("metric_files.rs");
    std::fs::write(dest, out).unwrap();
}
