//! The importer contract: source bytes + settings → derived-data artifacts.

use std::collections::BTreeMap;
use std::path::Path;

use concordia_core::id::{AssetId, TypedId};
use serde::{Deserialize, Serialize};

use crate::error::AssetError;

/// What an artifact contains (decides how it is decoded).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind {
    /// `TextureData`.
    Texture,
    /// `MeshData`.
    Mesh,
    /// `Material` (layered, ADR-0068).
    Material,
    /// Prefab RON text (`concordia-scene`).
    Prefab,
    /// Skin data (joints, inverse bind matrices).
    Skin,
    /// A punctual light.
    Light,
    /// A camera.
    Camera,
    /// Animation clip.
    Clip,
    /// `AnimGraph` (M2 phase 2.4.3).
    AnimGraph,
    /// `concordia_audio::Sound` (M2 phase 2.7a).
    Sound,
    /// `concordia_audio::Cue` (M2 phase 2.7a).
    Cue,
    /// `concordia_ui::Font` (M2 phase 2.7b).
    Font,
    /// `concordia_ui::Theme` (M2 phase 2.7b).
    Theme,
}

/// One derived artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    /// Content kind.
    pub kind: AssetKind,
    /// Postcard-encoded data (prefabs: UTF-8 RON).
    pub bytes: Vec<u8>,
}

/// An importer's result: artifacts by sub-path (`""` is the main asset).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ImportOutput {
    /// Artifacts.
    pub artifacts: BTreeMap<String, Artifact>,
}

/// Sub-asset ids: existing ones from the sidecar, new ones as UUIDv5(parent, sub-path).
#[derive(Debug, Clone, PartialEq)]
pub struct SubIds {
    parent: AssetId,
    ids: BTreeMap<String, AssetId>,
}

impl SubIds {
    /// Ids for `parent`, seeded from its sidecar.
    pub fn new(parent: AssetId, existing: BTreeMap<String, AssetId>) -> Self {
        Self {
            parent,
            ids: existing,
        }
    }

    /// The id of `sub_path`, creating a deterministic one if new.
    pub fn id(&mut self, sub_path: &str) -> AssetId {
        let parent = self.parent;
        *self.ids.entry(sub_path.to_owned()).or_insert_with(|| {
            let uuid = uuid::Uuid::new_v5(&parent.as_uuid(), sub_path.as_bytes());
            AssetId::try_from_uuid(uuid).expect("v5 UUIDs are valid asset ids")
        })
    }

    /// The parent asset (sub-asset references are `asset:<parent>#<path>`).
    pub fn parent(&self) -> AssetId {
        self.parent
    }

    /// All ids, for writing back to the sidecar.
    pub fn into_map(self) -> BTreeMap<String, AssetId> {
        self.ids
    }
}

/// What an importer sees.
pub struct ImportInput<'a> {
    /// Project-relative source path.
    pub path: &'a Path,
    /// Source bytes.
    pub bytes: &'a [u8],
    /// Settings from the sidecar.
    pub settings: &'a serde_json::Value,
    /// Reads a file relative to the source's directory (glTF buffers, images).
    pub read_relative: &'a dyn Fn(&str) -> Result<Vec<u8>, AssetError>,
    /// Sub-asset ids.
    pub sub_ids: &'a mut SubIds,
}

/// Turns source files into derived data. Implement it for new formats and register it with `AssetDb`.
pub trait Importer: Send + Sync {
    /// Unique name (written to sidecars).
    fn name(&self) -> &'static str;
    /// Bumped when output changes (part of the DDC key).
    fn version(&self) -> u32;
    /// Lowercase extensions handled, without the dot.
    fn extensions(&self) -> &'static [&'static str];
    /// Settings for a new sidecar.
    fn default_settings(&self) -> serde_json::Value;
    /// Imports one file; must be deterministic.
    fn import(&self, input: ImportInput<'_>) -> Result<ImportOutput, AssetError>;
}

/// Postcard-encodes derived data (what importers put in `Artifact::bytes`).
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, AssetError> {
    postcard::to_allocvec(value).map_err(|e| AssetError::Decode(e.to_string()))
}

/// Decodes postcard-encoded derived data.
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, AssetError> {
    postcard::from_bytes(bytes).map_err(|e| AssetError::Decode(e.to_string()))
}
