//! Compiles the vendored Manifold core and Concordia's shim with `cc` and links them statically
//! (ADR-0180): sequential (no TBB), no cross sections (no Clipper2), no fused multiply-add.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let root = manifest.join("../../third_party/manifold");
    let shim = manifest.join("shim");
    println!("cargo:rerun-if-changed={}", root.display());
    println!("cargo:rerun-if-changed={}", shim.display());
    let mut sources: Vec<PathBuf> = std::fs::read_dir(root.join("src"))
        .expect("third_party/manifold/src exists")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "cpp"))
        .collect();
    // Sorted so the archive is the same on every machine.
    sources.sort();
    cc::Build::new()
        .cpp(true)
        .files(&sources)
        .file(shim.join("concordia_manifold.cpp"))
        .include(root.join("include"))
        .include(root.join("src"))
        .include(&shim)
        .std("c++17")
        .opt_level(2)
        .define("MANIFOLD_PAR", "-1")
        // No fused multiply-add: x86 and ARM compute the same floats.
        .flag_if_supported("-ffp-contract=off")
        .warnings(false)
        .compile("concordia_manifold");
}
