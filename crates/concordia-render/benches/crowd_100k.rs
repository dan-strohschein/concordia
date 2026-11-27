//! Perf measurement (not a gate, M2 2.4b.2): a crowd of cheap unskinned meshes at 1,000 and
//! 100,000 entities, 1080p with a shadow-casting sun. Times each stage separately: full
//! extraction, the scene cache's update after every entity moved (a playing crowd), interpolation
//! (`RenderScene::world_at`) and the frame itself (CPU work, the synchronous submit and the
//! readback). Run with `cargo bench -p concordia-render --bench crowd_100k`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use concordia_assets::{Builtin, MeshRenderer};
use concordia_core::id::DeterministicIdGen;
use concordia_core::math::{Quat, Vec3};
use concordia_render::shadow::CascadeSettings;
use concordia_render::{
    Camera, DirectionalLight, MemoryAssets, Projection, RenderRequest, RenderScene, RenderWorld,
    Renderer, SceneCache,
};
use concordia_scene::builtin::Transform;
use concordia_shaders::{SlangToolchain, SlangcCompiler};
use concordia_terra::World;

fn main() {
    let Ok(device) = concordia_rhi_metal::new_headless() else {
        eprintln!("crowd_100k: no GPU device; skipped");
        return;
    };
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    let mut renderer = Renderer::new(Arc::from(device), Box::new(compiler)).expect("renderer");
    for n in [1_000u32, 100_000] {
        let (mut world, mut assets) = crowd(n);
        let extract = median(5, || {
            std::hint::black_box(RenderWorld::extract(&world));
        });
        let mut cache = SceneCache::default();
        let mut scene = RenderScene::default();
        scene.apply(cache.update(&world));
        let update = median(5, || {
            shift(&mut world);
            scene.apply_tick(cache.update(&world));
        });
        let blend = median(5, || {
            std::hint::black_box(scene.world_at(0.5));
        });
        let rw = scene.world_at(0.5);
        let request = RenderRequest {
            camera: None,
            view: None,
            width: 1920,
            height: 1080,
            overlays: None,
            view_mode: Default::default(),
            debug: Vec::new(),
            ui: Default::default(),
        };
        renderer.render(&rw, &mut assets, &request).expect("render");
        let frame = median(11, || {
            renderer.render(&rw, &mut assets, &request).expect("render");
        });
        println!(
            "crowd_100k {n}: extract {:.1} ms, cache update {:.1} ms, interpolate {:.1} ms, frame {:.1} ms; {} draw calls, {} culled",
            ms(extract),
            ms(update),
            ms(blend),
            ms(frame),
            renderer.stats().draw_calls,
            renderer.stats().culled,
        );
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn median(runs: usize, mut f: impl FnMut()) -> Duration {
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .collect();
    times.sort();
    times[runs / 2]
}

/// Moves every entity a little, as a playing crowd would each tick.
fn shift(world: &mut World) {
    let mut query = world.query::<&mut Transform>().unwrap();
    query.for_each(world, |_, t| t.translation.x += 0.01);
}

/// `n` cubes on a 1 m grid under an orthographic camera that sees all of them, and a sun.
pub fn crowd(n: u32) -> (World, MemoryAssets) {
    let (mut world, mut ids, mut assets) = (
        World::new(),
        DeterministicIdGen::new(1),
        MemoryAssets::default(),
    );
    let mesh = assets.insert_mesh("cube", Builtin::mesh("cube").unwrap());
    let material = assets.insert_material(
        "paint",
        concordia_assets::Material::from_slab(concordia_assets::Slab::default()),
    );
    let side = (n as f32).sqrt().ceil() as u32;
    for i in 0..n {
        let e = world.spawn(&mut ids).unwrap();
        let at = Vec3::new(
            (i % side) as f32 - side as f32 / 2.0,
            0.0,
            (i / side) as f32 - side as f32 / 2.0,
        );
        let t = Transform {
            translation: at,
            scale: Vec3::splat(0.4),
            ..Transform::default()
        };
        world.insert(e, t).unwrap();
        let r = MeshRenderer {
            mesh: mesh.clone(),
            material: material.clone(),
            cast_shadows: true,
            ..Default::default()
        };
        world.insert(e, r).unwrap();
    }
    let iso = Quat::from_euler(
        concordia_core::math::EulerRot::YXZ,
        std::f32::consts::FRAC_PI_4,
        -35.264f32.to_radians(),
        0.0,
    );
    let camera = world.spawn(&mut ids).unwrap();
    let t = Transform {
        translation: iso * Vec3::new(0.0, 0.0, side as f32 * 2.0),
        rotation: iso,
        scale: Vec3::ONE,
    };
    world.insert(camera, t).unwrap();
    let c = Camera {
        projection: Projection::Orthographic {
            height: side as f32 * 1.1,
            near: 0.1,
            far: side as f32 * 4.0,
        },
        ..Camera::default()
    };
    world.insert(camera, c).unwrap();
    let sun = world.spawn(&mut ids).unwrap();
    let rotation = Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::new(-0.4, -1.0, -0.6).normalize());
    world
        .insert(
            sun,
            Transform {
                rotation,
                ..Transform::default()
            },
        )
        .unwrap();
    let light = DirectionalLight {
        color: [1.0; 3],
        illuminance_lux: 20_000.0,
        shadows: Some(CascadeSettings::default()),
        ..Default::default()
    };
    world.insert(sun, light).unwrap();
    (world, assets)
}
