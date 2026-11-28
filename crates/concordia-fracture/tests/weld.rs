//! Welding (M8 8.1c): imported meshes split vertices along hard edges; the cutter needs them
//! joined.

use concordia_fracture::{FractureError, Mesh, Pattern, fracture, weld};

/// A unit cube with its own four vertices per face (24), as a glTF with flat normals has.
fn split_cube() -> Mesh {
    let corner = |i: u32| [0, 1, 2].map(|a| if i >> a & 1 == 0 { 0.0 } else { 1.0 });
    // Each face's corners, counter-clockwise from outside.
    let faces: [[u32; 4]; 6] = [
        [0, 4, 6, 2],
        [1, 3, 7, 5],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 2, 3, 1],
        [4, 5, 7, 6],
    ];
    let mut mesh = Mesh::default();
    for f in faces {
        let base = mesh.positions.len() as u32;
        mesh.positions.extend(f.map(corner));
        mesh.triangles.push([base, base + 1, base + 2]);
        mesh.triangles.push([base, base + 2, base + 3]);
    }
    mesh
}

#[test]
fn split_vertices_are_welded_so_the_mesh_is_closed() {
    let split = split_cube();
    let pattern = Pattern::Uniform { cells: 4 };
    assert_eq!(fracture(&split, &pattern, 1), Err(FractureError::NotClosed));
    let welded = weld(&split);
    assert_eq!(welded.positions.len(), 8);
    assert_eq!(welded.triangles.len(), 12);
    let f = fracture(&welded, &pattern, 1).unwrap();
    let total: f64 = f.pieces.iter().map(|p| p.volume).sum();
    assert!((total - 1.0).abs() < 1e-5, "{total}");
}

#[test]
fn negative_zero_welds_with_zero_and_bad_indices_stay_bad() {
    let mut split = split_cube();
    split.positions[0] = [-0.0, 0.0, -0.0];
    assert_eq!(weld(&split).positions.len(), 8);
    split.triangles[0][1] = 999;
    let welded = weld(&split);
    assert!(matches!(
        fracture(&welded, &Pattern::Uniform { cells: 2 }, 0),
        Err(FractureError::InvalidMesh(_))
    ));
}
