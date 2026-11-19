//! What the import cache key covers (M1 phase 1.4).

use concordia_assets::import_key;
use serde_json::json;

fn key(source: &[u8], settings: serde_json::Value, version: u32, platform: &str) -> String {
    import_key("image", version, platform, &settings, source).to_string()
}

#[test]
fn ddc_key_changes_with_source_settings_importer_version_and_platform() {
    let base = key(b"pixels", json!({"color_space": "Srgb"}), 1, "macos-metal");
    assert_eq!(
        base,
        key(b"pixels", json!({"color_space": "Srgb"}), 1, "macos-metal"),
        "deterministic"
    );
    assert_ne!(
        base,
        key(b"pixelz", json!({"color_space": "Srgb"}), 1, "macos-metal"),
        "source"
    );
    assert_ne!(
        base,
        key(
            b"pixels",
            json!({"color_space": "Linear"}),
            1,
            "macos-metal"
        ),
        "settings"
    );
    assert_ne!(
        base,
        key(b"pixels", json!({"color_space": "Srgb"}), 2, "macos-metal"),
        "importer version"
    );
    assert_ne!(
        base,
        key(b"pixels", json!({"color_space": "Srgb"}), 1, "linux-vulkan"),
        "platform"
    );
}

#[test]
fn settings_key_order_does_not_matter() {
    let a = key(b"x", json!({"a": 1, "b": 2}), 1, "p");
    let b = key(b"x", json!({"b": 2, "a": 1}), 1, "p");
    assert_eq!(a, b);
}
