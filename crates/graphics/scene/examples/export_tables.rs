//! Writes the tables web text is measured with, one file per family and kind, into a
//! folder: what a lazy engine (`lazy-metrics`) fetches on first use and pins (build.rs).
//!
//!   cargo run -p cw-scene --example export_tables -- <dir>
//!
//! Each file is the core's answer to a module asking for the same table (`tables::export`).
use cw_scene::metrics::Typeface;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("a folder to write into"));
    std::fs::create_dir_all(&dir).unwrap();
    for t in Typeface::ALL {
        for (key, kind) in [(b'W', "faces"), (b'K', "kerning")] {
            let bytes = cw_scene::tables::export(&[key, t as u8]).expect("a known request");
            if bytes.first() == Some(&1) {
                std::fs::write(dir.join(cw_scene::tables::web_file(t, kind)), bytes).unwrap();
            }
        }
    }
    std::fs::write(
        dir.join("dejavu-kerning.bin"),
        cw_scene::tables::export(b"k").unwrap(),
    )
    .unwrap();
}
