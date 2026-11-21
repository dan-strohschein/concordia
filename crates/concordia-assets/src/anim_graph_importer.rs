//! `.animgraph.ron` → `AnimGraph`: animation graphs as canonical text, read and validated by
//! `read_animgraph` (M2 phase 2.4.3).

use crate::anim_graph::read_animgraph;
use crate::error::AssetError;
use crate::importer::{Artifact, AssetKind, ImportInput, ImportOutput, Importer, encode};

/// The animation graph importer.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnimGraphImporter;

impl Importer for AnimGraphImporter {
    fn name(&self) -> &'static str {
        "animgraph"
    }

    fn version(&self) -> u32 {
        1
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["animgraph.ron"]
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
        let graph = read_animgraph(text).map_err(|e| fail(e.to_string()))?;
        let mut output = ImportOutput::default();
        output.artifacts.insert(
            String::new(),
            Artifact {
                kind: AssetKind::AnimGraph,
                bytes: encode(&graph)?,
            },
        );
        Ok(output)
    }
}
