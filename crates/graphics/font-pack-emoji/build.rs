//! Says where this crate's faces are, to the build scripts of the crates that depend on it.
fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts");
    println!("cargo:root={}", dir.display());
    println!("cargo:rerun-if-changed=build.rs");
}
