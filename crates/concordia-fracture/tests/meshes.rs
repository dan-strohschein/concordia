//! The pieces baked for drawing (M8 8.5b): one surface and one interior mesh per breakable, each
//! vertex on its piece's joint, so every fragment of the breakable shares them.

use concordia_assets::AssetRef;
use concordia_fracture::{Fracture, Mesh, Pattern, Slot, fracture, piece_mesh, piece_mesh_ref};

fn slab() -> Mesh {
    let mut positions = Vec::new();
    for i in 0..8u32 {
        let s = |bit: u32, hi: f32| if i & bit == 0 { 0.0 } else { hi };
        positions.push([s(1, 2.0), s(2, 1.0), s(4, 1.0)]);
    }
    let triangles = vec![
        [0, 4, 6],
        [0, 6, 2],
        [1, 3, 7],
        [1, 7, 5],
        [0, 1, 5],
        [0, 5, 4],
        [2, 6, 7],
        [2, 7, 3],
        [0, 2, 3],
        [0, 3, 1],
        [4, 5, 7],
        [4, 7, 6],
    ];
    Mesh {
        positions,
        triangles,
    }
}

fn sliced(count: u32) -> Fracture {
    let pattern = Pattern::Slices {
        normal: [1.0, 0.0, 0.0],
        count,
    };
    fracture(&slab(), &pattern, 0).unwrap()
}

/// The corners of `f`'s triangles of `slot`, piece by piece: (piece, position).
fn corners(f: &Fracture, slot: Slot) -> Vec<(u16, [f32; 3])> {
    let mut out = Vec::new();
    for (i, p) in f.pieces.iter().enumerate() {
        for (t, s) in p.triangles.iter().zip(&p.materials) {
            if *s == slot {
                out.extend(t.map(|v| (i as u16, p.positions[v as usize])));
            }
        }
    }
    out
}

#[test]
fn each_vertex_of_the_baked_meshes_is_on_its_pieces_joint() {
    let f = sliced(4);
    for slot in [Slot::Surface, Slot::Interior] {
        let mesh = piece_mesh(&f, slot, 1.0)
            .unwrap()
            .expect("4 slices have both");
        mesh.validate().unwrap();
        let expected = corners(&f, slot);
        let baked: Vec<(u16, [f32; 3])> = mesh
            .indices
            .iter()
            .map(|&i| (mesh.joints[i as usize][0], mesh.positions[i as usize]))
            .collect();
        assert_eq!(baked, expected, "{slot:?}: every triangle, in piece order");
        assert!(mesh.joints.iter().all(|j| j[1..] == [0, 0, 0]));
        assert!(mesh.weights.iter().all(|w| *w == [1.0, 0.0, 0.0, 0.0]));
        for (n, t) in mesh.normals.iter().zip(&mesh.tangents) {
            let (n, t) = (glam(n), glam(&[t[0], t[1], t[2]]));
            assert!((n.length() - 1.0).abs() < 1e-4 && (t.length() - 1.0).abs() < 1e-4);
            assert!(n.dot(t).abs() < 1e-4, "tangents lie in the face");
        }
    }
}

#[test]
fn baked_normals_face_out_and_interior_uvs_are_the_pieces() {
    let f = sliced(2);
    let mesh = piece_mesh(&f, Slot::Interior, 0.5).unwrap().unwrap();
    let uvs: Vec<[f32; 2]> = f
        .pieces
        .iter()
        .flat_map(|p| {
            let uvs = p.interior_uvs(0.5).unwrap();
            p.materials
                .iter()
                .zip(uvs)
                .filter(|(s, _)| **s == Slot::Interior)
                .flat_map(|(_, c)| c)
                .collect::<Vec<_>>()
        })
        .collect();
    let baked: Vec<[f32; 2]> = mesh.indices.iter().map(|&i| mesh.uv0[i as usize]).collect();
    assert_eq!(baked, uvs);
    // The cut is the plane x = 1: piece 0's face points +x, piece 1's -x.
    for &i in &mesh.indices {
        let want = if mesh.joints[i as usize][0] == 0 {
            1.0
        } else {
            -1.0
        };
        assert!((mesh.normals[i as usize][0] - want).abs() < 1e-4);
    }
    let surface = piece_mesh(&f, Slot::Surface, 0.5).unwrap().unwrap();
    assert!(surface.uv0.iter().all(|uv| *uv == [0.0, 0.0]));
    assert_eq!(surface.aabb, ([0.0; 3], [2.0, 1.0, 1.0]));
}

#[test]
fn one_piece_has_no_interior_mesh_and_a_bad_scale_is_an_error() {
    let f = sliced(1);
    assert_eq!(f.pieces.len(), 1);
    assert!(piece_mesh(&f, Slot::Interior, 1.0).unwrap().is_none());
    assert!(piece_mesh(&f, Slot::Surface, 1.0).unwrap().is_some());
    assert!(piece_mesh(&sliced(2), Slot::Interior, 0.0).is_err());
    assert!(
        piece_mesh(&Fracture::default(), Slot::Surface, 1.0)
            .unwrap()
            .is_none()
    );
}

#[test]
fn baked_mesh_refs_follow_the_pieces_key() {
    let surface = piece_mesh_ref("ab12", Slot::Surface);
    let interior = piece_mesh_ref("ab12", Slot::Interior);
    assert_ne!(surface, interior);
    assert_eq!(surface, piece_mesh_ref("ab12", Slot::Surface), "stable");
    assert_ne!(surface, piece_mesh_ref("ab13", Slot::Surface));
    let AssetRef::Asset { sub, .. } = &surface else {
        panic!("an asset reference");
    };
    assert_eq!(sub.as_deref(), Some("mesh/surface"));
    let text = surface.to_string();
    assert_eq!(text.parse::<AssetRef>().unwrap(), surface, "round-trips");
}

fn glam(v: &[f32; 3]) -> concordia_core::math::Vec3 {
    concordia_core::math::Vec3::from_array(*v)
}
