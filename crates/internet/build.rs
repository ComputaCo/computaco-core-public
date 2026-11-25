//! The SHA-256 of the internet's world, which a build that leaves it out (`on-demand`)
//! asks the module host for by name and hash.
use sha2::{Digest, Sha256};

fn main() {
    let path = "world.json";
    println!("cargo:rerun-if-changed={path}");
    let bytes = std::fs::read(path).expect("worlds/internet/world.json");
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("cargo:rustc-env=CW_INTERNET_SHA256={hash}");
}
