//! GPU culling on the meshlet path (ADR-0100, M4 4.4): instance and meshlet frustum tests, the
//! normal-cone test and per-cascade shadow culling never change the image (culling on against
//! off, on both draw paths), the GPU decides what the CPU reference decides, and both paths
//! decide alike.

mod common;
use common::{M1, Scene, compute_renderer, golden_in, renderer, with_meshlets};
use concordia_assets::{AssetRef, MeshData, MeshRenderer, shapes};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_render::shadow::CascadeSettings;
use concordia_render::{
    Camera, CullCode, CullView, CulledMeshlet, Culling, DirectionalLight, Renderer,
};

const SUITE: &str = "m4_culling";

fn ball() -> MeshData {
    with_meshlets(shapes::sphere(1.0, 96, 48))
}

fn lit(base: [f32; 3], double_sided: bool) -> M1 {
    M1 {
        base_color: [base[0], base[1], base[2], 1.0],
        metallic: 0.0,
        roughness: 0.5,
        double_sided,
        ..M1::default()
    }
}

/// Exposed for the few-lux sun of these scenes.
fn camera() -> Camera {
    Camera {
        exposure_ev100: 0.0,
        ..Camera::default()
    }
}

/// What the CPU reference needs of each placed instance.
struct Placed {
    world: Mat4,
    mesh: MeshData,
    double_sided: bool,
}

/// A scene and its instances by entity.
struct Culled {
    scene: Scene,
    placed: Vec<(EntityId, Placed)>,
    eye: Vec3,
    target: Vec3,
}

impl Culled {
    fn new() -> Self {
        Self {
            scene: Scene::new(),
            placed: Vec::new(),
            eye: Vec3::ZERO,
            target: Vec3::NEG_Z,
        }
    }

    fn place(
        &mut self,
        mesh: (&AssetRef, &MeshData),
        material: (&AssetRef, bool),
        at: Vec3,
        scale: f32,
    ) {
        let rotation = Quat::from_rotation_y(0.4);
        let id = self.scene.entity(at, rotation, Vec3::splat(scale));
        self.scene
            .world
            .insert(
                id,
                MeshRenderer {
                    mesh: mesh.0.clone(),
                    material: material.0.clone(),
                    cast_shadows: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let world = Mat4::from_scale_rotation_translation(Vec3::splat(scale), rotation, at);
        self.placed.push((
            id,
            Placed {
                world,
                mesh: mesh.1.clone(),
                double_sided: material.1,
            },
        ));
    }

    fn look(&mut self, eye: Vec3, target: Vec3) {
        self.eye = eye;
        self.target = target;
        self.scene.camera(eye, target, camera());
    }
}

/// Nine dense balls in a row wider than the view (some wholly outside, some cut by its sides), three
/// more above the view that only cast shadows into it, a ground on the M1 path, and a shadowing sun.
fn row(eye: Vec3, target: Vec3) -> Culled {
    let mut c = Culled::new();
    let mesh = ball();
    let ball_ref = c.scene.assets.insert_mesh("ball", mesh.clone());
    let red = c.scene.material("red", lit([0.8, 0.2, 0.1], false));
    let ground = c.scene.material("ground", lit([0.7, 0.7, 0.7], false));
    for i in 0..9 {
        let x = -16.0 + 4.0 * i as f32;
        c.place(
            (&ball_ref, &mesh),
            (&red, false),
            Vec3::new(x, 1.0, 0.0),
            1.0,
        );
    }
    for x in [-3.0, 0.0, 3.0] {
        c.place(
            (&ball_ref, &mesh),
            (&red, false),
            Vec3::new(x, 9.0, 4.5),
            1.0,
        );
    }
    c.scene.mesh(
        "plane",
        ground,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(40.0, 1.0, 40.0),
    );
    let id = c.scene.entity(
        Vec3::ZERO,
        Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::new(-0.3, -1.0, -0.2).normalize()),
        Vec3::ONE,
    );
    c.scene
        .world
        .insert(
            id,
            DirectionalLight {
                color: [1.0; 3],
                illuminance_lux: 4.0,
                shadows: Some(CascadeSettings::default()),
                ..Default::default()
            },
        )
        .unwrap();
    c.look(eye, target);
    c
}

/// Renders `c` with `on` and with culling off on `r`: the culled image agrees with the unculled
/// one within golden `name`'s `culling` entry, and the unculled one with the golden.
fn assert_culling_keeps_the_image(
    r: &mut Renderer,
    c: &mut Culled,
    name: &str,
    on: Culling,
    size: (u32, u32),
) {
    r.set_culling(Culling::OFF);
    let off = c.scene.render(r, size.0, size.1);
    r.set_culling(on);
    let culled = c.scene.render(r, size.0, size.1);
    let score = common::assert_culling_agrees(SUITE, name, (&off, &culled), "culling on vs off");
    println!(
        "{name} ({:?}): culling on vs off FLIP {score:?}; {:?}",
        r.stats().raster_path,
        r.stats().meshlets
    );
    golden_in(SUITE, name, &off);
}

/// Every device path: the device's own, and the compute path of a masked device.
fn renderers() -> Vec<Renderer> {
    common::meshlet_renderers()
}

const ROW_SIZE: (u32, u32) = (320, 180);
const ROW_EYE: Vec3 = Vec3::new(0.0, 2.5, 7.0);
const ROW_TARGET: Vec3 = Vec3::new(0.0, 1.0, 0.0);

/// Presented frames (a window's viewport) report their meshlet culling like offscreen ones: the
/// counts arrive when each frame retires (M4's end: the mutation sweep found nothing read them).
#[test]
fn presented_frames_report_their_meshlet_culling() {
    use concordia_render::{RenderRequest, RenderWorld};
    for mut r in renderers() {
        let mut c = row(ROW_EYE, ROW_TARGET);
        let layer = common::test_surface();
        // SAFETY: the layer outlives the surface, which the renderer releases when dropped.
        let surface = unsafe { r.create_surface(layer.target) }.unwrap();
        let rw = RenderWorld::extract(&c.scene.world);
        let request = RenderRequest {
            camera: None,
            view: None,
            width: ROW_SIZE.0,
            height: ROW_SIZE.1,
            overlays: None,
            view_mode: Default::default(),
            debug: Vec::new(),
            ui: Default::default(),
        };
        for _ in 0..6 {
            r.present(&rw, &mut c.scene.assets, &request, surface)
                .unwrap();
        }
        let presented = r.stats().meshlets;
        let _ = c.scene.render(&mut r, ROW_SIZE.0, ROW_SIZE.1);
        let offscreen = r.stats().meshlets;
        assert!(presented.after_cone > 0, "{presented:?}");
        let before_occlusion = |m: concordia_render::MeshletCounts| {
            (
                m.meshlets,
                m.candidates,
                m.after_instance_frustum,
                m.after_frustum,
                m.after_cone,
            )
        };
        assert_eq!(
            before_occlusion(presented),
            before_occlusion(offscreen),
            "presented {presented:?}, offscreen {offscreen:?}"
        );
    }
}

#[test]
fn frustum_culling_matches_culling_off() {
    for mut r in renderers() {
        let mut c = row(ROW_EYE, ROW_TARGET);
        let frustum = Culling {
            frustum: true,
            ..Culling::OFF
        };
        assert_culling_keeps_the_image(&mut r, &mut c, "culling_row", frustum, ROW_SIZE);
        let m = r.stats().meshlets;
        assert!(
            m.after_instance_frustum < m.candidates,
            "whole balls outside: {m:?}"
        );
        assert!(
            m.after_frustum < m.after_instance_frustum,
            "balls cut by the sides: {m:?}"
        );
        assert_eq!(m.after_cone, m.after_frustum, "the cone test is off");
        assert_culling_keeps_the_image(&mut r, &mut c, "culling_row", Culling::default(), ROW_SIZE);
    }
}

#[test]
fn culled_cascades_match_unculled_shadows() {
    for mut r in renderers() {
        let mut c = row(ROW_EYE, ROW_TARGET);
        r.set_culling(Culling::OFF);
        let _ = c.scene.render(&mut r, ROW_SIZE.0, ROW_SIZE.1);
        let all = r.stats().meshlets.shadow_meshlets;
        assert_culling_keeps_the_image(&mut r, &mut c, "culling_row", Culling::default(), ROW_SIZE);
        let culled = r.stats().meshlets.shadow_meshlets;
        assert!(
            culled < all,
            "cascades cull casters outside them: {culled} of {all}"
        );
        assert!(culled > 0);
    }
}

/// Views on a sphere of radius 4 around the origin, from a fixed seed.
fn views(n: usize, seed: u64) -> Vec<Vec3> {
    let mut state = seed;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 40) as f32 / (1u64 << 24) as f32
    };
    (0..n)
        .map(|_| {
            let z = next() * 2.0 - 1.0;
            let a = next() * std::f32::consts::TAU;
            let r = (1.0 - z * z).sqrt();
            Vec3::new(r * a.cos(), z, r * a.sin()) * 4.0
        })
        .collect()
}

/// A single-sided ball and a double-sided open bowl, seen from `eye`.
fn cone_scene(eye: Vec3) -> Culled {
    let mut c = Culled::new();
    let mesh = ball();
    let mut bowl = shapes::sphere(1.0, 96, 48);
    let kept: Vec<u32> = bowl
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| {
            t.iter()
                .map(|&i| bowl.positions[i as usize][1])
                .sum::<f32>()
                < 0.9
        })
        .flatten()
        .copied()
        .collect();
    bowl.indices = kept;
    let bowl = with_meshlets(bowl);
    let ball_ref = c.scene.assets.insert_mesh("ball", mesh.clone());
    let bowl_ref = c.scene.assets.insert_mesh("bowl", bowl.clone());
    let red = c.scene.material("red", lit([0.8, 0.2, 0.1], false));
    let teal = c.scene.material("teal", lit([0.1, 0.6, 0.55], true));
    c.place(
        (&ball_ref, &mesh),
        (&red, false),
        Vec3::new(-0.9, 0.0, 0.0),
        0.8,
    );
    c.place(
        (&bowl_ref, &bowl),
        (&teal, true),
        Vec3::new(1.0, 0.0, 0.0),
        0.8,
    );
    c.scene.sun(Vec3::new(-0.4, -1.0, -0.3), 4.0);
    c.look(eye, Vec3::ZERO);
    c
}

#[test]
fn backface_cone_culling_is_conservative() {
    let size = (128, 128);
    let cone = Culling {
        cone: true,
        ..Culling::OFF
    };
    for mut r in renderers() {
        let mut culled_any = 0;
        for (i, eye) in views(64, 0x4c4).into_iter().enumerate() {
            let mut c = cone_scene(eye);
            if i == 0 {
                assert_culling_keeps_the_image(&mut r, &mut c, "cone_views", cone, size);
            } else {
                r.set_culling(Culling::OFF);
                let off = c.scene.render(&mut r, size.0, size.1);
                r.set_culling(cone);
                let on = c.scene.render(&mut r, size.0, size.1);
                assert!(
                    off.rgba == on.rgba,
                    "view {i} from {eye}: the cone test changed the image"
                );
            }
            let m = r.stats().meshlets;
            culled_any += usize::from(m.after_cone < m.after_frustum);
        }
        assert!(
            culled_any > 32,
            "the cone test culls from most views: {culled_any} of 64"
        );
    }
}

/// How far a code got through the tests (strict ≤ GPU ≤ loose).
fn rank(code: CullCode) -> u32 {
    code as u32
}

/// The CPU reference's code for each of the camera's records of `c`, at `margin`.
fn reference(
    c: &Culled,
    records: &[CulledMeshlet],
    size: (u32, u32),
    margin: f32,
) -> Vec<CullCode> {
    let aspect = size.0 as f32 / size.1 as f32;
    let forward = (c.target - c.eye).normalize();
    let rotation = Quat::from_mat3(&concordia_core::math::Mat3::from_cols(
        forward.cross(Vec3::Y).normalize(),
        forward.cross(Vec3::Y).normalize().cross(forward),
        -forward,
    ));
    let cam = Mat4::from_rotation_translation(rotation, c.eye);
    let view_proj =
        camera().projection.matrix(aspect).unwrap() * concordia_render::view_matrix(&cam);
    let view = CullView::camera(view_proj, c.eye, true, Culling::default());
    let decoded: Vec<_> = c
        .placed
        .iter()
        .map(|(_, p)| concordia_cluster::MeshletMesh::decode(&p.mesh.meshlets).unwrap())
        .collect();
    records
        .iter()
        .map(|rec| {
            let i = c
                .placed
                .iter()
                .position(|(id, _)| *id == rec.entity)
                .unwrap();
            let p = &c.placed[i].1;
            let bounds = (
                Vec3::from_array(p.mesh.aabb.0),
                Vec3::from_array(p.mesh.aabb.1),
            );
            let meshlet = &decoded[i].meshlets[rec.meshlet as usize];
            view.code_within(&p.world, bounds, meshlet, p.double_sided, margin)
        })
        .collect()
}

/// Camera positions around and inside the row, looking along varied directions.
fn reference_views() -> Vec<(Vec3, Vec3)> {
    (0..50)
        .map(|i| {
            let t = i as f32 / 50.0 * std::f32::consts::TAU;
            let eye = Vec3::new(
                14.0 * t.cos(),
                1.5 + 3.0 * (3.0 * t).sin().abs(),
                9.0 * t.sin(),
            );
            let target = Vec3::new(6.0 * (2.0 * t).sin(), 1.0, 0.0);
            (eye, target)
        })
        .collect()
}

#[test]
fn gpu_culling_matches_cpu_reference() {
    let size = (200, 120);
    for mut r in renderers() {
        r.set_cull_readback(true);
        // The CPU reference has no HZB: occlusion is off (as the plan says).
        r.set_culling(Culling {
            occlusion: false,
            ..Culling::default()
        });
        let (mut exact, mut total) = (0, 0);
        for (eye, target) in reference_views() {
            let mut c = row(eye, target);
            let _ = c.scene.render(&mut r, size.0, size.1);
            let records: Vec<_> = r
                .read_culling()
                .unwrap()
                .iter()
                .filter(|m| m.view == 0)
                .copied()
                .collect();
            assert!(!records.is_empty());
            let strict = reference(&c, &records, size, -1e-3);
            let loose = reference(&c, &records, size, 1e-3);
            let cpu = reference(&c, &records, size, 0.0);
            for (k, rec) in records.iter().enumerate() {
                let gpu = rec.code.expect("a known code");
                assert!(
                    rank(strict[k]) <= rank(gpu) && rank(gpu) <= rank(loose[k]),
                    "from {eye} at {target}: {rec:?}: GPU {gpu:?} outside CPU [{:?}, {:?}]",
                    strict[k],
                    loose[k]
                );
                exact += usize::from(gpu == cpu[k]);
                total += 1;
            }
        }
        println!("{exact} of {total} codes equal the exact CPU reference");
        assert!(
            exact * 1000 >= total * 999,
            "{exact} of {total} equal the CPU reference"
        );
    }
}

#[test]
fn compute_fallback_culls_the_same_meshlets_as_mesh_shaders() {
    let Some(mut mesh) = renderer() else { return };
    if common::meshlet_path(&mesh) != concordia_render::RasterPath::mesh_shader {
        return; // No mesh shaders here (MoltenVK): nothing to compare.
    }
    let mut compute = compute_renderer().expect("the same device, masked");
    for r in [&mut mesh, &mut compute] {
        r.set_cull_readback(true);
    }
    for (eye, target) in reference_views().into_iter().step_by(5) {
        let mut c = row(eye, target);
        let _ = c.scene.render(&mut mesh, 200, 120);
        let _ = c.scene.render(&mut compute, 200, 120);
        assert_eq!(
            mesh.read_culling().unwrap(),
            compute.read_culling().unwrap(),
            "from {eye}: every view's codes, slot by slot"
        );
    }
}

/// A frame that reuses the shadow maps culls and draws no casters, and says so; toggling culling
/// redraws them (the switches are part of the cache key).
#[test]
fn cached_shadows_cull_no_casters() {
    for mut r in renderers() {
        r.set_cull_readback(true);
        let mut c = row(ROW_EYE, ROW_TARGET);
        let _ = c.scene.render(&mut r, ROW_SIZE.0, ROW_SIZE.1);
        assert!(!r.stats().shadows_cached);
        assert!(r.stats().meshlets.shadow_meshlets > 0);
        let _ = c.scene.render(&mut r, ROW_SIZE.0, ROW_SIZE.1);
        assert!(r.stats().shadows_cached);
        assert_eq!(r.stats().meshlets.shadow_meshlets, 0);
        assert!(
            r.read_culling().unwrap().iter().all(|m| m.view == 0),
            "camera slots only"
        );
        r.set_culling(Culling::OFF);
        let _ = c.scene.render(&mut r, ROW_SIZE.0, ROW_SIZE.1);
        assert!(!r.stats().shadows_cached, "new switches, new shadows");
    }
}
