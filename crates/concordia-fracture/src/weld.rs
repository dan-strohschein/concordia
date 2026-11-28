//! Welding: imported meshes split vertices along hard edges and UV seams, which leaves them
//! open to the cutter. Vertices at the same position become one.

use std::collections::HashMap;

use manifold_sys::safe::Mesh;

/// `mesh` with vertices at bit-identical positions merged (first occurrence first).
pub fn weld(mesh: &Mesh) -> Mesh {
    let mut index: HashMap<[u32; 3], u32> = HashMap::new();
    let mut positions = Vec::new();
    let remap: Vec<u32> = mesh
        .positions
        .iter()
        .map(|p| {
            // -0.0 and 0.0 are the same place.
            let key = p.map(|v| if v == 0.0 { 0 } else { v.to_bits() });
            *index.entry(key).or_insert_with(|| {
                positions.push(*p);
                (positions.len() - 1) as u32
            })
        })
        .collect();
    let triangles = mesh
        .triangles
        .iter()
        .map(|t| t.map(|i| remap.get(i as usize).copied().unwrap_or(u32::MAX)))
        .collect();
    Mesh {
        positions,
        triangles,
    }
}
