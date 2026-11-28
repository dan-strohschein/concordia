//! The pieces in the DDC: keyed by the recipe, the source mesh's bytes and the cutter's version,
//! rebuilt from the recipe when missing.

use concordia_assets::{AssetDb, AssetRef, MeshData, encode};
use concordia_core::ddc::{Ddc, DdcKey};
use manifold_sys::safe::Mesh;

use crate::{Breakable, FRACTURE_VERSION, Fracture, FractureError, weld};

/// Pieces and where they came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Stored {
    /// The pieces.
    pub fracture: Fracture,
    /// Their DDC key (hex).
    pub key: String,
    /// They were read from the DDC rather than cut.
    pub cached: bool,
}

/// The DDC key of `recipe`'s pieces cut from `source`.
pub fn cache_key(recipe: &Breakable, source: &MeshData) -> Result<DdcKey, FractureError> {
    let text = concordia_scene::write_document(recipe)
        .map_err(|e| FractureError::InvalidParameter(e.to_string()))?;
    let mesh = encode(source).map_err(|e| FractureError::InvalidMesh(e.to_string()))?;
    Ok(DdcKey::of(&[
        b"concordia-fracture",
        &FRACTURE_VERSION.to_le_bytes(),
        text.as_bytes(),
        blake3::hash(&mesh).as_bytes(),
    ]))
}

/// `recipe`'s pieces: from `ddc` if there, otherwise cut from its source and cached.
pub fn pieces(recipe: &Breakable, db: &mut AssetDb, ddc: &Ddc) -> Result<Stored, FractureError> {
    let source = load_source(recipe, db)?;
    let key = cache_key(recipe, &source)?;
    let io = |e: concordia_core::ddc::DdcError| FractureError::Io(e.to_string());
    if let Some(bytes) = ddc.get(&key).map_err(io)?
        && let Ok(fracture) = concordia_assets::decode::<Fracture>(&bytes)
    {
        return Ok(Stored {
            fracture,
            key: key.to_string(),
            cached: true,
        });
    }
    let fracture = crate::fracture(&mesh_of(&source), &recipe.pattern, recipe.seed)?;
    let bytes = encode(&fracture).map_err(|e| FractureError::Io(e.to_string()))?;
    ddc.put(&key, &bytes).map_err(io)?;
    Ok(Stored {
        fracture,
        key: key.to_string(),
        cached: false,
    })
}

fn load_source(recipe: &Breakable, db: &mut AssetDb) -> Result<MeshData, FractureError> {
    let reference: AssetRef = recipe
        .source
        .parse()
        .map_err(|e: concordia_assets::AssetError| FractureError::InvalidMesh(e.to_string()))?;
    db.load_mesh(&reference)
        .map_err(|e| FractureError::InvalidMesh(format!("{}: {e}", recipe.source)))
}

/// An imported mesh as the cutter's input, welded.
fn mesh_of(data: &MeshData) -> Mesh {
    weld(&Mesh {
        positions: data.positions.clone(),
        triangles: data.indices.as_chunks::<3>().0.to_vec(),
    })
}
