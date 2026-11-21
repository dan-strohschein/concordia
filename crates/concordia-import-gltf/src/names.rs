//! Stable sub-asset names: glTF names where present and unique, else indices.

use concordia_import_model::unique_names;

/// Names for meshes, materials, images, nodes, clips, lights and cameras.
pub(crate) struct Names {
    pub(crate) meshes: Vec<String>,
    pub(crate) materials: Vec<String>,
    pub(crate) images: Vec<String>,
    pub(crate) nodes: Vec<String>,
    pub(crate) clips: Vec<String>,
    pub(crate) lights: Vec<String>,
    pub(crate) cameras: Vec<String>,
}

impl Names {
    pub(crate) fn of(doc: &gltf::Document) -> Self {
        Self {
            meshes: unique_names(doc.meshes().map(|m| m.name())),
            materials: unique_names(doc.materials().map(|m| m.name())),
            images: unique_names(doc.images().map(|i| i.name())),
            nodes: unique_names(doc.nodes().map(|n| n.name())),
            clips: unique_names(doc.animations().map(|a| a.name())),
            lights: doc
                .lights()
                .map_or_else(Vec::new, |l| unique_names(l.map(|l| l.name()))),
            cameras: unique_names(doc.cameras().map(|c| c.name())),
        }
    }
}
