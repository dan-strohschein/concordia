//! Linear blend skinning on the CPU: the reference the GPU path is tested against, and the
//! conservative bounds of a skinned mesh.

use concordia_assets::MeshData;
use concordia_core::math::{Mat3, Mat4, Vec3, Vec4};

use crate::error::AnimError;

/// A mesh's vertices after skinning, in the bind mesh's space.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkinnedVertices {
    /// Positions.
    pub positions: Vec<Vec3>,
    /// Unit normals.
    pub normals: Vec<Vec3>,
    /// Unit tangents (xyz) with the bitangent sign unchanged (w).
    pub tangents: Vec<Vec4>,
}

/// Bind-space bounds (min, max) of the vertices each joint influences; `None` for joints that
/// influence none.
pub type JointBounds = Vec<Option<(Vec3, Vec3)>>;

/// Skins `mesh` with `palette` (one matrix per joint, from [`crate::Skeleton::palette`]).
pub fn skin_vertices(mesh: &MeshData, palette: &[Mat4]) -> Result<SkinnedVertices, AnimError> {
    check(mesh, palette)?;
    let mut out = SkinnedVertices::default();
    for v in 0..mesh.positions.len() {
        let m = blend(palette, mesh.joints[v], mesh.weights[v]);
        let normal = Vec3::from_array(mesh.normals[v]);
        let tangent = Vec4::from_array(mesh.tangents[v]);
        out.positions
            .push(m.transform_point3(Vec3::from_array(mesh.positions[v])));
        out.normals.push(skin_normal(&m, normal));
        out.tangents.push(skin_tangent(&m, tangent));
    }
    Ok(out)
}

/// Streams of the vertex count, and every joint index inside the palette.
fn check(mesh: &MeshData, palette: &[Mat4]) -> Result<(), AnimError> {
    mesh.validate()
        .map_err(|e| AnimError::Mesh(e.to_string()))?;
    let n = mesh.positions.len();
    for (name, len) in [
        ("joints", mesh.joints.len()),
        ("weights", mesh.weights.len()),
    ] {
        if len != n {
            return Err(AnimError::Mesh(format!(
                "{name} has {len} entries for {n} vertices"
            )));
        }
    }
    for (v, joints) in mesh.joints.iter().enumerate() {
        if let Some(j) = joints.iter().find(|j| usize::from(**j) >= palette.len()) {
            return Err(AnimError::Mesh(format!(
                "vertex {v} uses joint {j}, but the palette has {}",
                palette.len()
            )));
        }
    }
    Ok(())
}

/// The weighted sum of a vertex's joint matrices.
fn blend(palette: &[Mat4], joints: [u16; 4], weights: [f32; 4]) -> Mat4 {
    debug_assert!(joints.iter().all(|j| usize::from(*j) < palette.len()));
    joints
        .iter()
        .zip(weights)
        .fold(Mat4::ZERO, |m, (j, w)| m + palette[usize::from(*j)] * w)
}

/// A normal moved by `m`'s cofactor (the inverse transpose up to scale), normalized.
fn skin_normal(m: &Mat4, n: Vec3) -> Vec3 {
    let b = Mat3::from_mat4(*m);
    let cofactor = Mat3::from_cols(
        b.y_axis.cross(b.z_axis),
        b.z_axis.cross(b.x_axis),
        b.x_axis.cross(b.y_axis),
    );
    let sign = if b.determinant() < 0.0 { -1.0 } else { 1.0 };
    (cofactor * n * sign).normalize_or(n)
}

/// A tangent moved by `m`, normalized; the bitangent sign is kept.
fn skin_tangent(m: &Mat4, t: Vec4) -> Vec4 {
    let xyz = t.truncate();
    m.transform_vector3(xyz).normalize_or(xyz).extend(t.w)
}

/// Per joint, the bind-space box of the vertices it influences with a positive weight.
pub fn joint_bounds(mesh: &MeshData) -> JointBounds {
    let mut boxes: JointBounds = Vec::new();
    for ((p, joints), weights) in mesh.positions.iter().zip(&mesh.joints).zip(&mesh.weights) {
        let p = Vec3::from_array(*p);
        for (j, w) in joints.iter().zip(weights) {
            let j = usize::from(*j);
            if boxes.len() <= j {
                boxes.resize(j + 1, None);
            }
            if *w > 0.0 {
                boxes[j] = Some(boxes[j].map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))));
            }
        }
    }
    boxes
}

/// Bounds containing every vertex skinned with `palette`: the union of each joint's box moved by
/// its matrix. `None` when no joint influences anything, or one that does has no matrix.
pub fn skinned_bounds(joints: &[Option<(Vec3, Vec3)>], palette: &[Mat4]) -> Option<(Vec3, Vec3)> {
    let mut out: Option<(Vec3, Vec3)> = None;
    for (j, bounds) in joints.iter().enumerate() {
        let Some(bounds) = bounds else { continue };
        let (lo, hi) = moved_box(palette.get(j)?, *bounds);
        out = Some(out.map_or((lo, hi), |(a, b)| (a.min(lo), b.max(hi))));
    }
    out
}

/// The axis-aligned box around `bounds` moved by `m` (Arvo's method: center and absolute extents).
fn moved_box(m: &Mat4, (lo, hi): (Vec3, Vec3)) -> (Vec3, Vec3) {
    let center = m.transform_point3((lo + hi) * 0.5);
    let half = (hi - lo) * 0.5;
    let b = Mat3::from_mat4(*m);
    let abs = Mat3::from_cols(b.x_axis.abs(), b.y_axis.abs(), b.z_axis.abs());
    let extent = abs * half;
    (center - extent, center + extent)
}
