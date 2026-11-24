//! `<file>.import.ron`: the asset's stable id, importer and settings, and sub-asset ids.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use concordia_core::id::AssetId;
use concordia_reflect::Reflect;
use concordia_scene::{read_document, write_document};
use serde::{Deserialize, Serialize};

use crate::error::{AssetError, io};

/// The sidecar next to a source file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Sidecar {
    /// Stable asset id (UUIDv7); survives moves and re-imports.
    pub id: AssetId,
    /// Importer name.
    pub importer: String,
    /// Importer version that wrote these settings.
    pub importer_version: u32,
    /// Importer settings (each importer validates its own).
    pub settings: serde_json::Value,
    /// Sub-asset ids by path (`mesh/Body`, `material/Steel`, ...), kept stable across re-imports.
    pub sub_assets: BTreeMap<String, AssetId>,
    /// How a generator made the source file, so it can be made again (none: made by hand).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

/// What generated a source file (`concordia-gen`): enough to regenerate the same bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// The provider, e.g. `mesh`.
    pub provider: String,
    /// The provider's version (a change may change its output).
    pub provider_version: u32,
    /// The request, as given.
    pub request: serde_json::Value,
    /// The seed.
    pub seed: u64,
    /// BLAKE3 of the provider, version, request and seed (and any input files).
    pub inputs_hash: String,
}

impl Sidecar {
    /// What a sidecar's file name adds to its source's.
    pub const SUFFIX: &'static str = ".import.ron";

    /// The sidecar path for `source`.
    pub fn path_for(source: &Path) -> PathBuf {
        let mut name = source.as_os_str().to_owned();
        name.push(Self::SUFFIX);
        PathBuf::from(name)
    }

    /// Reads a sidecar.
    pub fn read(path: &Path) -> Result<Self, AssetError> {
        let text = std::fs::read_to_string(path).map_err(|e| io(path, e))?;
        read_document(&text).map_err(|e| AssetError::Sidecar {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })
    }

    /// Writes a sidecar in canonical form (only if it changed).
    pub fn write(&self, path: &Path) -> Result<(), AssetError> {
        let text = write_document(self).map_err(|e| AssetError::Sidecar {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;
        if std::fs::read_to_string(path).ok().as_deref() == Some(text.as_str()) {
            return Ok(());
        }
        std::fs::write(path, text).map_err(|e| io(path, e))
    }
}
