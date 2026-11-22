//! Compiles the vendored Recast and Detour and Concordia's shim with `cc` and links them statically.

use std::path::PathBuf;

const RECAST: [&str; 11] = [
    "Recast",
    "RecastAlloc",
    "RecastArea",
    "RecastAssert",
    "RecastContour",
    "RecastFilter",
    "RecastLayers",
    "RecastMesh",
    "RecastMeshDetail",
    "RecastRasterization",
    "RecastRegion",
];
const DETOUR: [&str; 7] = [
    "DetourAlloc",
    "DetourAssert",
    "DetourCommon",
    "DetourNavMesh",
    "DetourNavMeshBuilder",
    "DetourNavMeshQuery",
    "DetourNode",
];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let vendored = manifest.join("../../third_party/recastnavigation");
    let shim = manifest.join("shim");
    println!("cargo:rerun-if-changed={}", vendored.display());
    println!("cargo:rerun-if-changed={}", shim.display());
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .define("DT_POLYREF64", "1")
        .define("NDEBUG", None)
        .include(vendored.join("Recast/Include"))
        .include(vendored.join("Detour/Include"))
        .include(&shim)
        .opt_level(2)
        .flag_if_supported("-ffp-contract=off")
        .warnings(false);
    for name in RECAST {
        build.file(vendored.join(format!("Recast/Source/{name}.cpp")));
    }
    for name in DETOUR {
        build.file(vendored.join(format!("Detour/Source/{name}.cpp")));
    }
    build.file(shim.join("concordia_recast.cpp"));
    build.compile("concordia_recast");
}
