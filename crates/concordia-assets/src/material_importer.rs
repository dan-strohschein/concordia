//! `.material.ron` → `Material`: layered materials as canonical text (ADR-0068), read and validated
//! by `read_material`; M1's flat text migrates to one slab.

use crate::error::AssetError;
use crate::importer::{Artifact, AssetKind, ImportInput, ImportOutput, Importer, encode};
use crate::material::read_material;

/// The material importer.
#[derive(Debug, Clone, Copy, Default)]
pub struct MaterialImporter;

impl Importer for MaterialImporter {
    fn name(&self) -> &'static str {
        "material"
    }

    fn version(&self) -> u32 {
        2
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["material.ron"]
    }

    fn default_settings(&self) -> serde_json::Value {
        serde_json::Value::Object(Default::default())
    }

    fn import(&self, input: ImportInput<'_>) -> Result<ImportOutput, AssetError> {
        let fail = |reason: String| AssetError::Import {
            path: input.path.to_path_buf(),
            reason,
        };
        let text = std::str::from_utf8(input.bytes).map_err(|e| fail(e.to_string()))?;
        let material = read_material(text).map_err(|e| fail(e.to_string()))?;
        let mut output = ImportOutput::default();
        output.artifacts.insert(
            String::new(),
            Artifact {
                kind: AssetKind::Material,
                bytes: encode(&material)?,
            },
        );
        Ok(output)
    }
}
