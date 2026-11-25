//! Concordia assets: source files plus `<file>.import.ron` sidecars (stable ids), importers that
//! produce derived data in the content-addressed DDC (`.concordia/ddc`), built-in assets, hot
//! reload, and the `asset.import` / `asset.list` Engine API methods (ADR-0044).

mod anim_graph;
mod anim_graph_importer;
mod builtin;
mod data;
mod db;
mod error;
mod files;
mod image_importer;
mod importer;
mod key;
mod material;
mod material_importer;
mod material_m1;
mod material_method;
mod module;
mod prefab_method;
mod reference;
pub mod shapes;
mod sidecar;
mod source;

pub use anim_graph::{
    AdvanceMode, AnimGraph, AnimGraphError, AnimNode, FadeCurve, Parameter, StateTransition,
    SwitchMode, TransitionInput, read_animgraph, write_animgraph,
};
pub use anim_graph_importer::AnimGraphImporter;
pub use builtin::{Builtin, BuiltinAsset};
pub use data::{
    AlphaMode, CameraData, Channel, ChannelPath, ClipData, ClipEvent, Interpolation, LightData,
    MeshData, MeshRenderer, SkinData, Skinned, TextureData, TextureFormat, validate_events,
};
pub use db::{AssetDb, AssetInfo, ImportReport, prefab_doc};
pub use error::AssetError;
pub use files::{FileKind, ProjectFile, ProjectFiles};
pub use image_importer::{ColorSpace, ImageImporter, ImageSettings, decode as decode_image};
pub use importer::{
    Artifact, AssetKind, DerivedCache, ImportInput, ImportOutput, Importer, NoDerivedCache, SubIds,
    decode, encode, import_jobs,
};
pub use key::{PLATFORM, import_key};
pub use material::{
    Fuzz, Haze, Material, MaterialError, MaterialFormat, MaterialNode, SURFACE, Slab, Subsurface,
    ThinFilm, read_material, read_material_with_format, write_material,
};
pub use material_importer::MaterialImporter;
pub use material_method::{
    MaterialGet, MaterialGetParams, MaterialGetResult, MaterialSet, MaterialSetParams,
    MaterialSetResult,
};
pub use module::{AssetImport, AssetImportParams, AssetImportResult, AssetList, AssetsModule};
pub use prefab_method::PrefabInstantiate;
pub use reference::AssetRef;
pub use sidecar::{Provenance, Sidecar};
pub use source::AssetSource;

/// Registers the asset components (`MeshRenderer`, `Skinned`) and the `Material` and `AnimGraph`
/// types (for `schema.get`).
pub fn register_types(
    registry: &mut concordia_reflect::TypeRegistry,
) -> Result<(), concordia_reflect::RegistryError> {
    concordia_terra::register_component::<MeshRenderer>(registry)?;
    concordia_terra::register_component::<Skinned>(registry)?;
    registry.register::<Material>()?;
    registry.register::<AnimGraph>().map(|_| ())
}
