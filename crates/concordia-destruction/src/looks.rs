//! What fragments draw (M8 8.5b): a `PieceRenderer` of the breakable's baked meshes, showing the
//! fragment's own pieces. Simulation never reads it.

use concordia_assets::{MeshRenderer, PieceRenderer};
use concordia_core::id::EntityId;
use concordia_terra::World;

use crate::PieceLooks;

/// The renderer of a breakable's first fragment: `looks`' meshes with `source`'s material (the
/// entity's own `MeshRenderer`, else the default), every one of its `count` pieces shown.
fn whole(looks: &PieceLooks, source: Option<&MeshRenderer>, count: u32) -> PieceRenderer {
    let source = source.cloned().unwrap_or_default();
    let surface = looks.surface.iter().map(|mesh| MeshRenderer {
        mesh: mesh.clone(),
        ..source.clone()
    });
    let interior = looks.interior.iter().map(|mesh| MeshRenderer {
        mesh: mesh.clone(),
        material: looks
            .interior_material
            .clone()
            .unwrap_or_else(|| source.material.clone()),
        ..source.clone()
    });
    PieceRenderer {
        meshes: surface.chain(interior).collect(),
        pieces: count,
        shown: (0..count).collect(),
    }
}

/// Swaps `id`'s `MeshRenderer` (the source mesh) for its pieces, when the recipe has `looks`.
pub(crate) fn draw_whole(
    world: &mut World,
    id: EntityId,
    looks: Option<&PieceLooks>,
    count: u32,
) -> Result<(), concordia_terra::EcsError> {
    let Some(looks) = looks else {
        return Ok(());
    };
    let pieces = whole(looks, world.get::<MeshRenderer>(id), count);
    if world.get::<MeshRenderer>(id).is_some() {
        world.remove::<MeshRenderer>(id)?;
    }
    world.insert(id, pieces).map(drop)
}

/// Gives `child` its parent's renderer (`drawn`), showing only `pieces`.
pub(crate) fn draw_part(
    world: &mut World,
    child: EntityId,
    drawn: Option<&PieceRenderer>,
    pieces: &[u32],
) -> Result<(), concordia_terra::EcsError> {
    let Some(drawn) = drawn else {
        return Ok(());
    };
    let part = PieceRenderer {
        shown: pieces.to_vec(),
        ..drawn.clone()
    };
    world.insert(child, part).map(drop)
}
