//! Stable sub-asset names: FBX names where present and unique, else indices.

use std::path::Path;

use concordia_import_model::unique_names;

use crate::ffi::{MaterialValues, Mesh, Node, Stack, TextureFile};

/// Names for nodes, meshes, materials, texture files and clips.
pub(crate) struct Names {
    pub(crate) nodes: Vec<String>,
    pub(crate) meshes: Vec<String>,
    pub(crate) materials: Vec<String>,
    pub(crate) textures: Vec<String>,
    pub(crate) clips: Vec<String>,
}

impl Names {
    pub(crate) fn of(
        nodes: &[Node],
        meshes: &[Mesh],
        materials: &[MaterialValues],
        textures: &[TextureFile],
        stacks: &[Stack],
    ) -> Self {
        let stems: Vec<Option<String>> = textures
            .iter()
            .map(|t| file_stem(&t.relative_filename))
            .collect();
        Self {
            nodes: unique_names(nodes.iter().map(|n| Some(n.name.as_str()))),
            meshes: unique_names(meshes.iter().map(|m| Some(m.name.as_str()))),
            materials: unique_names(materials.iter().map(|m| Some(m.name.as_str()))),
            textures: unique_names(stems.iter().map(Option::as_deref)),
            clips: unique_names(stacks.iter().map(|s| Some(s.name.as_str()))),
        }
    }
}

/// A texture is named after its image file (FBX texture names are usually slot names).
fn file_stem(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    Path::new(&path).file_stem()?.to_str().map(str::to_owned)
}
