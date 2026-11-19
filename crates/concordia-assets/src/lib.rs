//! Concordia assets: source files plus `<file>.import.ron` sidecars (stable ids), importers that
//! produce derived data in the content-addressed DDC (`.concordia/ddc`), built-in assets, hot
//! reload, and the `asset.import` / `asset.list` Engine API methods (ADR-0044).

mod builtin;
mod data;
mod db;
mod error;
mod files;
mod image_importer;
mod importer;
mod key;
mod module;
mod prefab_method;
mod reference;
mod sidecar;

pub use builtin::{Builtin, BuiltinAsset};
pub use data::{
    AlphaMode, CameraData, Channel, ChannelPath, ClipData, Interpolation, LightData, MaterialData,
    MeshData, MeshRenderer, SkinData, TextureData, TextureFormat,
};
pub use db::{AssetDb, AssetInfo, ImportReport};
pub use error::AssetError;
pub use files::{FileKind, ProjectFile, ProjectFiles};
pub use image_importer::{ColorSpace, ImageImporter, ImageSettings, decode as decode_image};
pub use importer::{
    Artifact, AssetKind, ImportInput, ImportOutput, Importer, SubIds, decode, encode,
};
pub use key::{PLATFORM, import_key};
pub use module::{AssetImport, AssetList, AssetsModule};
pub use prefab_method::PrefabInstantiate;
pub use reference::AssetRef;
pub use sidecar::Sidecar;

/// Registers the asset components (`MeshRenderer`).
pub fn register_types(
    registry: &mut concordia_reflect::TypeRegistry,
) -> Result<(), concordia_reflect::RegistryError> {
    concordia_terra::register_component::<MeshRenderer>(registry)
}
