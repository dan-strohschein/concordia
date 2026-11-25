//! The meshlet path (M4 4.3a, ADR-0100): static meshes with meshlets drawn through a compute pass
//! and indirect draws look like the M1 path, cast the same shadows, and show in the stats.

mod common;
use common::{M1, Scene, assert_same_as_m1, pixel, renderer, with_meshlets};
use concordia_assets::{AssetRef, ImageSettings, MeshData, MeshRenderer, shapes};
use concordia_core::math::{Quat, Vec3};
use concordia_render::shadow::CascadeSettings;
use concordia_render::{
    Camera, DirectionalLight, Environment, Image, MeshletCounts, RasterPath, Renderer, ViewMode,
};

const SIZE: (u32, u32) = (320, 180);

/// A dense sphere: 96 × 48 quads, 9,216 triangles.
fn sphere() -> MeshData {
    shapes::sphere(1.0, 96, 48)
}

/// The sphere's lower part only (an open bowl), to show back faces.
fn bowl() -> MeshData {
    let mut mesh = sphere();
    let kept: Vec<u32> = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| {
            t.iter()
                .map(|&i| mesh.positions[i as usize][1])
                .sum::<f32>()
                < 0.9
        })
        .flatten()
        .copied()
        .collect();
    mesh.indices = kept;
    mesh
}

/// An entity drawing `mesh` with `material`.
fn place(s: &mut Scene, mesh: &AssetRef, material: &AssetRef, at: Vec3, scale: f32) {
    let id = s.entity(at, Quat::from_rotation_y(0.4), Vec3::splat(scale));
    s.world
        .insert(
            id,
            MeshRenderer {
                mesh: mesh.clone(),
                material: material.clone(),
                cast_shadows: true,
            },
        )
        .unwrap();
}

fn sun(s: &mut Scene, direction: Vec3) {
    let id = s.entity(
        Vec3::ZERO,
        Quat::from_rotation_arc(Vec3::NEG_Z, direction.normalize()),
        Vec3::ONE,
    );
    s.world
        .insert(
            id,
            DirectionalLight {
                color: [1.0; 3],
                illuminance_lux: 4.0,
                shadows: Some(CascadeSettings::default()),
            },
        )
        .unwrap();
}

fn environment(s: &mut Scene) {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/hdri/kloofendal_48d_partly_cloudy_puresky_512.hdr"
    ))
    .unwrap();
    let texture = concordia_assets::decode_image(&bytes, &ImageSettings::default()).unwrap();
    let map = s.assets.insert_texture("sky", texture);
    let id = s.entity(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    s.world
        .insert(
            id,
            Environment {
                map,
                intensity: 0.6,
                skybox: false,
            },
        )
        .unwrap();
}

fn lit(base: [f32; 3], metallic: f32, roughness: f32, double_sided: bool) -> M1 {
    M1 {
        base_color: [base[0], base[1], base[2], 1.0],
        metallic,
        roughness,
        double_sided,
        ..M1::default()
    }
}

/// Dense spheres (two sharing a mesh and material), an open double-sided bowl, a ground on the
/// M1 path, a shadowing sun and image-based light.
fn scene() -> Scene {
    let mut s = Scene::new();
    environment(&mut s);
    sun(&mut s, Vec3::new(-0.6, -1.0, -0.4));
    let ball = s.assets.insert_mesh("ball", with_meshlets(sphere()));
    let bowl = s.assets.insert_mesh("bowl", with_meshlets(bowl()));
    let red = s.material("red", lit([0.8, 0.15, 0.1], 0.0, 0.45, false));
    let gold = s.material("gold", lit([1.0, 0.78, 0.35], 1.0, 0.3, false));
    let teal = s.material("teal", lit([0.1, 0.6, 0.55], 0.0, 0.6, true));
    let ground = s.material("ground", lit([0.7, 0.7, 0.7], 0.0, 0.9, false));
    place(&mut s, &ball, &red, Vec3::new(-2.2, 1.0, 0.0), 1.0);
    place(&mut s, &ball, &red, Vec3::new(2.4, 0.8, -1.0), 0.8);
    place(&mut s, &ball, &gold, Vec3::new(0.0, 1.2, -2.5), 1.2);
    place(&mut s, &bowl, &teal, Vec3::new(0.2, 0.9, 1.2), 0.9);
    s.mesh(
        "plane",
        ground,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(14.0, 1.0, 14.0),
    );
    s.camera(
        Vec3::new(0.0, 4.5, 7.5),
        Vec3::new(0.0, 0.6, -0.5),
        camera(),
    );
    s
}

/// Exposed for the few-lux sun and sky of these scenes.
fn camera() -> Camera {
    Camera {
        exposure_ev100: 0.0,
        ..Camera::default()
    }
}

fn render(r: &mut Renderer, s: &mut Scene, meshlets: bool) -> Image {
    r.set_meshlets(meshlets);
    s.render(r, SIZE.0, SIZE.1)
}

#[test]
fn meshlet_render_matches_forward_plus_golden() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene();
    let m1 = render(&mut r, &mut s, false);
    assert_eq!(r.stats().raster_path, RasterPath::forward);
    let meshlets = render(&mut r, &mut s, true);
    assert_eq!(r.stats().raster_path, RasterPath::compute_indirect);
    assert_same_as_m1("meshlets_vs_forward", &m1, &meshlets);
}

#[test]
fn meshlet_shadows_match_m1_shadows() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    sun(&mut s, Vec3::new(0.0, -1.0, 0.0));
    let ball = s.assets.insert_mesh("ball", with_meshlets(sphere()));
    let white = s.material("white", lit([0.9; 3], 0.0, 0.8, false));
    // The ball floats above the camera's view, so it only casts its shadow into the image.
    place(&mut s, &ball, &white, Vec3::new(0.0, 6.0, 0.0), 1.5);
    s.mesh(
        "plane",
        white.clone(),
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(20.0, 1.0, 20.0),
    );
    s.camera(Vec3::new(0.0, 2.0, 4.0), Vec3::ZERO, camera());
    let m1 = render(&mut r, &mut s, false);
    let meshlets = render(&mut r, &mut s, true);
    assert_eq!(r.stats().meshlets.instances, 0, "the ball is out of view");
    assert_eq!(r.stats().raster_path, RasterPath::compute_indirect);
    let (center, edge) = (pixel(&meshlets, 160, 90), pixel(&meshlets, 10, 90));
    assert!(
        center[0] + 40 < edge[0],
        "the ball's shadow darkens the ground: {center:?} vs {edge:?}"
    );
    assert_same_as_m1("meshlet_shadows", &m1, &meshlets);
}

/// A texture whose alpha is a 8 × 8 checker of 0 and 1 (RGB white).
fn checker() -> concordia_assets::TextureData {
    let side = 64u32;
    let mips = vec![
        (0..side * side)
            .flat_map(|i| {
                let on = ((i % side) / 8 + (i / side) / 8).is_multiple_of(2);
                [255, 255, 255, if on { 255 } else { 0 }]
            })
            .collect(),
    ];
    concordia_assets::TextureData {
        width: side,
        height: side,
        format: concordia_assets::TextureFormat::Rgba8Srgb,
        mips,
    }
}

#[test]
fn masked_material_meshlets_render_alpha_cutouts_correctly() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    sun(&mut s, Vec3::new(-0.5, -1.0, -0.3));
    let checker = s.assets.insert_texture("checker", checker());
    let cut = s.material(
        "cut",
        M1 {
            base_color: [0.9, 0.5, 0.2, 1.0],
            base_color_texture: Some(checker),
            metallic: 0.0,
            roughness: 0.6,
            alpha_mode: concordia_assets::AlphaMode::Mask { cutoff: 0.5 },
            double_sided: true,
            ..M1::default()
        },
    );
    let ground = s.material("ground", lit([0.6, 0.6, 0.65], 0.0, 0.9, false));
    let ball = s.assets.insert_mesh("ball", with_meshlets(sphere()));
    place(&mut s, &ball, &cut, Vec3::new(0.0, 1.3, 0.0), 1.3);
    s.mesh(
        "plane",
        ground,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(12.0, 1.0, 12.0),
    );
    s.camera(Vec3::new(0.0, 2.5, 5.0), Vec3::new(0.0, 1.1, 0.0), camera());
    let m1 = render(&mut r, &mut s, false);
    let meshlets = render(&mut r, &mut s, true);
    assert_eq!(r.stats().meshlets.instances, 1);
    assert_same_as_m1("masked_meshlets", &m1, &meshlets);
}

#[test]
fn render_stats_report_the_meshlet_path() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene();
    let _ = render(&mut r, &mut s, true);
    let stats = r.stats();
    let ball = MeshletInfo::of(&with_meshlets(sphere()));
    let bowl = MeshletInfo::of(&with_meshlets(bowl()));
    assert_eq!(
        stats.meshlets,
        MeshletCounts {
            instances: 4,
            meshlets: 3 * ball.meshlets + bowl.meshlets,
            triangles: 3 * ball.triangles + bowl.triangles,
            indirect_draws: 3,
        },
        "red (two balls), gold and teal"
    );
    assert_eq!(stats.draws, 5, "four meshlet instances and the ground");
    assert_eq!(stats.draw_calls, 4, "three buckets and the ground");
    assert_eq!(stats.triangles, 3 * ball.triangles + bowl.triangles + 2);
}

#[test]
fn meshlets_off_draws_everything_through_m1() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene();
    let _ = render(&mut r, &mut s, false);
    let stats = r.stats();
    assert_eq!(stats.raster_path, RasterPath::forward);
    assert_eq!(stats.meshlets, MeshletCounts::default());
    assert_eq!(
        stats.draw_calls, 4,
        "red (instanced), gold, teal and the ground"
    );
}

/// A mesh's meshlet and triangle counts, read from its blob.
struct MeshletInfo {
    meshlets: u64,
    triangles: u64,
}

impl MeshletInfo {
    fn of(mesh: &MeshData) -> Self {
        let decoded = concordia_cluster::MeshletMesh::decode(&mesh.meshlets).unwrap();
        Self {
            meshlets: decoded.meshlets.len() as u64,
            triangles: (mesh.indices.len() / 3) as u64,
        }
    }
}

/// Distinct colors among the pixels of `image` inside `rect` (x0, y0, x1, y1).
fn colors(image: &Image, (x0, y0, x1, y1): (u32, u32, u32, u32)) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    for y in y0..y1 {
        for x in x0..x1 {
            seen.insert(pixel(image, x, y));
        }
    }
    seen.len()
}

#[test]
fn meshlet_and_triangle_view_modes_color_their_primitives() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene();
    r.set_meshlets(true);
    let meshlets = s.render_mode(&mut r, SIZE.0, SIZE.1, ViewMode::meshlets);
    let triangles = s.render_mode(&mut r, SIZE.0, SIZE.1, ViewMode::triangles);
    r.set_meshlets(false);
    let m1_meshlets = s.render_mode(&mut r, SIZE.0, SIZE.1, ViewMode::meshlets);
    let m1_triangles = s.render_mode(&mut r, SIZE.0, SIZE.1, ViewMode::triangles);
    // The gold ball sits around (160, 62); the ground's front edge around (160, 170).
    let ball = (148, 50, 172, 74);
    assert!(
        colors(&meshlets, ball) > 8,
        "meshlets: one color per meshlet"
    );
    assert!(
        colors(&triangles, ball) > colors(&meshlets, ball),
        "triangles are finer"
    );
    assert!(
        colors(&m1_triangles, ball) > 8,
        "triangles: on the M1 path too"
    );
    let [r0, g0, b0, _] = pixel(&m1_meshlets, 160, 62);
    assert!(
        r0 == g0 && g0 == b0,
        "the M1 path is grey in the meshlets view"
    );
    let [r1, g1, b1, _] = pixel(&meshlets, 160, 170);
    assert!(r1 == g1 && g1 == b1, "the ground (M1 path) is grey");
    common::golden_in("m4_meshlet", "view_meshlets", &meshlets);
    common::golden_in("m4_meshlet", "view_triangles", &triangles);
}
