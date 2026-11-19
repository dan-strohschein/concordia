//! Perf smoke (not a gate): 1 024 point lights over a plane at 1080p through an ortho camera.
//! Run with `cargo bench -p concordia-render --bench many_lights`; prints the median frame time
//! (wall clock, including CPU work, the synchronous submit and the readback).

use std::sync::Arc;
use std::time::Instant;

use concordia_assets::{MaterialData, MeshRenderer};
use concordia_core::id::DeterministicIdGen;
use concordia_core::math::{EulerRot, Quat, Vec3};
use concordia_render::{
    Camera, MemoryAssets, PointLight, Projection, RenderRequest, RenderWorld, Renderer,
};
use concordia_scene::builtin::Transform;
use concordia_shaders::{SlangToolchain, SlangcCompiler};
use concordia_terra::World;

fn main() {
    let Ok(device) = concordia_rhi_metal::new_headless() else {
        eprintln!("many_lights: no GPU device; skipped");
        return;
    };
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    let mut renderer = Renderer::new(Arc::from(device), Box::new(compiler)).expect("renderer");
    let (mut world, mut ids, mut assets) = (
        World::new(),
        DeterministicIdGen::new(1),
        MemoryAssets::default(),
    );
    let floor = assets.insert_material(
        "floor",
        MaterialData {
            metallic: 0.0,
            roughness: 0.6,
            ..MaterialData::default()
        },
    );
    let plane = world.spawn(&mut ids).unwrap();
    world
        .insert(
            plane,
            Transform {
                scale: Vec3::splat(80.0),
                ..Transform::default()
            },
        )
        .unwrap();
    world
        .insert(
            plane,
            MeshRenderer {
                mesh: "builtin:mesh/plane".parse().unwrap(),
                material: floor,
                cast_shadows: false,
            },
        )
        .unwrap();
    for i in 0..1024u32 {
        let light = world.spawn(&mut ids).unwrap();
        let at = Vec3::new(
            (i % 32) as f32 * 2.0 - 31.0,
            0.5,
            (i / 32) as f32 * 2.0 - 31.0,
        );
        world
            .insert(
                light,
                Transform {
                    translation: at,
                    ..Transform::default()
                },
            )
            .unwrap();
        world
            .insert(
                light,
                PointLight {
                    color: [1.0, 0.8, 0.6],
                    intensity_lumens: 100.0,
                    range: 3.0,
                },
            )
            .unwrap();
    }
    let iso = Quat::from_euler(
        EulerRot::YXZ,
        std::f32::consts::FRAC_PI_4,
        -35.264f32.to_radians(),
        0.0,
    );
    let camera = world.spawn(&mut ids).unwrap();
    world
        .insert(
            camera,
            Transform {
                translation: iso * Vec3::new(0.0, 0.0, 60.0),
                rotation: iso,
                scale: Vec3::ONE,
            },
        )
        .unwrap();
    world
        .insert(
            camera,
            Camera {
                projection: Projection::Orthographic {
                    height: 40.0,
                    near: 0.1,
                    far: 200.0,
                },
                exposure_ev100: 4.0,
                ..Camera::default()
            },
        )
        .unwrap();
    let rw = RenderWorld::extract(&world);
    let request = RenderRequest {
        camera: None,
        view: None,
        width: 1920,
        height: 1080,
        overlays: None,
        view_mode: Default::default(),
    };
    for _ in 0..3 {
        renderer
            .render(&rw, &mut assets, &request)
            .expect("warm-up frame");
    }
    let mut times: Vec<f64> = (0..15)
        .map(|_| {
            let start = Instant::now();
            renderer.render(&rw, &mut assets, &request).expect("frame");
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    let stats = renderer.stats();
    println!(
        "many_lights: 1024 point lights, 1920x1080 ortho: median {:.2} ms, min {:.2} ms (overflow {})",
        times[7], times[0], stats.cluster_overflow
    );
}
