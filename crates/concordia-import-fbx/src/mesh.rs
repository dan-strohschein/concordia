//! Material parts → `MeshData` (`mesh/<mesh>/<part>`): corners welded, V flipped, weights reduced.

use std::collections::{BTreeMap, HashMap};

use concordia_assets::{AssetKind, ImportOutput, MeshData};
use concordia_import_model::{finish_mesh, insert_artifact, reduce_influences};

use crate::ffi::{Corner, Mesh, Scene};
use crate::names::Names;

/// The sub-path of one material part.
pub(crate) fn part_sub(names: &Names, mesh: usize, part: usize) -> String {
    format!("mesh/{}/{part}", names.meshes[mesh])
}

/// Imports every non-empty part; `clusters` holds each used skin's cluster count.
pub(crate) fn meshes(
    scene: &Scene<'_>,
    meshes: &[Mesh],
    clusters: &BTreeMap<usize, usize>,
    names: &Names,
    out: &mut ImportOutput,
) -> Result<(), String> {
    for (m, mesh) in meshes.iter().enumerate() {
        for (p, triangles) in mesh.triangles.iter().enumerate().filter(|(_, t)| **t > 0) {
            let sub = part_sub(names, m, p);
            let data = part(scene, mesh, (m, p, *triangles), clusters)
                .map_err(|e| format!("{sub}: {e}"))?;
            insert_artifact(out, sub, AssetKind::Mesh, &data).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn part(
    scene: &Scene<'_>,
    mesh: &Mesh,
    (m, p, triangles): (usize, usize, usize),
    clusters: &BTreeMap<usize, usize>,
) -> Result<MeshData, String> {
    let corners = scene.corners(m, p, triangles);
    if corners.len() != triangles * 3 {
        return Err(format!(
            "ufbx gave {} of {} corners",
            corners.len(),
            triangles * 3
        ));
    }
    let (mut data, logical) = weld(&corners)?;
    if !mesh.has_normals {
        data.normals.clear();
    }
    if !mesh.has_uvs {
        data.uv0.clear();
    }
    if let Some(skin) = mesh.skin {
        let count = clusters
            .get(&skin)
            .copied()
            .ok_or("its skin has no clusters")?;
        weights(scene, skin, count, &logical, &mut data);
    }
    finish_mesh(&mut data)?;
    Ok(data)
}

/// One vertex per distinct corner; returns the mesh and each vertex's logical FBX vertex.
fn weld(corners: &[Corner]) -> Result<(MeshData, Vec<u32>), String> {
    let mut data = MeshData::default();
    let mut logical = Vec::new();
    let mut seen: HashMap<[u32; 9], u32> = HashMap::new();
    for c in corners {
        if !c
            .position
            .iter()
            .chain(&c.normal)
            .chain(&c.uv)
            .all(|v| v.is_finite())
        {
            return Err(format!("vertex {} has a non-finite attribute", c.vertex));
        }
        let index = *seen.entry(key(c)).or_insert_with(|| {
            data.positions.push(c.position);
            data.normals.push(c.normal);
            data.uv0.push([c.uv[0], 1.0 - c.uv[1]]);
            logical.push(c.vertex);
            (logical.len() - 1) as u32
        });
        data.indices.push(index);
    }
    Ok((data, logical))
}

fn key(c: &Corner) -> [u32; 9] {
    let [px, py, pz] = c.position.map(f32::to_bits);
    let [nx, ny, nz] = c.normal.map(f32::to_bits);
    let [u, v] = c.uv.map(f32::to_bits);
    [px, py, pz, nx, ny, nz, u, v, c.vertex]
}

/// The four largest influences of each vertex; joint indices are cluster indices.
fn weights(scene: &Scene<'_>, skin: usize, clusters: usize, logical: &[u32], data: &mut MeshData) {
    for vertex in logical {
        let influences: Vec<(u16, f32)> = scene
            .weights(skin, *vertex)
            .into_iter()
            .filter(|(c, _)| (*c as usize) < clusters)
            .filter_map(|(c, w)| Some((u16::try_from(c).ok()?, w)))
            .collect();
        let (joints, weights) = reduce_influences(&influences);
        data.joints.push(joints);
        data.weights.push(weights);
    }
}
