//! The hand-written bindings match the shim header (M8 8.1a).

#[test]
fn header_hash_matches_bindings() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let header = std::fs::read(root.join("shim/concordia_manifold.h")).unwrap();
    let expected = format!("// header blake3: {}", blake3::hash(&header).to_hex());
    let bindings = std::fs::read_to_string(root.join("src/bindings.rs")).unwrap();
    assert!(
        bindings.lines().any(|l| l == expected),
        "update src/bindings.rs to the header, then its first line to: {expected}"
    );
}

#[test]
fn version_matches_vendored() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let vendor =
        std::fs::read_to_string(root.join("../../third_party/manifold/VENDOR.md")).unwrap();
    assert!(
        vendor.contains(&format!("tag v{}", manifold_sys::MANIFOLD_VERSION)),
        "{vendor}"
    );
}
