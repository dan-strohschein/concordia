//! Cutting a mesh into its pattern's cells.

use manifold_sys::safe::{Mesh, clip_convex};

use crate::{Bounds, FractureError, Pattern};

/// Which material a triangle uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Slot {
    /// The source mesh's own surface.
    Surface = 0,
    /// A face made by a cut.
    Interior = 1,
}

/// One piece of a fractured mesh: closed, manifold and not empty.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// The index of the pattern cell it came from.
    pub cell: u32,
    /// Vertex positions.
    pub positions: Vec<[f32; 3]>,
    /// Triangles, counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    /// Each triangle's material.
    pub materials: Vec<Slot>,
    /// Its volume.
    pub volume: f64,
}

/// A mesh cut into pieces, in cell order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fracture {
    /// The pieces; cells that miss the mesh make none.
    pub pieces: Vec<Piece>,
}

/// Cuts `mesh` by `pattern`'s cells for `seed`. The same inputs give the same bytes.
pub fn fracture(mesh: &Mesh, pattern: &Pattern, seed: u64) -> Result<Fracture, FractureError> {
    let bounds = Bounds::of(mesh)?;
    let cells = pattern.cells(&bounds, seed)?;
    let mut pieces = Vec::new();
    for (index, cell) in cells.iter().enumerate() {
        let clipped = clip_convex(mesh, &cell.planes)?;
        if clipped.triangles.is_empty() || clipped.volume <= 0.0 {
            continue;
        }
        pieces.push(Piece {
            cell: u32::try_from(index).map_err(|_| FractureError::Cutter)?,
            positions: clipped.positions,
            triangles: clipped.triangles,
            materials: clipped
                .interior
                .iter()
                .map(|&cut| if cut { Slot::Interior } else { Slot::Surface })
                .collect(),
            volume: clipped.volume,
        });
    }
    Ok(Fracture { pieces })
}
