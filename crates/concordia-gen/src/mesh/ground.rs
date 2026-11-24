//! A seeded heightfield on XZ.

use concordia_assets::MeshData;

use super::build::{Builder, normalize};
use super::noise;
use super::sweep::{positive, segments_ok};
use crate::error::GenError;

/// `size` metres on XZ, centred, in `cells`, heights ±`amplitude` of value noise at `frequency`.
pub(crate) fn heightfield(
    size: [f32; 2],
    cells: [u32; 2],
    amplitude: f32,
    frequency: f32,
    seed: u64,
) -> Result<MeshData, GenError> {
    positive("size", size[0])?;
    positive("size", size[1])?;
    segments_ok("cells", cells[0], 1)?;
    segments_ok("cells", cells[1], 1)?;
    if !(amplitude.is_finite() && amplitude >= 0.0) {
        return Err(GenError::invalid("amplitude", "must be ≥ 0"));
    }
    positive("frequency", frequency)?;
    let height = |x: f32, z: f32| amplitude * noise::value(x * frequency, z * frequency, seed);
    let step = [size[0] / cells[0] as f32, size[1] / cells[1] as f32];
    let mut b = Builder::default();
    let mut ids = Vec::new();
    for j in 0..=cells[1] {
        for i in 0..=cells[0] {
            let (x, z) = (
                -size[0] / 2.0 + i as f32 * step[0],
                -size[1] / 2.0 + j as f32 * step[1],
            );
            let (dx, dz) = (
                height(x + step[0], z) - height(x - step[0], z),
                height(x, z + step[1]) - height(x, z - step[1]),
            );
            let n = normalize([-dx / (2.0 * step[0]), 1.0, -dz / (2.0 * step[1])]);
            ids.push(b.vertex(
                [x, height(x, z), z],
                n,
                [i as f32 / cells[0] as f32, j as f32 / cells[1] as f32],
            ));
        }
    }
    let row = cells[0] as usize + 1;
    for j in 0..cells[1] as usize {
        for i in 0..cells[0] as usize {
            let a = j * row + i;
            b.quad(ids[a], ids[a + 1], ids[a + row + 1], ids[a + row]);
        }
    }
    Ok(b.finish())
}
