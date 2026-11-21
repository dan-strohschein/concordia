//! Skinned draws in one frame: the joint palette buffer, and where each draw is placed (world
//! matrix, object-space bounds and its instance's skin row).

use std::collections::BTreeMap;

use concordia_anim::{JointBounds, skinned_bounds};
use concordia_assets::AssetRef;
use concordia_core::math::{Mat4, Vec3};

use crate::frame::push_rows;
use crate::gpu::GpuMesh;
use crate::{DrawItem, RenderWorld, SkinView};

/// Where one draw goes this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    /// World matrix (a skinned draw takes its posed entity's).
    pub(crate) world: Mat4,
    /// Object-space bounds (posed when skinned).
    pub(crate) bounds: (Vec3, Vec3),
    /// The instance's skin row: first palette matrix and joint count (0: unskinned).
    pub(crate) skin: [u32; 4],
    /// Index into `RenderWorld::skins` when skinned.
    pub(crate) pose: Option<usize>,
}

/// Every draw's placement and the palette rows (four `float4` columns per matrix).
pub(crate) struct SkinFrame {
    pub(crate) placements: Vec<Placement>,
    pub(crate) palette: Vec<u8>,
    /// Skinned meshes drawn at their bind pose because they can't be skinned with their palette.
    pub(crate) refused: Vec<AssetRef>,
}

impl SkinFrame {
    /// Places `world`'s draws; every draw's mesh must be in `meshes`.
    pub(crate) fn new(
        world: &RenderWorld,
        meshes: &BTreeMap<AssetRef, GpuMesh>,
        bounds: &BTreeMap<AssetRef, JointBounds>,
    ) -> Self {
        let (palette, firsts) = palette_rows(&world.skins);
        let mut refused = Vec::new();
        let placements = world
            .draws
            .iter()
            .map(|d| {
                let mesh = &meshes[&d.mesh];
                let unskinned = Placement {
                    world: d.world,
                    bounds: mesh.aabb,
                    skin: [0; 4],
                    pose: None,
                };
                let Some(i) = d.skin.and_then(|owner| pose_of(world, owner)) else {
                    return unskinned;
                };
                match skinned(d, mesh, bounds, &world.skins[i], firsts[i]) {
                    Some(p) => Placement { pose: Some(i), ..p },
                    None => {
                        refused.push(d.mesh.clone());
                        unskinned
                    }
                }
            })
            .collect();
        Self {
            placements,
            palette,
            refused,
        }
    }
}

/// The index of `owner`'s pose in `world.skins` (sorted by entity).
fn pose_of(world: &RenderWorld, owner: concordia_core::id::EntityId) -> Option<usize> {
    world.skins.binary_search_by_key(&owner, |s| s.entity).ok()
}

/// A skinned placement, or `None` when the mesh lacks streams or joints the palette has.
fn skinned(
    draw: &DrawItem,
    mesh: &GpuMesh,
    bounds: &BTreeMap<AssetRef, JointBounds>,
    pose: &SkinView,
    first: u32,
) -> Option<Placement> {
    let count = u32::try_from(pose.palette.len()).ok()?;
    if mesh.joint_count == 0 || mesh.joint_count > count {
        return None;
    }
    let bounds = skinned_bounds(bounds.get(&draw.mesh)?, &pose.palette)?;
    Some(Placement {
        world: pose.world,
        bounds,
        skin: [first, count, 0, 0],
        pose: None,
    })
}

/// Every palette's matrices as rows (at least one matrix, so the buffer is never empty), and
/// each palette's first matrix.
fn palette_rows(skins: &[SkinView]) -> (Vec<u8>, Vec<u32>) {
    let mut rows = Vec::new();
    let mut firsts = Vec::with_capacity(skins.len());
    let mut next = 0u32;
    for skin in skins {
        firsts.push(next);
        for m in &skin.palette {
            push_rows(&mut rows, &[m.x_axis, m.y_axis, m.z_axis, m.w_axis]);
        }
        next += skin.palette.len() as u32;
    }
    if rows.is_empty() {
        let m = Mat4::IDENTITY;
        push_rows(&mut rows, &[m.x_axis, m.y_axis, m.z_axis, m.w_axis]);
    }
    (rows, firsts)
}
