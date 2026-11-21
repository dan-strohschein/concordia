//! Primitives → `MeshData` (`mesh/<mesh>/<primitive>`); missing streams are filled by `finish_mesh`.

use concordia_assets::{Artifact, AssetKind, ImportOutput, MeshData, encode};
use concordia_import_model::finish_mesh;

use crate::names::Names;

/// Imports every triangle primitive.
pub(crate) fn meshes(
    doc: &gltf::Document,
    buffers: &[Vec<u8>],
    names: &Names,
    out: &mut ImportOutput,
) -> Result<(), String> {
    for mesh in doc.meshes() {
        for (p, primitive) in mesh.primitives().enumerate() {
            let sub = format!("mesh/{}/{p}", names.meshes[mesh.index()]);
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                return Err(format!(
                    "{sub}: only triangle primitives are supported, found {:?}",
                    primitive.mode()
                ));
            }
            let data = primitive_data(&primitive, buffers).map_err(|e| format!("{sub}: {e}"))?;
            out.artifacts.insert(
                sub,
                Artifact {
                    kind: AssetKind::Mesh,
                    bytes: encode(&data).map_err(|e| e.to_string())?,
                },
            );
        }
    }
    Ok(())
}

fn primitive_data(
    primitive: &gltf::Primitive<'_>,
    buffers: &[Vec<u8>],
) -> Result<MeshData, String> {
    let reader = primitive.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or("no POSITION attribute")?
        .collect();
    let count = positions.len() as u32;
    let indices: Vec<u32> = reader
        .read_indices()
        .map_or_else(|| (0..count).collect(), |i| i.into_u32().collect());
    let mut mesh = MeshData {
        normals: reader
            .read_normals()
            .map(Iterator::collect)
            .unwrap_or_default(),
        tangents: reader
            .read_tangents()
            .map(Iterator::collect)
            .unwrap_or_default(),
        uv0: reader
            .read_tex_coords(0)
            .map(|t| t.into_f32().collect())
            .unwrap_or_default(),
        uv1: reader
            .read_tex_coords(1)
            .map(|t| t.into_f32().collect())
            .unwrap_or_default(),
        colors: reader
            .read_colors(0)
            .map(|c| c.into_rgba_f32().collect())
            .unwrap_or_default(),
        joints: reader
            .read_joints(0)
            .map(|j| j.into_u16().collect())
            .unwrap_or_default(),
        weights: reader
            .read_weights(0)
            .map(|w| w.into_f32().collect())
            .unwrap_or_default(),
        positions,
        indices,
        meshlets: Vec::new(),
        aabb: Default::default(),
    };
    finish_mesh(&mut mesh)?;
    Ok(mesh)
}
