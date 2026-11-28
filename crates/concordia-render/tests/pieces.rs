//! Rigid pieces (M8 8.5b): a breakable's parts share one surface and one interior mesh whose
//! vertices sit on their piece's joint; each part (`PieceRenderer`) draws through the skinning
//! path with its own palette, so every part of one breakable is one instanced draw per mesh.

mod common;

use common::{Scene, pixel, renderer, unlit};
use concordia_assets::{AssetRef, MeshData, MeshRenderer, PieceRenderer, shapes};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_render::{Camera, Msaa, Projection, RenderRequest, RenderWorld, Tonemap};

/// `n` cubes of side `size` in a row along x from `x0`, 1.5 × `size` apart, each on its own joint.
fn row(n: u32, size: f32, x0: f32) -> MeshData {
    let mut mesh = MeshData::default();
    for i in 0..n {
        let cube = shapes::cube([size; 3]);
        let base = mesh.positions.len() as u32;
        let dx = x0 + i as f32 * size * 1.5;
        mesh.positions
            .extend(cube.positions.iter().map(|p| [p[0] + dx, p[1], p[2]]));
        mesh.normals.extend(&cube.normals);
        mesh.tangents.extend(&cube.tangents);
        mesh.uv0.extend(&cube.uv0);
        mesh.joints
            .extend(cube.positions.iter().map(|_| [i as u16, 0, 0, 0]));
        mesh.weights
            .extend(cube.positions.iter().map(|_| [1.0, 0.0, 0.0, 0.0]));
        mesh.indices.extend(cube.indices.iter().map(|i| i + base));
    }
    mesh.aabb = MeshData::compute_aabb(&mesh.positions);
    mesh
}

/// A breakable of `n` pieces: a surface row and a smaller interior row, two materials.
struct Pieces {
    meshes: Vec<MeshRenderer>,
    pieces: u32,
}

fn breakable(s: &mut Scene, n: u32) -> Pieces {
    let size = 4.0 / (n as f32 * 1.5);
    let surface = s
        .assets
        .insert_mesh(&format!("surface{n}"), row(n, size, -2.0));
    let interior = s
        .assets
        .insert_mesh(&format!("interior{n}"), row(n, size * 0.5, -2.0));
    let red = s.material("red", unlit([0.8, 0.1, 0.1, 1.0]));
    let grey = s.material("grey", unlit([0.4, 0.4, 0.4, 1.0]));
    let renderer = |mesh: AssetRef, material: AssetRef| MeshRenderer {
        mesh,
        material,
        cast_shadows: true,
        ..MeshRenderer::default()
    };
    Pieces {
        meshes: vec![renderer(surface, red), renderer(interior, grey)],
        pieces: n,
    }
}

fn fragment(s: &mut Scene, b: &Pieces, at: Vec3, shown: Vec<u32>) -> EntityId {
    let id = s.entity(at, Quat::IDENTITY, Vec3::ONE);
    let r = PieceRenderer {
        meshes: b.meshes.clone(),
        pieces: b.pieces,
        shown,
    };
    s.world.insert(id, r).unwrap();
    id
}

/// An orthographic camera on +z: 4 m tall in a 96 × 64 image, so (x, y) is at pixel
/// (48 + 16x, 32 − 16y).
fn camera(s: &mut Scene) {
    let camera = Camera {
        projection: Projection::Orthographic {
            height: 4.0,
            near: 0.1,
            far: 20.0,
        },
        msaa: Msaa::Off,
        tonemap: Tonemap::Linear,
        clear: [0.02, 0.02, 0.03, 1.0],
        ..Camera::default()
    };
    s.camera(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO, camera);
}

fn request() -> RenderRequest {
    RenderRequest {
        camera: None,
        view: None,
        width: 96,
        height: 64,
        overlays: None,
        view_mode: Default::default(),
        debug: Vec::new(),
        ui: Default::default(),
    }
}

#[test]
fn piece_renderers_draw_each_mesh_skinned_by_a_palette_of_their_pieces() {
    let mut s = Scene::new();
    let b = breakable(&mut s, 4);
    let id = fragment(&mut s, &b, Vec3::X, vec![1, 3, 9]);
    let world = RenderWorld::extract(&s.world);
    let draws: Vec<_> = world.draws.iter().filter(|d| d.entity == id).collect();
    assert_eq!(draws.len(), 2, "surface and interior");
    assert!(draws.iter().all(|d| d.skin == Some(id)));
    assert_eq!(draws[0].world, Mat4::from_translation(Vec3::X));
    let drawn = world.represented(Vec3::ZERO);
    let skin = drawn.skins.iter().find(|k| k.entity == id).unwrap();
    assert_eq!(
        skin.palette,
        [Mat4::ZERO, Mat4::IDENTITY, Mat4::ZERO, Mat4::IDENTITY],
        "its pieces in place, the rest collapsed; piece 9 isn't one"
    );
    assert!(skin.skeleton.is_none() && skin.joints.is_empty());

    let mut empty = Scene::new();
    let b = breakable(&mut empty, 4);
    fragment(&mut empty, &b, Vec3::ZERO, Vec::new());
    let none = PieceRenderer {
        pieces: 0,
        ..PieceRenderer::default()
    };
    let other = empty.entity(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    empty.world.insert(other, none).unwrap();
    assert!(
        RenderWorld::extract(&empty.world).draws.is_empty(),
        "nothing shown, nothing drawn"
    );
}

#[test]
fn one_draw_per_breakable_regardless_of_piece_count() {
    let Some(mut r) = renderer() else { return };
    let mut calls = Vec::new();
    for n in [4, 32] {
        for split in [false, true] {
            let mut s = Scene::new();
            camera(&mut s);
            let b = breakable(&mut s, n);
            match split {
                false => {
                    fragment(&mut s, &b, Vec3::ZERO, (0..n).collect());
                }
                // Every piece its own fragment, each nudged so they are separate bodies.
                true => (0..n).for_each(|p| {
                    fragment(&mut s, &b, Vec3::new(0.0, 0.01 * p as f32, 0.0), vec![p]);
                }),
            }
            let world = RenderWorld::extract(&s.world);
            r.render(&world, &mut s.assets, &request()).unwrap();
            let stats = r.stats();
            assert_eq!(stats.culled, 0, "{n} pieces, split {split}: {stats:?}");
            calls.push((n, split, stats.draw_calls));
        }
    }
    assert_eq!(
        calls,
        [(4, false, 2), (4, true, 2), (32, false, 2), (32, true, 2)],
        "one draw per mesh of the breakable, however many pieces and parts"
    );
}

#[test]
fn parts_out_of_view_do_not_split_the_instanced_draw() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    camera(&mut s);
    let b = breakable(&mut s, 8);
    // Every other part flew far out of view; in entity order the culled sit between the seen.
    for p in 0..8 {
        let x = if p % 2 == 0 { 0.0 } else { 100.0 };
        fragment(&mut s, &b, Vec3::new(x, 0.0, 0.0), vec![p]);
    }
    let world = RenderWorld::extract(&s.world);
    r.render(&world, &mut s.assets, &request()).unwrap();
    let stats = r.stats();
    assert_eq!((stats.draws, stats.culled), (8, 8), "{stats:?}");
    assert_eq!(stats.draw_calls, 2, "{stats:?}");
}

#[test]
fn a_split_fragment_draws_only_its_own_pieces() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    camera(&mut s);
    // Two 1 m pieces, centred at x = -1.25 and x = 0.25.
    let surface = s.assets.insert_mesh("pair", row(2, 1.0, -1.25));
    let red = s.material("red", unlit([0.8, 0.1, 0.1, 1.0]));
    let b = Pieces {
        meshes: vec![MeshRenderer {
            mesh: surface,
            material: red,
            cast_shadows: true,
            ..MeshRenderer::default()
        }],
        pieces: 2,
    };
    // Piece 0 stays put; piece 1's part flew 1.25 m up.
    fragment(&mut s, &b, Vec3::ZERO, vec![0]);
    fragment(&mut s, &b, Vec3::new(0.0, 1.25, 0.0), vec![1]);
    let world = RenderWorld::extract(&s.world);
    let image = r.render(&world, &mut s.assets, &request()).unwrap();
    let at = |x: f32, y: f32| pixel(&image, (48.0 + 16.0 * x) as u32, (32.0 - 16.0 * y) as u32);
    let background = at(-2.5, 1.6);
    assert_ne!(at(-1.25, 0.0), background, "piece 0, where it was");
    assert_ne!(at(0.25, 1.25), background, "piece 1, where its part went");
    assert_eq!(
        at(0.25, 0.0),
        background,
        "piece 1 isn't drawn where it was: part one doesn't hold it"
    );
    assert_eq!(at(-1.25, 1.25), background, "nor piece 0 with part two");
    assert_eq!(r.stats().draw_calls, 1);
}
