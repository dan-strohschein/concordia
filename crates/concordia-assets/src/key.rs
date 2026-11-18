//! The DDC key of an import: everything that can change its output, and nothing else.

use concordia_core::ddc::DdcKey;

/// The target platform tag in import keys (derived data may differ per GPU backend).
pub const PLATFORM: &str = if cfg!(target_os = "macos") {
    "macos-metal"
} else {
    "linux-vulkan"
};

/// blake3 over importer name and version, platform, canonical settings and source bytes.
pub fn import_key(
    importer: &str,
    version: u32,
    platform: &str,
    settings: &serde_json::Value,
    source: &[u8],
) -> DdcKey {
    // serde_json maps are sorted (no `preserve_order`), so this text is canonical.
    let settings = settings.to_string();
    DdcKey::of(&[
        b"concordia-import-v1",
        importer.as_bytes(),
        &version.to_le_bytes(),
        platform.as_bytes(),
        settings.as_bytes(),
        source,
    ])
}

/// The key of one artifact of an import.
pub(crate) fn artifact_key(import: &DdcKey, sub_path: &str) -> DdcKey {
    DdcKey::of(&[import.to_string().as_bytes(), sub_path.as_bytes()])
}
