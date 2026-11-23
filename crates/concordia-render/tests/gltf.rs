//! glTF conformance goldens (M1 phase 1.6): Khronos sample models (fetched by `just gltf-assets`)
//! imported, instantiated, framed by their bounds and lit by the sky environment. Skips without a
//! GPU unless CONCORDIA_REQUIRE_GPU=1.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::{golden, renderer};
use concordia_assets::{AssetDb, AssetRef};
use concordia_command::{Author, CommandLog};
use concordia_core::id::DeterministicIdGen;
use concordia_core::math::{Mat3, Quat, Vec3};
use concordia_import_gltf::GltfImporter;
use concordia_render::{
    Camera, DirectionalLight, Environment, Image, Projection, RenderRequest, RenderWorld,
};
use concordia_scene::builtin::Transform;
use concordia_terra::World;

const MANIFEST: &str = include_str!("../../../tests/fixtures/gltf-sample-assets/manifest.ron");
const SKY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/hdri/kloofendal_48d_partly_cloudy_puresky_512.hdr"
);

fn samples() -> PathBuf {
    let commit = MANIFEST
        .lines()
        .find_map(|l| l.trim().strip_prefix("commit: \"")?.strip_suffix("\","))
        .expect("manifest commit");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/gltf-sample-assets")
        .join(commit)
        .join("Models");
    assert!(
        dir.is_dir(),
        "glTF sample assets are missing: run `just gltf-assets` ({})",
        dir.display()
    );
    dir
}

fn registry() -> concordia_reflect::TypeRegistry {
    let mut reg = concordia_reflect::TypeRegistry::new();
    concordia_scene::builtin::register(&mut reg).unwrap();
    concordia_assets::register_types(&mut reg).unwrap();
    concordia_render::register_types(&mut reg).unwrap();
    reg
}

/// Imports `Models/<model>/<variant>/<file>`, instantiates it with the sky, and renders it framed.
fn render_model(model: &str, variant: &str, file: &str) -> Option<Image> {
    let mut r = renderer()?;
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join(model);
    std::fs::create_dir_all(&dst).unwrap();
    for entry in std::fs::read_dir(samples().join(model).join(variant))
        .unwrap()
        .flatten()
    {
        std::fs::copy(entry.path(), dst.join(entry.file_name())).unwrap();
    }
    std::fs::copy(SKY, dir.path().join("sky.hdr")).unwrap();
    let mut db = AssetDb::new(dir.path(), Box::new(DeterministicIdGen::new(9))).unwrap();
    db.register(Arc::new(GltfImporter));
    let model_report = db.import(&Path::new(model).join(file)).unwrap();
    let sky = db.import(Path::new("sky.hdr")).unwrap();

    let mut reg = registry();
    let (_, prefab) = db
        .load_artifact(&AssetRef::Asset {
            id: model_report.id,
            sub: None,
        })
        .unwrap();
    let doc = concordia_scene::read_prefab(&String::from_utf8(prefab).unwrap(), &reg).unwrap();
    let mut ids = DeterministicIdGen::new(3);
    let (_, ops) = concordia_scene::instantiate_prefab(&doc, &mut ids).unwrap();
    let mut world = World::new();
    let mut log = CommandLog::new();
    log.begin(
        &world,
        Author {
            actor: "agent:test".parse().unwrap(),
            session: None,
        },
        "prefab",
        &mut ids,
    )
    .unwrap();
    for op in ops {
        log.apply(&mut world, &mut reg, op).unwrap();
    }
    log.commit(&mut world, &mut reg).unwrap();

    let (center, radius) = bounds(&world, &mut db);
    let env = world.spawn(&mut ids).unwrap();
    world.insert(env, Transform::default()).unwrap();
    world
        .insert(
            env,
            Environment {
                map: AssetRef::Asset {
                    id: sky.id,
                    sub: None,
                },
                intensity: 1.0,
                skybox: true,
            },
        )
        .unwrap();
    let sun = world.spawn(&mut ids).unwrap();
    world
        .insert(
            sun,
            Transform {
                rotation: Quat::from_rotation_arc(
                    Vec3::NEG_Z,
                    Vec3::new(-0.4, -0.7, -0.6).normalize(),
                ),
                ..Transform::default()
            },
        )
        .unwrap();
    world
        .insert(
            sun,
            DirectionalLight {
                color: [1.0, 0.97, 0.92],
                illuminance_lux: 2.0,
                shadows: None,
            },
        )
        .unwrap();
    let fov = 0.8f32;
    let back = Vec3::new(0.0, 0.25, 1.0).normalize();
    let eye = center + back * (radius / (fov / 2.0).sin() * 1.05);
    let forward = -back;
    let right = forward.cross(Vec3::Y).normalize();
    let rotation = Quat::from_mat3(&Mat3::from_cols(right, right.cross(forward), -forward));
    let camera = world.spawn(&mut ids).unwrap();
    world
        .insert(
            camera,
            Transform {
                translation: eye,
                rotation,
                scale: Vec3::ONE,
            },
        )
        .unwrap();
    let projection = Projection::Perspective {
        fov_y: fov,
        near: radius * 0.01,
        far: radius * 20.0,
    };
    world
        .insert(
            camera,
            Camera {
                projection,
                exposure_ev100: 0.0,
                ..Camera::default()
            },
        )
        .unwrap();
    let rw = RenderWorld::extract(&world);
    Some(
        r.render(
            &rw,
            &mut db,
            &RenderRequest {
                camera: None,
                view: None,
                width: 384,
                height: 384,
                overlays: None,
                view_mode: Default::default(),
                debug: Vec::new(),
                ui: Default::default(),
            },
        )
        .unwrap(),
    )
}

/// Bounding-sphere center and radius of every mesh in world space.
fn bounds(world: &World, db: &mut AssetDb) -> (Vec3, f32) {
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for draw in RenderWorld::extract(world).draws {
        let (a, b) = db.load_mesh(&draw.mesh).unwrap().aabb;
        for i in 0..8 {
            let p = Vec3::new(
                if i & 1 == 0 { a[0] } else { b[0] },
                if i & 2 == 0 { a[1] } else { b[1] },
                if i & 4 == 0 { a[2] } else { b[2] },
            );
            let w = draw.world.transform_point3(p);
            lo = lo.min(w);
            hi = hi.max(w);
        }
    }
    ((lo + hi) / 2.0, ((hi - lo) / 2.0).length().max(1e-3))
}

fn check(name: &str, model: &str, file: &str) {
    if let Some(image) = render_model(model, "glTF-Binary", file) {
        golden(&format!("gltf_{name}"), &image, 0.02);
    }
}

#[test]
fn gltf_metal_rough_spheres_golden() {
    check(
        "metal_rough_spheres",
        "MetalRoughSpheres",
        "MetalRoughSpheres.glb",
    );
}

#[test]
fn gltf_metal_rough_spheres_no_textures_golden() {
    check(
        "metal_rough_spheres_no_textures",
        "MetalRoughSpheresNoTextures",
        "MetalRoughSpheresNoTextures.glb",
    );
}

#[test]
fn gltf_normal_tangent_test_golden() {
    check(
        "normal_tangent_test",
        "NormalTangentTest",
        "NormalTangentTest.glb",
    );
}

#[test]
fn gltf_normal_tangent_mirror_test_golden() {
    check(
        "normal_tangent_mirror_test",
        "NormalTangentMirrorTest",
        "NormalTangentMirrorTest.glb",
    );
}

#[test]
fn gltf_alpha_blend_mode_test_golden() {
    check(
        "alpha_blend_mode_test",
        "AlphaBlendModeTest",
        "AlphaBlendModeTest.glb",
    );
}

#[test]
fn gltf_orientation_test_golden() {
    check("orientation_test", "OrientationTest", "OrientationTest.glb");
}

#[test]
fn gltf_boombox_golden() {
    check("boombox", "BoomBox", "BoomBox.glb");
}

#[test]
fn gltf_flight_helmet_golden() {
    if let Some(image) = render_model("FlightHelmet", "glTF", "FlightHelmet.gltf") {
        golden("gltf_flight_helmet", &image, 0.02);
    }
}
