//! The pieces baked for drawing (M8 8.5b): one mesh per slot holding every piece, each vertex
//! rigidly on its piece's joint, so all fragments of a breakable share the meshes.

use concordia_assets::{AssetRef, MeshData};
use concordia_core::id::{AssetId, TypedId};
use concordia_core::math::Vec3;

use crate::{Fracture, FractureError, Piece, Slot};

/// The namespace of baked mesh ids (UUIDv5 of the pieces' DDC key).
const BAKED: uuid::Uuid = uuid::uuid!("6f2b8e0c-5a1d-4c3e-9b7a-2d4f6e8a0c1b");

/// `slot`'s triangles of every piece as one mesh (flat normals; interior faces get
/// [`Piece::interior_uvs`], surface faces zeros); `None` when there are none.
pub fn piece_mesh(
    fracture: &Fracture,
    slot: Slot,
    metres_per_tile: f32,
) -> Result<Option<MeshData>, FractureError> {
    if fracture.pieces.len() > usize::from(u16::MAX) + 1 {
        return Err(FractureError::InvalidParameter(format!(
            "{} pieces; a baked mesh holds at most 65536",
            fracture.pieces.len()
        )));
    }
    let mut mesh = MeshData::default();
    for (index, piece) in fracture.pieces.iter().enumerate() {
        add_piece(&mut mesh, piece, index as u16, slot, metres_per_tile)?;
    }
    if mesh.indices.is_empty() {
        return Ok(None);
    }
    mesh.aabb = MeshData::compute_aabb(&mesh.positions);
    Ok(Some(mesh))
}

/// The reference the baked mesh of `slot` is held under, for the pieces with DDC key `key`.
pub fn piece_mesh_ref(key: &str, slot: Slot) -> AssetRef {
    let uuid = uuid::Uuid::new_v5(&BAKED, key.as_bytes());
    let id = AssetId::try_from_uuid(uuid)
        .unwrap_or_else(|_| unreachable!("a v5 UUID is a valid asset id"));
    let sub = match slot {
        Slot::Surface => "mesh/surface",
        Slot::Interior => "mesh/interior",
    };
    AssetRef::Asset {
        id,
        sub: Some(sub.to_owned()),
    }
}

/// Appends `piece`'s triangles of `slot`, three vertices each, on joint `joint`.
fn add_piece(
    mesh: &mut MeshData,
    piece: &Piece,
    joint: u16,
    slot: Slot,
    metres_per_tile: f32,
) -> Result<(), FractureError> {
    let uvs = piece.interior_uvs(metres_per_tile)?;
    for ((t, s), uv) in piece.triangles.iter().zip(&piece.materials).zip(uvs) {
        if *s != slot {
            continue;
        }
        let corners = t.map(|i| Vec3::from_array(piece.positions[i as usize]));
        let (normal, tangent) = frame(corners);
        for (p, uv) in corners.iter().zip(uv) {
            mesh.indices.push(mesh.positions.len() as u32);
            mesh.positions.push(p.to_array());
            mesh.normals.push(normal.to_array());
            mesh.tangents.push(tangent);
            mesh.uv0.push(uv);
            mesh.joints.push([joint, 0, 0, 0]);
            mesh.weights.push([1.0, 0.0, 0.0, 0.0]);
        }
    }
    Ok(())
}

/// A face's unit normal and tangent (the interior UVs' u axis, with the bitangent's sign).
fn frame([a, b, c]: [Vec3; 3]) -> (Vec3, [f32; 4]) {
    let normal = (b - a).cross(c - a).normalize_or(Vec3::Z);
    let (u, v) = crate::piece::basis(normal);
    let sign = if normal.cross(u).dot(v) < 0.0 {
        -1.0
    } else {
        1.0
    };
    (normal, [u.x, u.y, u.z, sign])
}
