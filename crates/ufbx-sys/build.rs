//! Compiles the vendored ufbx and Concordia's shim with `cc` and links them statically.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let ufbx = manifest.join("../../third_party/ufbx");
    let shim = manifest.join("shim");
    for watched in [ufbx.join("ufbx.c"), ufbx.join("ufbx.h"), shim.clone()] {
        println!("cargo:rerun-if-changed={}", watched.display());
    }
    cc::Build::new()
        .file(ufbx.join("ufbx.c"))
        .file(shim.join("concordia_ufbx.c"))
        .include(&ufbx)
        .include(&shim)
        .std("c11")
        .opt_level(2)
        .warnings(false)
        .compile("concordia_ufbx");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        println!("cargo:rustc-link-lib=m");
    }
}
