//! Format-neutral model import shared by the glTF and FBX importers.
//!
//! An importer reads its format into a [`ModelTree`] and mesh streams; this crate names the
//! sub-assets, writes the prefab, finishes meshes (normals, UVs, tangents), picks texture color
//! spaces and reduces skin weights, so every model format produces the same artifacts.

mod mesh;
mod names;
mod prefab;
mod settings;
mod skin;
mod texture;

pub use mesh::{finish_mesh, generate_tangents};
pub use names::unique_names;
pub use prefab::{
    EntitySummary, MeshPart, ModelNode, ModelTree, PrefabSummary, load_prefab, load_prefab_doc,
    write_prefab,
};
pub use settings::ModelSettings;
pub use skin::{MAX_INFLUENCES, reduce_influences};
pub use texture::TextureUsage;

use concordia_assets::{Artifact, AssetError, AssetKind, ImportOutput, encode};

/// Postcard-encodes `data` as the artifact at `sub`.
pub fn insert_artifact<T: serde::Serialize>(
    out: &mut ImportOutput,
    sub: String,
    kind: AssetKind,
    data: &T,
) -> Result<(), AssetError> {
    let bytes = encode(data)?;
    out.artifacts.insert(sub, Artifact { kind, bytes });
    Ok(())
}
