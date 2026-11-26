//! One strip of the canyon as a heightfield: vertices at the canyon's heights on a grid whose
//! coordinates come from global indices, so neighbouring strips share their edge exactly.

use concordia_assets::MeshData;

use super::build::{Builder, normalize};
use crate::canyon::{Canyon, LENGTH, WIDTH};
use crate::error::GenError;

/// Cells per axis, at most.
const MAX_CELLS: u32 = 4096;

/// Strip `tile` of `of` of `canyon`, in `cells` (across X, along Z).
pub(crate) fn tile(
    canyon: &Canyon,
    tile: u32,
    of: u32,
    cells: [u32; 2],
) -> Result<MeshData, GenError> {
    validate(tile, of, cells)?;
    let [cx, cz] = cells;
    let step = [WIDTH / cx as f32, LENGTH / (of * cz) as f32];
    let x_at = |i: u32| -WIDTH / 2.0 + i as f32 * step[0];
    let z_at = |row: u32| -(row as f32) * step[1];
    let h = |x: f32, z: f32| canyon.height(x, z);
    let mut b = Builder::default();
    for j in 0..=cz {
        let row = tile * cz + j;
        let z = z_at(row);
        for i in 0..=cx {
            let x = x_at(i);
            let dx = h(x + step[0], z) - h(x - step[0], z);
            let dz = h(x, z + step[1]) - h(x, z - step[1]);
            let n = normalize([-dx / (2.0 * step[0]), 1.0, -dz / (2.0 * step[1])]);
            let uv = [i as f32 / cx as f32, row as f32 / (of * cz) as f32];
            b.vertex([x, h(x, z), z], n, uv);
        }
    }
    let width = cx + 1;
    for j in 0..cz {
        for i in 0..cx {
            let a = j * width + i;
            let (right, down) = (a + 1, a + width);
            b.wound_triangles(&[a, down + 1, right, a, down, down + 1]);
        }
    }
    Ok(b.finish())
}

fn validate(tile: u32, of: u32, cells: [u32; 2]) -> Result<(), GenError> {
    if of == 0 {
        return Err(GenError::invalid("of", "needs at least one strip"));
    }
    if tile >= of {
        return Err(GenError::invalid(
            "tile",
            format!("{tile} is not below {of}"),
        ));
    }
    if cells.iter().any(|c| !(1..=MAX_CELLS).contains(c)) {
        return Err(GenError::invalid(
            "cells",
            format!("each must be 1–{MAX_CELLS}"),
        ));
    }
    Ok(())
}
