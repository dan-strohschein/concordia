//! Stable sub-asset names: glTF names where present and unique, else indices.

use std::collections::BTreeMap;

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
            meshes: unique(doc.meshes().map(|m| m.name())),
            materials: unique(doc.materials().map(|m| m.name())),
            images: unique(doc.images().map(|i| i.name())),
            nodes: unique(doc.nodes().map(|n| n.name())),
            clips: unique(doc.animations().map(|a| a.name())),
            lights: doc
                .lights()
                .map_or_else(Vec::new, |l| unique(l.map(|l| l.name()))),
            cameras: unique(doc.cameras().map(|c| c.name())),
        }
    }
}

/// Each name, or its index when missing; duplicates get `#<index>`. `/` is replaced (it separates sub-paths).
fn unique<'a>(names: impl Iterator<Item = Option<&'a str>>) -> Vec<String> {
    let names: Vec<Option<String>> = names
        .map(|n| n.filter(|n| !n.is_empty()).map(|n| n.replace('/', "_")))
        .collect();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for name in names.iter().flatten() {
        *counts.entry(name.as_str()).or_default() += 1;
    }
    names
        .iter()
        .enumerate()
        .map(|(i, n)| match n {
            Some(n) if counts.get(n.as_str()) == Some(&1) => n.clone(),
            Some(n) => format!("{n}#{i}"),
            None => i.to_string(),
        })
        .collect()
}
