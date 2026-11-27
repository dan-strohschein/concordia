//! Perf smoke (not a gate, 2.4.2): 1,000 characters, each a column skinned to a 65-bone chain,
//! with a shadow-casting sun at 1080p; the same crowd drawn unskinned for comparison.
//! Run with `cargo bench -p concordia-render --bench skinned_crowd`; prints medians (wall clock,
//! including CPU work, the synchronous submit and the readback) and the extraction time.

use std::sync::Arc;
use std::time::Instant;

use concordia_anim::{AnimLibrary, Joint, Pose, Skeleton};
use concordia_assets::{AssetRef, MeshData, MeshRenderer, Skinned};
use concordia_core::id::DeterministicIdGen;
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_render::shadow::CascadeSettings;
use concordia_render::{
    Camera, DirectionalLight, MemoryAssets, Projection, RenderRequest, RenderWorld, Renderer,
};
use concordia_scene::builtin::Transform;
use concordia_shaders::{SlangToolchain, SlangcCompiler};
use concordia_terra::World;

const BONES: usize = 65;
const CHARACTERS: u32 = 1000;
const SPACING: f32 = 0.03;
const SKIN: &str = "asset:018bcfe5-6800-7000-a424-6c55076b1300#skin/0";

fn main() {
    let Ok(device) = concordia_rhi_metal::new_headless() else {
        eprintln!("skinned_crowd: no GPU device; skipped");
        return;
    };
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    let mut renderer = Renderer::new(Arc::from(device), Box::new(compiler)).expect("renderer");
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
    for skinned in [false, true] {
        let (world, mut assets) = crowd(skinned);
        let start = Instant::now();
        let rw = RenderWorld::extract(&world);
        let extract = start.elapsed();
        let mut times = Vec::new();
        for i in 0..23 {
            let start = Instant::now();
            renderer.render(&rw, &mut assets, &request).expect("render");
            if i >= 3 {
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        times.sort_by(f64::total_cmp);
        let label = if skinned { "skinned" } else { "unskinned" };
        println!(
            "skinned_crowd {label}: median {:.2} ms, best {:.2} ms per frame; extraction {:.2} ms; {} draw calls",
            times[times.len() / 2],
            times[0],
            extract.as_secs_f64() * 1000.0,
            renderer.stats().draw_calls,
        );
    }
}

/// A grid of characters, each posed differently when `skinned`, plus a camera and a sun.
fn crowd(skinned: bool) -> (World, MemoryAssets) {
    let (mut world, mut ids, mut assets) = (
        World::new(),
        DeterministicIdGen::new(1),
        MemoryAssets::default(),
    );
    let mesh = assets.insert_mesh("column", column());
    let material = assets.insert_material(
        "paint",
        concordia_assets::Material::from_slab(concordia_assets::Slab::default()),
    );
    let skin: AssetRef = SKIN.parse().unwrap();
    let skeleton = chain();
    for i in 0..CHARACTERS {
        let e = world.spawn(&mut ids).unwrap();
        let at = Vec3::new((i % 40) as f32 - 19.5, 0.0, (i / 40) as f32 - 12.0);
        world
            .insert(
                e,
                Transform {
                    translation: at,
                    ..Transform::default()
                },
            )
            .unwrap();
        world
            .insert(
                e,
                MeshRenderer {
                    mesh: mesh.clone(),
                    material: material.clone(),
                    cast_shadows: true,
                    ..Default::default()
                },
            )
            .unwrap();
        if skinned {
            world.insert(e, Skinned { skin: skin.clone() }).unwrap();
            world.insert(e, pose(&skeleton, i as f32)).unwrap();
        }
    }
    let mut library = AnimLibrary::new();
    library.insert_skeleton(skin, skeleton);
    world.insert_resource(Arc::new(library));
    let iso = Quat::from_euler(
        concordia_core::math::EulerRot::YXZ,
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
                    height: 30.0,
                    near: 0.1,
                    far: 200.0,
                },
                ..Camera::default()
            },
        )
        .unwrap();
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
    world
        .insert(
            sun,
            DirectionalLight {
                color: [1.0; 3],
                illuminance_lux: 20_000.0,
                shadows: Some(CascadeSettings::default()),
            },
        )
        .unwrap();
    (world, assets)
}

/// A straight chain of `BONES` joints, `SPACING` apart, bound at rest.
fn chain() -> Skeleton {
    let joints = (0..BONES)
        .map(|j| Joint {
            name: format!("bone{j}"),
            parent: j.checked_sub(1),
            offset: Mat4::IDENTITY,
            rest: Transform {
                translation: Vec3::new(0.0, if j == 0 { 0.0 } else { SPACING }, 0.0),
                ..Transform::default()
            },
            inverse_bind: Mat4::from_translation(Vec3::new(0.0, -(j as f32) * SPACING, 0.0)),
        })
        .collect();
    Skeleton::new(joints).unwrap()
}

/// Every joint bent a little about Z and X, phased by `seed`.
fn pose(skeleton: &Skeleton, seed: f32) -> Pose {
    let mut pose = skeleton.rest_pose();
    for (j, bone) in pose.bones.iter_mut().enumerate() {
        let t = seed * 0.37 + j as f32 * 0.21;
        bone.rotation =
            Quat::from_rotation_z(0.02 * t.sin()) * Quat::from_rotation_x(0.015 * t.cos());
    }
    pose
}

/// A square column along the chain: one ring per joint, each vertex weighted to its two nearest
/// joints (flat sides, one vertex per face corner).
fn column() -> MeshData {
    let mut m = MeshData::default();
    let half = 0.08;
    let sides = [
        (Vec3::X, Vec3::NEG_Z),
        (Vec3::Z, Vec3::X),
        (Vec3::NEG_X, Vec3::Z),
        (Vec3::NEG_Z, Vec3::NEG_X),
    ];
    for (normal, along) in sides {
        for ring in 0..BONES - 1 {
            let base = m.positions.len() as u32;
            for (k, s) in [(ring, -1.0), (ring, 1.0), (ring + 1, 1.0), (ring + 1, -1.0)] {
                let p = normal * half + along * half * s + Vec3::Y * (k as f32 * SPACING);
                m.positions.push(p.to_array());
                m.normals.push(normal.to_array());
                m.tangents.push(along.extend(1.0).to_array());
                m.uv0.push([0.0, 0.0]);
                let next = (k + 1).min(BONES - 1) as u16;
                m.joints.push([k as u16, next, 0, 0]);
                m.weights.push([0.7, 0.3, 0.0, 0.0]);
            }
            m.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    let top = (BONES - 1) as f32 * SPACING;
    m.aabb = ([-half, 0.0, -half], [half, top, half]);
    m
}
