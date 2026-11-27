//! The GPU traversal (`shaders/bvh.slang`) finds what the CPU's finds (M4 4.8a). Runs where a
//! Metal device exists (required with CONCORDIA_REQUIRE_GPU=1).

use std::num::NonZeroU64;

use concordia_bvh::{Accel, Bvh, GpuBvh, Instance, Ray, Triangle};
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_rhi::{
    Buffer, BufferDesc, BufferUsage, Command, CommandList, ComputePipelineDesc, Device,
    MemoryLocation, QueueKind, Submit,
};
use concordia_shaders::{
    CompileRequest, ShaderCompiler, SlangToolchain, SlangcCompiler, Stage, Target,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

fn device() -> Option<Box<dyn Device>> {
    match concordia_rhi_metal::new_headless() {
        Ok(d) => Some(d),
        Err(e) => {
            assert!(
                !concordia_testkit::gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but {e}"
            );
            None
        }
    }
}

fn upload(device: &dyn Device, bytes: &[u8]) -> Buffer {
    let size = NonZeroU64::new(bytes.len().max(16) as u64).unwrap();
    let mut data = bytes.to_vec();
    data.resize(size.get() as usize, 0);
    let buffer = device
        .create_buffer(&BufferDesc {
            size,
            usage: BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_SRC,
            memory: MemoryLocation::Upload,
            label: "bvh test".into(),
        })
        .unwrap();
    device.write_buffer(buffer, 0, &data).unwrap();
    buffer
}

fn download(device: &dyn Device, buffer: Buffer, size: u64) -> Vec<u8> {
    let readback = device
        .create_buffer(&BufferDesc {
            size: NonZeroU64::new(size).unwrap(),
            usage: BufferUsage::COPY_DST,
            memory: MemoryLocation::Readback,
            label: "bvh test readback".into(),
        })
        .unwrap();
    let mut list = CommandList::new("bvh readback");
    list.push(Command::CopyBuffer {
        src: buffer,
        src_offset: 0,
        dst: readback,
        dst_offset: 0,
        size,
    });
    submit(device, &list);
    let bytes = device.read_buffer(readback, 0, size).unwrap();
    device.destroy_buffer(readback);
    bytes
}

fn submit(device: &dyn Device, list: &CommandList) {
    device
        .submit(Submit {
            queue: QueueKind::Graphics,
            commands: list,
            wait: &[],
            signal: &[],
        })
        .unwrap();
    device.wait_idle().unwrap();
}

fn words(rows: &[[u32; 4]]) -> Vec<u8> {
    rows.iter()
        .flatten()
        .flat_map(|w| w.to_le_bytes())
        .collect()
}

/// One GPU hit: (t, u, w, instance, prim) when found, and whether anything blocks the ray.
type GpuHit = (Option<(f32, f32, f32, u32, u32)>, bool);

/// The test kernel's pipeline.
fn pipeline(device: &dyn Device) -> concordia_rhi::Pipeline {
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    let module = compiler
        .compile(&CompileRequest {
            path: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/shaders/bvh_rays.slang").into(),
            entry: "trace_rays".into(),
            stage: Stage::Compute,
            target: Target::from(device.caps().backend),
            defines: Default::default(),
            include_dirs: vec![
                concordia_shaders::builtin_dir(),
                concordia_bvh::shader_dir(),
            ],
        })
        .unwrap();
    device
        .create_compute_pipeline(&ComputePipelineDesc {
            shader: module.source(),
            threads_per_group: [64, 1, 1],
            label: "bvh test".into(),
        })
        .unwrap()
}

/// Traces `rays` through `accel` on the GPU.
fn trace(
    device: &dyn Device,
    pipeline: concordia_rhi::Pipeline,
    accel: &Accel,
    rays: &[Ray],
) -> Vec<GpuHit> {
    let packed = GpuBvh::pack(accel);
    let mut owned = Vec::new();
    let mut put = |bytes: &[u8]| {
        let b = upload(device, bytes);
        owned.push(b);
        device.buffer_index(b).unwrap().0
    };
    let order: Vec<u8> = packed.order.iter().flat_map(|w| w.to_le_bytes()).collect();
    let ids = [
        put(&words(&packed.nodes)),
        put(&order),
        put(&words(&packed.tris)),
        put(&words(&packed.meshes)),
        put(&words(&packed.instances)),
    ];
    let header = put(&words(&[
        [ids[0], ids[1], ids[2], ids[3]],
        [ids[4], packed.tlas_node, packed.tlas_order, 0],
    ]));
    let ray_rows: Vec<[u32; 4]> = rays
        .iter()
        .flat_map(|r| {
            [
                [r.origin.x, r.origin.y, r.origin.z, 0.0],
                [r.dir.x, r.dir.y, r.dir.z, 0.0],
            ]
        })
        .map(|v| v.map(f32::to_bits))
        .collect();
    let rays_id = put(&words(&ray_rows));
    let out_size = (rays.len() * 32) as u64;
    let out = put(&vec![0u8; out_size as usize]);
    let out_buffer = *owned.last().unwrap();
    let mut list = CommandList::new("bvh rays");
    let push: Vec<u8> = [header, rays_id, out, rays.len() as u32]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    list.push(Command::SetPipeline(pipeline))
        .push(Command::PushConstants(push))
        .push(Command::Dispatch {
            groups: [(rays.len() as u32).div_ceil(64), 1, 1],
        });
    submit(device, &list);
    let bytes = download(device, out_buffer, out_size);
    for b in owned {
        device.destroy_buffer(b);
    }
    let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    (0..rays.len())
        .map(|k| {
            let base = k * 8;
            let hit = (word(base + 3) == 1).then(|| {
                (
                    f32::from_bits(word(base)),
                    f32::from_bits(word(base + 1)),
                    f32::from_bits(word(base + 2)),
                    word(base + 4),
                    word(base + 5),
                )
            });
            (hit, word(base + 6) == 1)
        })
        .collect()
}

fn v3(r: f32) -> impl Strategy<Value = Vec3> {
    (-r..r, -r..r, -r..r).prop_map(|(x, y, z)| Vec3::new(x, y, z))
}

/// A few meshes placed several times, and rays through the scene.
fn scene() -> impl Strategy<Value = (Vec<Vec<Triangle>>, Vec<Instance>, Vec<Ray>)> {
    let mesh = proptest::collection::vec(
        (v3(2.0), v3(2.0), v3(2.0)).prop_map(|(a, b, c)| Triangle { v: [a, b, c] }),
        1..40,
    );
    (
        proptest::collection::vec(mesh, 1..4),
        proptest::collection::vec((0u32..4, v3(8.0), v3(1.0), 0.3f32..2.0), 0..12),
        proptest::collection::vec(
            (v3(12.0), v3(1.0)).prop_filter("a direction", |(_, d)| d.length() > 1e-2),
            64,
        ),
    )
        .prop_map(|(meshes, places, rays)| {
            let count = meshes.len() as u32;
            let instances = places
                .into_iter()
                .map(|(m, t, axis, s)| {
                    let rotation = if axis.length() > 1e-3 {
                        Quat::from_axis_angle(axis.normalize(), axis.length() * 3.0)
                    } else {
                        Quat::IDENTITY
                    };
                    Instance {
                        mesh: m % count,
                        world: Mat4::from_scale_rotation_translation(Vec3::splat(s), rotation, t),
                    }
                })
                .collect();
            let rays = rays
                .into_iter()
                .map(|(origin, dir)| Ray {
                    origin,
                    dir,
                    t_min: 0.0,
                    t_max: f32::INFINITY,
                })
                .collect();
            (meshes, instances, rays)
        })
}

#[test]
fn gpu_bvh_matches_cpu_hits() {
    let Some(device) = device() else {
        return;
    };
    let pipeline = pipeline(device.as_ref());
    let mut runner = TestRunner::new(Config::with_cases(24));
    runner
        .run(&scene(), |(meshes, instances, rays)| {
            let accel = Accel::build(meshes.iter().map(|m| Bvh::build(m)).collect(), instances);
            let gpu = trace(device.as_ref(), pipeline, &accel, &rays);
            for (ray, (got, blocked)) in rays.iter().zip(gpu) {
                let want = accel.intersect(ray);
                prop_assert_eq!(blocked, want.is_some(), "occlusion agrees");
                match (want, got) {
                    (None, None) => {}
                    (Some(w), Some((t, ..))) => {
                        // Same arithmetic in another precision order: distances agree closely;
                        // different surfaces only at a tie.
                        prop_assert!((w.t - t).abs() <= 1e-4 * w.t.max(1.0), "t {} vs {}", w.t, t);
                    }
                    (w, g) => {
                        // Rounding at an edge may split a grazing ray; allow only that.
                        let edge = |u: f32, w: f32| u.min(w).min(1.0 - u - w);
                        let near_edge = w.map(|h| edge(h.u, h.w)).or(g.map(|h| edge(h.1, h.2)));
                        prop_assert!(
                            near_edge.is_some_and(|e| e < 1e-4),
                            "cpu {:?} gpu {:?}",
                            w,
                            g
                        );
                    }
                }
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn an_empty_scene_has_no_hits() {
    let Some(device) = device() else {
        return;
    };
    let accel = Accel::build(Vec::new(), Vec::new());
    let ray = Ray {
        origin: Vec3::ZERO,
        dir: Vec3::Z,
        t_min: 0.0,
        t_max: f32::INFINITY,
    };
    let pipeline = pipeline(device.as_ref());
    assert_eq!(
        trace(device.as_ref(), pipeline, &accel, &[ray]),
        vec![(None, false)]
    );
}
