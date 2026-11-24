//! A world plus its assets becomes the scene the renderer would draw (M4 4.1b2).

use std::collections::BTreeMap;

use concordia_assets::{
    AssetRef, AssetSource, Material, MeshData, MeshRenderer, Slab, TextureData, TextureFormat,
};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_pathtrace::{Light, PathTraceError, View, scene_from_world};
use concordia_render_types::{
    Camera, DirectionalLight, Environment, PointLight, Projection, SpotLight,
};
use concordia_scene::builtin::{Parent, Transform};
use concordia_terra::World;

/// In-memory assets for tests (the built-ins resolve through `concordia-assets`).
#[derive(Default)]
struct Assets {
    textures: BTreeMap<AssetRef, TextureData>,
}

impl AssetSource for Assets {
    fn mesh(&mut self, r: &AssetRef) -> Result<MeshData, String> {
        match concordia_assets::Builtin::of(r) {
            Some(concordia_assets::BuiltinAsset::Mesh(m)) => Ok(m),
            _ => Err(format!("{r} is not loaded")),
        }
    }
    fn material(&mut self, r: &AssetRef) -> Result<Material, String> {
        match concordia_assets::Builtin::of(r) {
            Some(concordia_assets::BuiltinAsset::Material(m)) => Ok(m),
            _ => Ok(Material::from_slab(Slab::default())),
        }
    }
    fn texture(&mut self, r: &AssetRef) -> Result<TextureData, String> {
        self.textures
            .get(r)
            .cloned()
            .ok_or_else(|| format!("{r} is not loaded"))
    }
}

struct Fixture {
    world: World,
    ids: DeterministicIdGen,
}

impl Fixture {
    fn new() -> Self {
        Self {
            world: World::new(),
            ids: DeterministicIdGen::new(3),
        }
    }

    fn at(&mut self, translation: Vec3, rotation: Quat) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        self.world
            .insert(
                id,
                Transform {
                    translation,
                    rotation,
                    scale: Vec3::ONE,
                },
            )
            .unwrap();
        id
    }

    fn mesh(&mut self, translation: Vec3, mesh: &str) -> EntityId {
        let id = self.at(translation, Quat::IDENTITY);
        self.world
            .insert(
                id,
                MeshRenderer {
                    mesh: format!("builtin:mesh/{mesh}").parse().unwrap(),
                    material: "builtin:material/default".parse().unwrap(),
                    cast_shadows: true,
                },
            )
            .unwrap();
        id
    }

    fn camera(&mut self, eye: Vec3, priority: i32, ev100: f32) -> EntityId {
        let id = self.at(eye, Quat::IDENTITY);
        self.world
            .insert(
                id,
                Camera {
                    priority,
                    exposure_ev100: ev100,
                    ..Camera::default()
                },
            )
            .unwrap();
        id
    }
}

#[test]
fn a_world_becomes_the_scene_the_renderer_would_draw() {
    let mut f = Fixture::new();
    let parent = f.mesh(Vec3::new(1.0, 0.0, 0.0), "cube");
    let child = f.mesh(Vec3::new(0.0, 2.0, 0.0), "sphere");
    f.world.insert(child, Parent(parent)).unwrap();
    f.mesh(Vec3::new(-3.0, 0.0, 0.0), "cube");
    let lamp = f.at(Vec3::new(0.0, 4.0, 0.0), Quat::IDENTITY);
    f.world.insert(lamp, PointLight::default()).unwrap();
    let down = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let spot = f.at(Vec3::new(2.0, 3.0, 0.0), down);
    f.world.insert(spot, SpotLight::default()).unwrap();
    let sun = f.at(Vec3::ZERO, down);
    f.world.insert(sun, DirectionalLight::default()).unwrap();
    f.camera(Vec3::new(0.0, 1.0, 10.0), 0, 12.0);
    f.camera(Vec3::new(0.0, 1.0, 20.0), 5, 9.0);

    let scene = scene_from_world(&f.world, &mut Assets::default(), &View::Camera(None)).unwrap();
    assert_eq!(scene.accel.instances().len(), 3);
    assert_eq!(
        scene.accel.meshes().len(),
        2,
        "the two cubes share one mesh"
    );
    let child_world = scene.accel.instances()[1].world;
    assert!(
        child_world
            .transform_point3(Vec3::ZERO)
            .distance(Vec3::new(1.0, 2.0, 0.0))
            < 1e-6,
        "parented"
    );
    let kinds: Vec<&str> = scene
        .lights
        .iter()
        .map(|l| match l {
            Light::Point { .. } => "point",
            Light::Spot { axis, .. } => {
                assert!(
                    axis.distance(Vec3::NEG_Y) < 1e-5,
                    "spots point along their -Z: {axis}"
                );
                "spot"
            }
            Light::Sun { travel, .. } => {
                assert!(
                    travel.distance(Vec3::NEG_Y) < 1e-5,
                    "the sun shines along its -Z: {travel}"
                );
                "sun"
            }
        })
        .collect();
    assert_eq!(kinds, ["point", "spot", "sun"], "in entity order");
    assert!(
        scene
            .camera
            .world
            .transform_point3(Vec3::ZERO)
            .distance(Vec3::new(0.0, 1.0, 20.0))
            < 1e-6,
        "the highest-priority camera"
    );
    assert!(
        (scene.camera.exposure
            - Camera {
                exposure_ev100: 9.0,
                ..Camera::default()
            }
            .exposure())
        .abs()
            < 1e-9
    );
}

#[test]
fn a_view_keeps_the_scene_cameras_settings() {
    let mut f = Fixture::new();
    f.mesh(Vec3::ZERO, "cube");
    f.camera(Vec3::new(0.0, 0.0, 5.0), 0, 7.0);
    let projection = Projection::Orthographic {
        height: 3.0,
        near: 0.1,
        far: 50.0,
    };
    let view = View::Look {
        eye: Vec3::new(4.0, 0.0, 0.0),
        target: Vec3::ZERO,
        projection,
    };
    let scene = scene_from_world(&f.world, &mut Assets::default(), &view).unwrap();
    assert_eq!(scene.camera.projection, projection);
    assert!(
        scene
            .camera
            .world
            .transform_point3(Vec3::ZERO)
            .distance(Vec3::new(4.0, 0.0, 0.0))
            < 1e-6
    );
    let forward = scene.camera.world.transform_vector3(Vec3::NEG_Z);
    assert!(
        forward.distance(Vec3::NEG_X) < 1e-6,
        "looks at the target: {forward}"
    );
    let ev7 = Camera {
        exposure_ev100: 7.0,
        ..Camera::default()
    }
    .exposure();
    assert!(
        (scene.camera.exposure - ev7).abs() < 1e-9,
        "exposure from the scene camera"
    );
}

#[test]
fn missing_cameras_and_assets_are_errors_naming_what_is_missing() {
    let mut f = Fixture::new();
    f.mesh(Vec3::ZERO, "cube");
    let err = scene_from_world(&f.world, &mut Assets::default(), &View::Camera(None)).unwrap_err();
    assert_eq!(err, PathTraceError::NoCamera);
    let cam = f.camera(Vec3::new(0.0, 0.0, 5.0), 0, 10.0);
    let other = f.at(Vec3::ZERO, Quat::IDENTITY);
    let err =
        scene_from_world(&f.world, &mut Assets::default(), &View::Camera(Some(other))).unwrap_err();
    assert!(matches!(err, PathTraceError::NoCamera), "{err}");
    let broken = f.at(Vec3::ZERO, Quat::IDENTITY);
    f.world
        .insert(
            broken,
            MeshRenderer {
                mesh: "builtin:mesh/nope".parse().unwrap(),
                ..MeshRenderer::default()
            },
        )
        .unwrap();
    let err =
        scene_from_world(&f.world, &mut Assets::default(), &View::Camera(Some(cam))).unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains(&broken.to_string()) && text.contains("nope"),
        "{text}"
    );
}

#[test]
fn the_environment_texture_becomes_the_sky() {
    let mut f = Fixture::new();
    f.camera(Vec3::ZERO, 0, 10.0);
    let map: AssetRef = "asset:0193a1c2-7e40-7000-8000-0000000000aa"
        .parse()
        .unwrap();
    let sky = f.world.spawn(&mut f.ids).unwrap();
    f.world
        .insert(
            sky,
            Environment {
                map: map.clone(),
                intensity: 2.0,
                skybox: true,
            },
        )
        .unwrap();
    let mut assets = Assets::default();
    let texel = |v: f32| half::f16::from_f32(v).to_le_bytes();
    let bytes: Vec<u8> = [0.5f32, 0.25, 1.0, 1.0]
        .iter()
        .flat_map(|c| texel(*c))
        .collect();
    assets.textures.insert(
        map,
        TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Rgba16Float,
            mips: vec![bytes],
        },
    );
    let scene = scene_from_world(&f.world, &mut assets, &View::Camera(None)).unwrap();
    let env = scene.environment.expect("a sky");
    assert_eq!(
        env.radiance(Vec3::Y),
        Vec3::new(1.0, 0.5, 2.0),
        "texel × intensity"
    );
    let _ = Mat4::IDENTITY;
}
