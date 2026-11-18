//! Concordia assets: source files plus `<file>.import.ron` sidecars (stable ids), importers that
//! produce derived data in the content-addressed DDC (`.concordia/ddc`), built-in assets, hot
//! reload, and the `asset.import` / `asset.list` Engine API methods (ADR-0044).

mod builtin;
mod data;
mod db;
mod error;
mod image_importer;
mod importer;
mod key;
mod module;
mod reference;
mod sidecar;

pub use builtin::{Builtin, BuiltinAsset};
pub use data::{AlphaMode, MaterialData, MeshData, TextureData, TextureFormat};
pub use db::{AssetDb, AssetInfo, ImportReport};
pub use error::AssetError;
pub use image_importer::{ColorSpace, ImageImporter, ImageSettings};
pub use importer::{Artifact, AssetKind, ImportInput, ImportOutput, Importer, SubIds};
pub use key::{PLATFORM, import_key};
pub use module::{AssetImport, AssetList, AssetsModule};
pub use reference::AssetRef;
pub use sidecar::Sidecar;
