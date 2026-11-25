//! Compiles the vendored meshoptimizer and Concordia's size shim with `cc` and links them statically.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let src = manifest.join("../../third_party/meshoptimizer/src");
    let shim = manifest.join("shim");
    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed={}", shim.display());
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&src)
        .expect("third_party/meshoptimizer/src exists")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "cpp"))
        .collect();
    // Sorted so the archive is the same on every machine.
    sources.sort();
    cc::Build::new()
        .cpp(true)
        .files(&sources)
        .file(shim.join("concordia_meshopt.cpp"))
        .include(&src)
        .std("c++11")
        .opt_level(2)
        // No fused multiply-add: x86 and ARM compute the same floats (4.2 Q1, Q5).
        .flag_if_supported("-ffp-contract=off")
        .flag_if_supported("-fno-exceptions")
        .flag_if_supported("-fno-rtti")
        .warnings(false)
        .compile("concordia_meshopt");
}
