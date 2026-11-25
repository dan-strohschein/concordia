//! The meshlet path (ADR-0100): static meshes with meshlets, drawn through a compute pass and
//! indirect draws (M4 4.3a) or through mesh shaders (4.3b), look like the M1 path, cast the same
//! shadows, and show in the stats. Each golden scene renders on every path the device has: M1,
//! its meshlet path, and the compute path of a device with mesh shaders masked.

mod common;
use common::{
    M1, Scene, assert_same_as_m1, compute_renderer, meshlet_path, pixel, renderer, with_meshlets,
};
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

/// `s` on `r`'s meshlet path against `m1` and golden `name`; where that path is the mesh-shader
/// path, again on the compute path of a masked device. Returns the meshlet-path image.
fn assert_every_path(r: &mut Renderer, s: &mut Scene, name: &str, m1: &Image) -> Image {
    let meshlets = render(r, s, true);
    assert_eq!(r.stats().raster_path, meshlet_path(r), "{name}");
    assert_same_as_m1(name, m1, &meshlets);
    if meshlet_path(r) == RasterPath::mesh_shader {
        let mut compute = compute_renderer().expect("the same device, masked");
        let image = render(&mut compute, s, true);
        assert_eq!(compute.stats().raster_path, RasterPath::compute_indirect);
        assert_same_as_m1(name, m1, &image);
    }
    meshlets
}

#[test]
fn meshlet_render_matches_forward_plus_golden() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene();
    let m1 = render(&mut r, &mut s, false);
    assert_eq!(r.stats().raster_path, RasterPath::forward);
    assert_every_path(&mut r, &mut s, "meshlets_vs_forward", &m1);
}

/// With mesh shaders masked, the compute fallback draws what the mesh-shader path draws, within the
/// `paths` tolerance of `meshlets_vs_forward` (4.3b).
#[test]
fn compute_fallback_matches_mesh_shader_path() {
    let Some(mut r) = renderer() else { return };
    if meshlet_path(&r) != RasterPath::mesh_shader {
        return; // No mesh shaders here (MoltenVK): the other tests cover the compute path.
    }
    let mut compute = compute_renderer().expect("the same device, masked");
    let mut s = scene();
    let mesh = render(&mut r, &mut s, true);
    assert_eq!(r.stats().raster_path, RasterPath::mesh_shader);
    let fallback = render(&mut compute, &mut s, true);
    assert_eq!(compute.stats().raster_path, RasterPath::compute_indirect);
    let backend = Some(common::backend());
    let golden = concordia_testkit::golden::Golden::from_env();
    let tolerance = golden
        .path_tolerance_on(backend, "m4_meshlet", "meshlets_vs_forward")
        .unwrap()
        .expect("a paths entry for meshlets_vs_forward");
    let rgba = |i: &Image| {
        concordia_testkit::golden::RgbaImage::new(i.width, i.height, i.rgba.clone()).unwrap()
    };
    let score = concordia_testkit::golden::flip_score(&rgba(&mesh), &rgba(&fallback)).unwrap();
    println!("compute vs mesh shaders: FLIP {score:?}");
    assert!(
        tolerance.accepts(&score),
        "compute vs mesh shaders: FLIP {score:?} > {tolerance:?}"
    );
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
    let meshlets = assert_every_path(&mut r, &mut s, "meshlet_shadows", &m1);
    assert_eq!(r.stats().meshlets.instances, 0, "the ball is out of view");
    let (center, edge) = (pixel(&meshlets, 160, 90), pixel(&meshlets, 10, 90));
    assert!(
        center[0] + 40 < edge[0],
        "the ball's shadow darkens the ground: {center:?} vs {edge:?}"
    );
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
    assert_every_path(&mut r, &mut s, "masked_meshlets", &m1);
    assert_eq!(r.stats().meshlets.instances, 1);
}

/// What each meshlet path reports: the same counts, as indirect draws or mesh-task draws.
#[test]
fn render_stats_report_the_meshlet_path() {
    let Some(mut r) = renderer() else { return };
    let Some(mut compute) = compute_renderer() else {
        return;
    };
    let mut s = scene();
    let ball = MeshletInfo::of(&with_meshlets(sphere()));
    let bowl = MeshletInfo::of(&with_meshlets(bowl()));
    for renderer in [&mut r, &mut compute] {
        let _ = render(renderer, &mut s, true);
        let stats = renderer.stats();
        let mesh = stats.raster_path == RasterPath::mesh_shader;
        assert_eq!(stats.raster_path, meshlet_path(renderer));
        assert_eq!(
            stats.meshlets,
            MeshletCounts {
                instances: 4,
                meshlets: 3 * ball.meshlets + bowl.meshlets,
                triangles: 3 * ball.triangles + bowl.triangles,
                indirect_draws: if mesh { 0 } else { 3 },
                mesh_draws: if mesh { 3 } else { 0 },
            },
            "red (two balls), gold and teal"
        );
        assert_eq!(stats.draws, 5, "four meshlet instances and the ground");
        assert_eq!(stats.draw_calls, 4, "three buckets and the ground");
        assert_eq!(stats.triangles, 3 * ball.triangles + bowl.triangles + 2);
    }
}

/// The wireframe and triangles views need per-triangle data that shared mesh-stage vertices can't
/// carry: with mesh shaders, meshlet meshes draw through the compute path in those views (4.3b).
#[test]
fn per_triangle_views_draw_meshlets_through_the_compute_path() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene();
    r.set_meshlets(true);
    for (view, path) in [
        (ViewMode::lit, meshlet_path(&r)),
        (ViewMode::meshlets, meshlet_path(&r)),
        (ViewMode::overdraw, meshlet_path(&r)),
        (ViewMode::wireframe, RasterPath::compute_indirect),
        (ViewMode::triangles, RasterPath::compute_indirect),
    ] {
        let _ = s.render_mode(&mut r, SIZE.0, SIZE.1, view);
        assert_eq!(r.stats().raster_path, path, "{view:?}");
    }
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
    if meshlet_path(&r) == RasterPath::mesh_shader {
        let mut compute = compute_renderer().expect("the same device, masked");
        compute.set_meshlets(true);
        let image = s.render_mode(&mut compute, SIZE.0, SIZE.1, ViewMode::meshlets);
        common::golden_in("m4_meshlet", "view_meshlets", &image);
    }
}

/// Perf mode times `meshlet.expand` before the draws on the compute path; the mesh-shader path has
/// no such pass.
#[test]
fn perf_mode_times_meshlet_expand_before_the_draws() {
    let Some(mut r) = compute_renderer() else {
        return;
    };
    r.set_perf_mode(true);
    let mut s = scene();
    let _ = common::render_timed(&mut r, &mut s, SIZE);
    let names = r.stats().timed_passes;
    let at = |n: &str| names.iter().position(|p| p == n);
    let expand = at("meshlet.expand").unwrap_or_else(|| panic!("{names:?}"));
    assert!(
        expand < at("shadow 0").unwrap() && expand < at("forward").unwrap(),
        "{names:?}"
    );
    r.set_meshlets(false);
    let _ = common::render_timed(&mut r, &mut s, SIZE);
    assert!(!r.stats().timed_passes.iter().any(|p| p == "meshlet.expand"));
    let Some(mut mesh) = renderer() else { return };
    if meshlet_path(&mesh) == RasterPath::mesh_shader {
        mesh.set_perf_mode(true);
        let _ = common::render_timed(&mut mesh, &mut s, SIZE);
        let names = mesh.stats().timed_passes;
        assert!(!names.iter().any(|p| p == "meshlet.expand"), "{names:?}");
    }
}

/// Timings only (perf is deferred): 64 dense balls (590k triangles) at 1280×720, 150 frames
/// after 30 warm-up frames, on the M1, compute and mesh-shader paths; prints each pass's median. Run on the reference Mac:
/// `cargo nextest run -p concordia-render --release --run-ignored only -E 'test(meshlet_path_timings)' --no-capture`.
#[test]
#[ignore = "timings for the As built, run by hand in release"]
fn meshlet_path_timings() {
    let Some(mut r) = renderer() else { return };
    r.set_perf_mode(true);
    let mut s = Scene::new();
    environment(&mut s);
    sun(&mut s, Vec3::new(-0.6, -1.0, -0.4));
    let ball = s.assets.insert_mesh("ball", with_meshlets(sphere()));
    let red = s.material("red", lit([0.8, 0.15, 0.1], 0.0, 0.45, false));
    for i in 0..64 {
        let at = Vec3::new((i % 8) as f32 * 2.5 - 8.75, 1.0, (i / 8) as f32 * -2.5);
        place(&mut s, &ball, &red, at, 1.0);
    }
    s.camera(
        Vec3::new(0.0, 6.0, 8.0),
        Vec3::new(0.0, 0.0, -8.0),
        camera(),
    );
    let mut compute = compute_renderer().expect("the same device, masked");
    compute.set_perf_mode(true);
    let mesh = meshlet_path(&r) == RasterPath::mesh_shader;
    let runs = [
        ("m1", false, false),
        ("compute", true, true),
        ("mesh", true, false),
    ];
    for (label, meshlets, masked) in runs {
        if label == "mesh" && !mesh {
            continue;
        }
        let r = if masked { &mut compute } else { &mut r };
        r.set_meshlets(meshlets);
        let mut samples: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
        for frame in 0..180 {
            let (_, timing) = common::render_timed(r, &mut s, (1280, 720));
            if frame < 30 {
                continue;
            }
            let names = r.stats().timed_passes;
            for p in concordia_render::perf::pass_times(&names, &timing) {
                samples.entry(p.name).or_default().push(p.gpu_ms);
            }
            let total = timing.total_ms.unwrap_or(f64::NAN);
            samples.entry("frame".into()).or_default().push(total);
        }
        for (name, mut ms) in samples {
            ms.sort_by(f64::total_cmp);
            println!("{label} {name}: median {:.3} ms", ms[ms.len() / 2]);
        }
    }
}

/// `count` small separate triangles in a grid facing +Z: a stray triangle between meshlet
/// vertices would span the gaps, where nothing else draws.
fn scattered(count: u32) -> MeshData {
    let mut vertices = Vec::new();
    for i in 0..count {
        let (x, y) = ((i % 24) as f32 * 0.25 - 3.0, (i / 24) as f32 * 0.25 - 1.6);
        for (dx, dy) in [(0.0, 0.0), (0.08, 0.0), (0.0, 0.08)] {
            vertices.push((
                [x + dx, y + dy, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 0.0],
            ));
        }
    }
    shapes::mesh(vertices, (0..count * 3).collect())
}

#[test]
fn meshlets_draw_exactly_their_triangles() {
    for mut r in [renderer(), compute_renderer()].into_iter().flatten() {
        draws_exactly_its_triangles(&mut r);
    }
}

fn draws_exactly_its_triangles(r: &mut Renderer) {
    let mut s = Scene::new();
    // 301 triangles: two full meshlets and a partial one (45 triangles, a run with no padding).
    let pieces = s
        .assets
        .insert_mesh("pieces", with_meshlets(scattered(301)));
    let white = s.material("white", common::unlit([1.0; 4]));
    place(&mut s, &pieces, &white, Vec3::ZERO, 1.0);
    s.camera(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, camera());
    let m1 = render(r, &mut s, false);
    let meshlets = render(r, &mut s, true);
    assert_eq!(r.stats().meshlets.triangles, 301);
    let differing = m1
        .rgba
        .chunks(4)
        .zip(meshlets.rgba.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(differing <= 4, "{differing} pixels differ from the M1 path");
}

/// One bucket of 70 one-meshlet instances: its entries span three object groups (32 + 32 + 6), and
/// every entry of every group draws, pixel for pixel as the M1 path, on each path (4.3b).
#[test]
fn every_entry_of_a_bucket_spanning_object_groups_is_drawn() {
    for mut r in [renderer(), compute_renderer()].into_iter().flatten() {
        let mut s = Scene::new();
        let corners = [[0.0, 0.0, 0.0], [0.3, 0.0, 0.0], [0.0, 0.3, 0.0]];
        let vertices = corners
            .iter()
            .map(|p| (*p, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0]))
            .collect();
        let one = s
            .assets
            .insert_mesh("one", with_meshlets(shapes::mesh(vertices, vec![0, 1, 2])));
        let white = s.material("white", common::unlit([1.0; 4]));
        for i in 0..70u32 {
            let at = Vec3::new(
                (i % 10) as f32 * 0.4 - 2.0,
                (i / 10) as f32 * 0.4 - 1.4,
                0.0,
            );
            place(&mut s, &one, &white, at, 1.0);
        }
        s.camera(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, camera());
        let m1 = render(&mut r, &mut s, false);
        let meshlets = render(&mut r, &mut s, true);
        let stats = r.stats();
        assert_eq!(
            (stats.meshlets.instances, stats.meshlets.meshlets),
            (70, 70)
        );
        let background = pixel(&m1, 0, 0);
        let drawn = m1.rgba.chunks(4).filter(|p| *p != background).count();
        assert!(drawn > 70 * 20, "every triangle covers pixels: {drawn}");
        let differing = m1
            .rgba
            .chunks(4)
            .zip(meshlets.rgba.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            differing, 0,
            "{:?}: pixels differ from the M1 path",
            stats.raster_path
        );
    }
}

/// A blob built by hand: meshlet 0 draws triangle A four times (12 bytes, so no padding follows),
/// meshlet 1 draws B. Read through meshlet 0's references, meshlet 1's first corners name C, a
/// triangle in the vertex buffer that nothing draws; reading one triangle too far would show it.
#[test]
fn triangles_past_a_meshlets_count_are_never_drawn() {
    use concordia_cluster::{Meshlet, MeshletBuildSettings, MeshletMesh, NormalCone, Sphere};
    let tri = |x: f32, y: f32| {
        [(0.0, 0.0), (0.6, 0.0), (0.0, 0.6)].map(|(dx, dy)| {
            (
                [x + dx, y + dy, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 0.0],
            )
        })
    };
    // A at the left, B at the right, C (never indexed) at the top.
    let vertices: Vec<_> = [tri(-2.0, -0.5), tri(1.4, -0.5), tri(-0.3, 1.2)].concat();
    let mut mesh = shapes::mesh(vertices, vec![0, 1, 2, 3, 4, 5]);
    let bounds = Sphere {
        center: [0.0, 0.5, 0.0],
        radius: 3.0,
    };
    let meshlet = |vertex_offset, triangle_offset, triangle_count| Meshlet {
        bounds,
        cone: NormalCone::default(),
        material: 0,
        vertex_offset,
        vertex_count: 6,
        triangle_offset,
        triangle_count,
    };
    let blob = MeshletMesh {
        meshlets: vec![meshlet(0, 0, 4), meshlet(6, 12, 1)],
        // References: meshlet 0 is A then C, meshlet 1 is A then B.
        vertices: vec![0, 1, 2, 6, 7, 8, 0, 1, 2, 3, 4, 5],
        // Meshlet 0 draws its locals 0, 1, 2 (A) four times; meshlet 1 its locals 3, 4, 5 (B),
        // which through meshlet 0's references are C.
        triangles: vec![0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2, 3, 4, 5, 0],
        source_triangles: 5,
        source_vertices: 9,
        settings: MeshletBuildSettings::default(),
    };
    mesh.meshlets = blob.encode().unwrap();
    for mut r in [renderer(), compute_renderer()].into_iter().flatten() {
        let (m1, meshlets, differing) = both_paths(&mut r, mesh.clone());
        assert!(m1.contains(&255), "A and B are drawn");
        assert_eq!(m1, meshlets, "{differing} pixels differ from the M1 path");
    }
}

/// Renders `mesh` unlit white on both paths: (M1 image, meshlet image, differing pixels).
fn both_paths(r: &mut Renderer, mesh: MeshData) -> (Vec<u8>, Vec<u8>, usize) {
    let mut s = Scene::new();
    let mesh = s.assets.insert_mesh("pieces", mesh);
    let white = s.material(
        "white",
        M1 {
            double_sided: true,
            ..common::unlit([1.0; 4])
        },
    );
    place(&mut s, &mesh, &white, Vec3::ZERO, 1.0);
    s.camera(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, camera());
    let (m1, meshlets) = (render(r, &mut s, false).rgba, render(r, &mut s, true).rgba);
    let differing = m1
        .chunks(4)
        .zip(meshlets.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    (m1, meshlets, differing)
}
