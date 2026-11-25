//! Per-pass GPU timing (M4 phase 4.1a): `PassTimer`, perf mode and masked caps. GPU cases skip
//! without a GPU unless CONCORDIA_REQUIRE_GPU=1.

mod common;

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::mpsc;
use std::time::Duration;

use common::{M1, Scene};
use concordia_core::math::{Quat, Vec3};
use concordia_render::graph::{RenderGraph, Use};
use concordia_render::perf::{PassTimer, PassTiming, pass_times};
use concordia_render::{Camera, PointLight, Renderer};
use concordia_rhi::{
    CapsMask, ClearValue, ColorAttachment, Command, CommandList, Format, GpuTiming, Layout, LoadOp,
    MeshPipelineDesc, QueueKind, RenderPassDesc, RhiError, SampleCount, ScopeTime, ShaderSource,
    StoreOp, Submit, Texture, TextureDesc, TextureUsage, TimerScope,
};

fn target_desc(label: &str, size: u32) -> TextureDesc {
    TextureDesc {
        width: NonZeroU32::new(size).unwrap(),
        height: NonZeroU32::new(size).unwrap(),
        mip_levels: NonZeroU32::MIN,
        samples: SampleCount::X1,
        format: Format::Rgba16Float,
        usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
        label: label.into(),
    }
}

/// A pass that clears `texture` (a graph record).
fn clear_pass(texture: concordia_render::graph::GraphTexture) -> concordia_render::graph::Record {
    Box::new(move |ctx| {
        let t = ctx.texture(texture);
        ctx.commands
            .push(Command::BeginRenderPass(RenderPassDesc {
                colors: vec![ColorAttachment {
                    texture: t,
                    resolve: None,
                    load: LoadOp::Clear(ClearValue::Color([0.1, 0.2, 0.3, 1.0])),
                    store: StoreOp::Store,
                }],
                depth: None,
            }))
            .push(Command::EndRenderPass);
        Ok(())
    })
}

/// A graph of passes named `names`, each clearing its own imported texture.
fn clearing_graph(names: &[&str], textures: &[Texture], size: u32) -> RenderGraph {
    let mut g = RenderGraph::new();
    for (name, texture) in names.iter().zip(textures) {
        let t = g.import_texture(
            name,
            *texture,
            target_desc(name, size),
            Layout::Undefined,
            None,
        );
        g.add_pass_recorded(
            name,
            |p| {
                p.uses(t, Use::ColorWrite);
                p.side_effect();
            },
            Some(clear_pass(t)),
        );
    }
    g
}

fn kinds(list: &CommandList) -> Vec<String> {
    list.commands
        .iter()
        .map(|c| match c {
            Command::BeginTimer(s) => format!("begin{}", s.0),
            Command::EndTimer => "end".into(),
            Command::Barrier { .. } => "barrier".into(),
            Command::BeginRenderPass(_) => "pass".into(),
            Command::EndRenderPass => "endpass".into(),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn execute_timed_wraps_each_pass_after_its_barriers() {
    let textures = [Texture::from_raw(1, 0), Texture::from_raw(2, 0)];
    let g = clearing_graph(&["first", "second"], &textures, 64);
    let compiled = g.compile(&|_: &TextureDesc| (1024, 256)).unwrap();
    let mut list = CommandList::new("frame");
    let mut timer = PassTimer::new();
    compiled
        .execute_timed(&g, &BTreeMap::new(), &mut list, &mut timer)
        .unwrap();
    assert_eq!(
        kinds(&list),
        [
            "barrier", "begin0", "pass", "endpass", "end", "barrier", "begin1", "pass", "endpass",
            "end"
        ]
    );
    assert_eq!(timer.names(), ["first", "second"]);
    list.validate().expect("a valid list");
}

#[test]
fn pass_times_name_scopes_in_order() {
    let names = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
    let scope = |i, start_ms, end_ms| ScopeTime {
        scope: TimerScope(i),
        start_ms,
        end_ms,
    };
    let timing = GpuTiming {
        total_ms: Some(3.0),
        scopes: vec![scope(0, 0.0, 1.0), scope(2, 1.5, 2.5)],
    };
    let times = pass_times(&names, &timing);
    let got: Vec<(&str, f64, f64)> = times
        .iter()
        .map(|p| (p.name.as_str(), p.start_ms, p.gpu_ms))
        .collect();
    assert_eq!(got, [("a", 0.0, 1.0), ("c", 1.5, 1.0)], "b never ran");
}

/// Submits `list` timed and waits for its timing.
fn submit_timed(device: &dyn concordia_rhi::Device, list: &CommandList) -> GpuTiming {
    let (tx, rx) = mpsc::channel();
    device
        .submit_timed(
            Submit {
                queue: QueueKind::Graphics,
                commands: list,
                wait: &[],
                signal: &[],
            },
            Box::new(move |t| tx.send(t).unwrap()),
        )
        .unwrap();
    device.wait_idle().unwrap();
    rx.recv_timeout(Duration::from_secs(5)).expect("timing")
}

#[test]
fn pass_timer_reports_every_graph_pass_once() {
    let Some(device) = common::device() else {
        return;
    };
    let names = ["first", "second", "third"];
    let size = 2048;
    let textures: Vec<Texture> = names
        .iter()
        .map(|n| device.create_texture(&target_desc(n, size)).unwrap())
        .collect();
    let g = clearing_graph(&names, &textures, size);
    let compiled = g.compile(&|_: &TextureDesc| (1024, 256)).unwrap();
    let mut list = CommandList::new("three passes");
    let mut timer = PassTimer::new();
    compiled
        .execute_timed(&g, &BTreeMap::new(), &mut list, &mut timer)
        .unwrap();
    let times = pass_times(timer.names(), &submit_timed(device.as_ref(), &list));
    let got: Vec<&str> = times.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(got, names, "each pass once, in graph order: {times:?}");
    assert!(times.iter().all(|p| p.gpu_ms > 0.0), "non-zero: {times:?}");
    assert!(
        times.windows(2).all(|w| w[0].start_ms <= w[1].start_ms),
        "monotonic: {times:?}"
    );
}

/// A lit scene with a shadowing sun, many cubes and lights, at 1280×720: about an M1 gate frame.
fn busy_scene(lights: usize, cubes: i32) -> Scene {
    let mut s = Scene::new();
    let lit = s.material(
        "lit",
        M1 {
            base_color: [0.7, 0.6, 0.5, 1.0],
            ..M1::default()
        },
    );
    for x in -cubes..cubes {
        for z in -cubes..cubes {
            let p = Vec3::new(x as f32 * 1.5, 0.5, z as f32 * 1.5);
            s.mesh("cube", lit.clone(), p, Quat::IDENTITY, Vec3::ONE);
        }
    }
    s.mesh(
        "plane",
        lit,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::splat(4.0 * cubes as f32),
    );
    let sun = s.sun(Vec3::new(-0.4, -1.0, -0.3), 20_000.0);
    s.world
        .get_mut::<concordia_render::DirectionalLight>(sun)
        .unwrap()
        .shadows = Some(Default::default());
    for i in 0..lights {
        let a = i as f32 * 2.399;
        let r = 2.0 + (i % 17) as f32;
        let id = s.entity(
            Vec3::new(a.cos() * r, 1.0, a.sin() * r),
            Quat::IDENTITY,
            Vec3::ONE,
        );
        s.world
            .insert(
                id,
                PointLight {
                    range: 4.0,
                    ..PointLight::default()
                },
            )
            .unwrap();
    }
    s.camera(
        Vec3::new(12.0, 10.0, 12.0),
        Vec3::ZERO,
        Camera {
            exposure_ev100: 10.0,
            ..Camera::default()
        },
    );
    s
}

#[test]
fn perf_mode_times_every_frame_pass() {
    let Some(mut r) = common::renderer() else {
        return;
    };
    r.set_perf_mode(true);
    let mut scene = busy_scene(16, 4);
    let (_, timing) = common::render_timed(&mut r, &mut scene, (320, 180));
    let stats = r.stats();
    assert_eq!(stats.pass_timing, PassTiming::measured);
    assert_eq!(
        stats.timed_passes,
        [
            "lights.cluster",
            "shadow 0",
            "shadow 1",
            "shadow 2",
            "forward",
            "tonemap",
            "readback"
        ]
    );
    let times = pass_times(&stats.timed_passes, &timing);
    assert_eq!(times.len(), stats.timed_passes.len(), "{times:?}");
    let sum: f64 = times.iter().map(|p| p.gpu_ms).sum();
    let total = timing.total_ms.expect("Metal times command buffers");
    assert!(
        sum <= total * 1.05 + 0.01,
        "passes ({sum} ms) fit in the frame ({total} ms): {times:?}"
    );
}

#[test]
fn perf_mode_off_times_no_passes() {
    let Some(mut r) = common::renderer() else {
        return;
    };
    assert!(!r.perf_mode(), "off by default");
    let mut scene = busy_scene(4, 2);
    let (_, timing) = common::render_timed(&mut r, &mut scene, (160, 90));
    let stats = r.stats();
    assert_eq!(stats.pass_timing, PassTiming::off);
    assert!(stats.timed_passes.is_empty());
    assert!(timing.scopes.is_empty(), "{timing:?}");
    assert!(timing.total_ms.is_some(), "the whole frame is still timed");
}

/// What 4.1a can observe of masking: the device refuses the hidden features, the renderer's perf
/// mode reports timing as unsupported, and the image is unchanged (no render path depends on
/// them yet; 4.3's meshlet path will select its compute fallback the same way).
#[test]
fn masked_caps_hide_features_from_render_paths() {
    let mask = CapsMask {
        mesh_shaders: true,
        ray_tracing: true,
        metalfx_temporal: true,
        timestamps: true,
        ..CapsMask::default()
    };
    let (Some(mut masked), Some(mut full)) = (common::renderer_masked(&mask), common::renderer())
    else {
        return;
    };
    let caps = masked.device().caps().clone();
    assert!(!caps.mesh_shaders && !caps.object_shaders && !caps.ray_tracing);
    assert!(!caps.hw_ray_tracing && !caps.metalfx_temporal);
    assert!(!caps.timestamp_granularity.times_passes());
    let msl = |entry: &str| ShaderSource::Msl {
        source: String::new(),
        entry: entry.into(),
    };
    let mesh = masked.device().create_mesh_pipeline(&MeshPipelineDesc {
        object: None,
        mesh: concordia_rhi::MeshStage {
            shader: msl("mesh"),
            threads_per_group: [1, 1, 1],
            max_vertices: 3,
            max_primitives: 1,
        },
        fragment: Some(msl("fragment")),
        color_formats: vec![Format::Rgba8Unorm],
        depth: None,
        samples: concordia_rhi::SampleCount::X1,
        cull: concordia_rhi::CullMode::None,
        blend: concordia_rhi::BlendMode::Opaque,
        color_writes: concordia_rhi::ColorWrites::ALL,
        label: "masked".into(),
    });
    assert!(matches!(mesh, Err(RhiError::Unsupported(_))), "{mesh:?}");

    masked.set_perf_mode(true);
    full.set_perf_mode(true);
    let mut scene = busy_scene(8, 2);
    let (masked_image, timing) = common::render_timed(&mut masked, &mut scene, (160, 90));
    let (full_image, _) = common::render_timed(&mut full, &mut scene, (160, 90));
    let stats = masked.stats();
    assert_eq!(stats.pass_timing, PassTiming::unsupported);
    assert!(stats.timed_passes.is_empty() && timing.scopes.is_empty());
    assert_eq!(full.stats().pass_timing, PassTiming::measured);
    assert_eq!(
        masked_image.rgba, full_image.rgba,
        "masking changes no pixels"
    );
}

/// Perf mode's encoder splits and samples cost under 2% of the frame's GPU time. GPU clocks and
/// load drift by far more than 2% within seconds, so modes alternate every frame (order swapped
/// each pair) and the median of adjacent on/off ratios is compared, which cancels the drift.
#[test]
fn pass_timer_overhead_under_2_percent() {
    if common::backend() != concordia_rhi::Backend::Metal {
        return; // D4: perf is asserted only on Metal, the reference machine's backend.
    }
    let Some(mut r) = common::renderer() else {
        return;
    };
    let mut scene = busy_scene(256, 12);
    let size = (1280, 720);
    for _ in 0..10 {
        common::render_timed(&mut r, &mut scene, size);
    }
    let mut frame = |r: &mut Renderer, perf: bool| {
        r.set_perf_mode(perf);
        common::render_timed(r, &mut scene, size)
            .1
            .total_ms
            .unwrap()
    };
    let mut ratios: Vec<f64> = (0..150)
        .map(|pair| {
            let first = pair % 2 == 0;
            let a = frame(&mut r, first);
            let b = frame(&mut r, !first);
            let (on, off) = if first { (a, b) } else { (b, a) };
            on / off
        })
        .collect();
    ratios.sort_by(f64::total_cmp);
    let median = ratios[ratios.len() / 2];
    eprintln!(
        "perf mode / off, median of {} adjacent pairs: {median:.4}",
        ratios.len()
    );
    assert!(
        median <= 1.02,
        "perf mode costs {:+.1}% of the frame's GPU time",
        (median - 1.0) * 100.0
    );
}
