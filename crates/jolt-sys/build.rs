//! Builds the vendored Jolt and Concordia's shim with CMake and links them statically.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let third_party = manifest.join("../../third_party");
    for watched in ["cmake", "shim", "../../third_party/JoltPhysics/Build"] {
        println!(
            "cargo:rerun-if-changed={}",
            manifest.join(watched).display()
        );
    }
    let mut config = cmake::Config::new(manifest.join("cmake"));
    config
        .define("JOLT_PHYSICS_ROOT", third_party.join("JoltPhysics"))
        .profile("Release");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("aarch64") => "arm64",
            _ => "x86_64",
        };
        config
            .define("CMAKE_OSX_ARCHITECTURES", arch)
            .define("CMAKE_OSX_DEPLOYMENT_TARGET", "11.0");
    }
    let out = config.build();
    println!(
        "cargo:rustc-link-search=native={}",
        out.join("lib").display()
    );
    for lib in ["concordia_jolt_ext", "Jolt"] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    let cxx = match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => "c++",
        _ => "stdc++",
    };
    println!("cargo:rustc-link-lib={cxx}");
}
