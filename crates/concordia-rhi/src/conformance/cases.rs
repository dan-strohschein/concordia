//! The conformance cases. Each takes a device and a golden checker and panics on failure.

use crate::barrier::{Access, AccessFlags, Layout, Stages, TextureBarrier};
use crate::caps::SampleCount;
use crate::command::{
    ClearValue, ColorAttachment, Command, CommandList, DepthAttachment, LoadOp, RenderPassDesc,
    StoreOp, TimerScope,
};
use crate::device::{Device, QueueKind, Submit};
use crate::error::RhiError;
use crate::format::Format;
use crate::handle::{Buffer, Texture};
use crate::pipeline::{
    BlendMode, CompareOp, ComputePipelineDesc, DepthState, ShaderSource, StaticSampler,
};
use crate::resource::{BufferUsage, HeapDesc, MemoryLocation, TextureUsage};

use super::Harness;
use super::util::{
    WAIT, buffer, compute, download, graphics, graphics_blended, push, read_texture, run, target,
    texture_desc, upload, words,
};

pub(super) const SIZE: u32 = 256;

pub(super) fn pass(
    texture: Texture,
    load: LoadOp,
    resolve: Option<Texture>,
    depth: Option<DepthAttachment>,
) -> Command {
    Command::BeginRenderPass(RenderPassDesc {
        colors: vec![ColorAttachment {
            texture,
            resolve,
            load,
            store: if resolve.is_some() {
                StoreOp::DontCare
            } else {
                StoreOp::Store
            },
        }],
        depth,
        extent: None,
    })
}

pub(super) fn clear(rgba: [f32; 4]) -> LoadOp {
    LoadOp::Clear(ClearValue::Color(rgba))
}

/// Vertex data: per vertex a clip-space position then a color (float4 each).
pub(super) fn vertices(points: &[([f32; 4], [f32; 4])]) -> Vec<u8> {
    points
        .iter()
        .flat_map(|(p, c)| p.iter().chain(c.iter()))
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

pub(super) fn triangle(z: f32, color: [f32; 4]) -> Vec<([f32; 4], [f32; 4])> {
    vec![
        ([-0.8, -0.8, z, 1.0], color),
        ([0.8, -0.8, z, 1.0], color),
        ([0.0, 0.8, z, 1.0], color),
    ]
}

pub(super) fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE + x) * 4) as usize;
    [image[i], image[i + 1], image[i + 2], image[i + 3]]
}

/// Draws `vertices` (bindless) with the triangle pipeline into `color`.
fn draw(
    device: &dyn Device,
    h: &Harness<'_>,
    color: Texture,
    data: &[u8],
    samples: SampleCount,
    resolve: Option<Texture>,
    count: u32,
) {
    let pipeline = graphics(device, h, "triangle", ("vs", "fs"), samples, None);
    let vb = upload(device, data);
    let index = device.buffer_index(vb).expect("bindless index").0;
    let mut list = CommandList::new("draw");
    list.push(pass(color, clear([0.0, 0.0, 0.0, 1.0]), resolve, None))
        .push(Command::SetPipeline(pipeline))
        .push(push(&[index, 0, 0, 0], &[1.0, 1.0, 1.0, 1.0]))
        .push(Command::Draw {
            vertices: 0..count,
            instances: 0..1,
        })
        .push(Command::EndRenderPass);
    run(device, &list);
}

/// Caps are populated and serialize; the bindless tables are large enough for M1.
pub fn device_reports_caps_and_limits(device: &dyn Device, _: &Harness<'_>) {
    let caps = device.caps();
    assert!(caps.max_bindless_textures >= 16_384, "{caps:?}");
    assert!(caps.max_bindless_buffers >= 16_384, "{caps:?}");
    assert!(
        caps.msaa_sample_counts.contains(&SampleCount::X4),
        "{caps:?}"
    );
    assert!(!caps.device_name.is_empty());
}

/// 1 MiB of patterned bytes survives upload → GPU buffer → readback.
pub fn buffer_upload_and_readback_round_trips(device: &dyn Device, _: &Harness<'_>) {
    let data: Vec<u8> = (0..1u32 << 20)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    let gpu = upload(device, &data);
    assert_eq!(download(device, gpu, data.len() as u64), data);
}

/// A kernel writes out[i] = i * 3 + 1 for 65 536 elements through a bindless pointer.
pub fn compute_dispatch_writes_expected_values(device: &dyn Device, h: &Harness<'_>) {
    const N: u32 = 65_536;
    let out = upload(device, &vec![0u8; N as usize * 4]);
    let pipeline = compute(device, h, "fill");
    let mut list = CommandList::new("fill");
    list.push(Command::SetPipeline(pipeline))
        .push(push(&[device.buffer_index(out).unwrap().0, N], &[]))
        .push(Command::Dispatch {
            groups: [N / 64, 1, 1],
        });
    run(device, &list);
    let got = words(&download(device, out, u64::from(N) * 4));
    assert!(
        got.iter().enumerate().all(|(i, v)| *v == i as u32 * 3 + 1),
        "first: {:?}",
        &got[..8]
    );
}

/// A kernel receives only bindless indices and sums the referenced buffer.
pub fn compute_reads_bindless_buffer_by_index(device: &dyn Device, h: &Harness<'_>) {
    let values: Vec<u8> = (1..=1000u32).flat_map(u32::to_le_bytes).collect();
    let src = upload(device, &values);
    let dst = upload(device, &[0u8; 4]);
    let pipeline = compute(device, h, "sum");
    let (s, d) = (
        device.buffer_index(src).unwrap().0,
        device.buffer_index(dst).unwrap().0,
    );
    let mut list = CommandList::new("sum");
    list.push(Command::SetPipeline(pipeline))
        .push(push(&[s, d, 1000], &[]))
        .push(Command::Dispatch { groups: [1, 1, 1] });
    run(device, &list);
    assert_eq!(words(&download(device, dst, 4)), [500_500]);
}

/// One vertex-pulled triangle, no vertex buffers bound.
pub fn triangle_renders_golden(device: &dyn Device, h: &Harness<'_>) {
    let color = target(device, SIZE, SampleCount::X1);
    let points = vec![
        ([-0.8, -0.8, 0.5, 1.0], [1.0, 0.0, 0.0, 1.0]),
        ([0.8, -0.8, 0.5, 1.0], [0.0, 1.0, 0.0, 1.0]),
        ([0.0, 0.8, 0.5, 1.0], [0.0, 0.0, 1.0, 1.0]),
    ];
    draw(
        device,
        h,
        color,
        &vertices(&points),
        SampleCount::X1,
        None,
        3,
    );
    let image = read_texture(device, color, SIZE);
    assert_eq!(
        pixel(&image, 0, 0),
        [0, 0, 0, 255],
        "corner is the clear color"
    );
    h.golden.check("triangle", SIZE, SIZE, &image);
}

/// Clearing RGBA8 to (0.25, 0.5, 0.75, 1) reads back (64, 128, 191, 255).
pub fn clear_color_readback_is_exact(device: &dyn Device, _: &Harness<'_>) {
    let color = target(device, SIZE, SampleCount::X1);
    let mut list = CommandList::new("clear");
    list.push(pass(color, clear([0.25, 0.5, 0.75, 1.0]), None, None))
        .push(Command::EndRenderPass);
    run(device, &list);
    let image = read_texture(device, color, SIZE);
    assert_eq!(pixel(&image, 7, 200), [64, 128, 191, 255]);
}

/// Reverse-Z (clear 0, GreaterEqual): the nearer triangle wins whatever the draw order.
pub fn depth_test_orders_overlapping_triangles(device: &dyn Device, h: &Harness<'_>) {
    let state = DepthState {
        format: Format::Depth32Float,
        compare: CompareOp::GreaterEqual,
        write: true,
    };
    let pipeline = graphics(
        device,
        h,
        "triangle",
        ("vs", "fs"),
        SampleCount::X1,
        Some(state),
    );
    let depth_usage = TextureUsage::RENDER_TARGET;
    let depth = device
        .create_texture(&texture_desc(
            SIZE,
            Format::Depth32Float,
            SampleCount::X1,
            depth_usage,
        ))
        .unwrap();
    let near = triangle(0.9, [1.0, 0.0, 0.0, 1.0]);
    let far = triangle(0.1, [0.0, 0.0, 1.0, 1.0]);
    for order in [[&near, &far], [&far, &near]] {
        let color = target(device, SIZE, SampleCount::X1);
        let first = upload(device, &vertices(order[0]));
        let second = upload(device, &vertices(order[1]));
        let depth_attachment = DepthAttachment {
            resolve: None,
            texture: depth,
            load: LoadOp::Clear(ClearValue::Depth(0.0)),
            store: StoreOp::DontCare,
        };
        let mut list = CommandList::new("depth");
        list.push(pass(
            color,
            clear([0.0, 0.0, 0.0, 1.0]),
            None,
            Some(depth_attachment),
        ))
        .push(Command::SetPipeline(pipeline));
        for vb in [first, second] {
            list.push(push(
                &[device.buffer_index(vb).unwrap().0, 0, 0, 0],
                &[1.0; 4],
            ))
            .push(Command::Draw {
                vertices: 0..3,
                instances: 0..1,
            });
        }
        list.push(Command::EndRenderPass);
        run(device, &list);
        assert_eq!(
            pixel(&read_texture(device, color, SIZE), SIZE / 2, SIZE / 2),
            [255, 0, 0, 255]
        );
    }
}

/// A 4× MSAA triangle resolves to a single-sample image with intermediate edge values.
pub fn msaa4x_resolve_produces_antialiased_edge(device: &dyn Device, h: &Harness<'_>) {
    let msaa = target(device, SIZE, SampleCount::X4);
    let resolved = target(device, SIZE, SampleCount::X1);
    draw(
        device,
        h,
        msaa,
        &vertices(&triangle(0.5, [1.0, 1.0, 1.0, 1.0])),
        SampleCount::X4,
        Some(resolved),
        3,
    );
    let image = read_texture(device, resolved, SIZE);
    let partial = image
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 10 && p[0] < 245)
        .count();
    assert!(
        partial > 50,
        "only {partial} partially covered pixels: no antialiasing"
    );
    h.golden.check("msaa_edge", SIZE, SIZE, &image);
}

/// A compute write to a texture is visible to a fragment read in the same submission, every time.
pub fn barrier_between_compute_write_and_fragment_read_is_respected(
    device: &dyn Device,
    h: &Harness<'_>,
) {
    let usage = TextureUsage::STORAGE | TextureUsage::SAMPLED | TextureUsage::BINDLESS;
    let storage = device
        .create_texture(&texture_desc(4, Format::Rgba8Unorm, SampleCount::X1, usage))
        .unwrap();
    let index = device.texture_index(storage).unwrap().0;
    let writer = compute(device, h, "write_texture");
    let reader = graphics(
        device,
        h,
        "sample_texture",
        ("fullscreen", "sample_texture"),
        SampleCount::X1,
        None,
    );
    let color = target(device, SIZE, SampleCount::X1);
    for i in 0..100u32 {
        let value = (i % 250) as f32 / 255.0;
        let mut list = CommandList::new("barrier");
        list.push(Command::SetPipeline(writer))
            .push(push(&[index, 0, 0, 0], &[value, 0.0, 0.0, 1.0]))
            .push(Command::Dispatch { groups: [4, 4, 1] })
            .push(Command::Barrier {
                buffers: vec![],
                textures: vec![TextureBarrier {
                    texture: storage,
                    before: Access {
                        stages: Stages::COMPUTE,
                        access: AccessFlags::SHADER_WRITE,
                        layout: Layout::General,
                    },
                    after: Access {
                        stages: Stages::FRAGMENT,
                        access: AccessFlags::SHADER_READ,
                        layout: Layout::ShaderRead,
                    },
                    aliasing: None,
                }],
            })
            .push(pass(color, LoadOp::DontCare, None, None))
            .push(Command::SetPipeline(reader))
            .push(push(&[index, 0, 0, 0], &[0.0; 4]))
            .push(Command::Draw {
                vertices: 0..3,
                instances: 0..1,
            })
            .push(Command::EndRenderPass);
        run(device, &list);
        let got = pixel(&read_texture(device, color, SIZE), 100, 100)[0];
        assert_eq!(u32::from(got), i % 250, "iteration {i}");
    }
}

/// Two textures placed at the same heap offset, used one after the other, each produce their own output.
pub fn transient_textures_aliased_in_same_heap_do_not_corrupt(
    device: &dyn Device,
    _: &Harness<'_>,
) {
    let desc = texture_desc(
        SIZE,
        Format::Rgba8Unorm,
        SampleCount::X1,
        TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC,
    );
    let (size, _) = device.texture_heap_footprint(&desc).unwrap();
    let heap = device
        .create_heap(&HeapDesc {
            size: std::num::NonZeroU64::new(size).unwrap(),
            label: "aliasing".into(),
        })
        .unwrap();
    let a = device.create_placed_texture(heap, 0, &desc).unwrap();
    let b = device.create_placed_texture(heap, 0, &desc).unwrap();
    let bytes = u64::from(SIZE) * u64::from(SIZE) * 4;
    let (ra, rb) = (
        buffer(
            device,
            bytes,
            BufferUsage::COPY_DST,
            MemoryLocation::Readback,
        ),
        buffer(
            device,
            bytes,
            BufferUsage::COPY_DST,
            MemoryLocation::Readback,
        ),
    );
    let mut list = CommandList::new("aliasing");
    for (texture, readback, rgba, previous) in [
        (a, ra, [1.0, 0.0, 0.0, 1.0], None),
        (b, rb, [0.0, 1.0, 0.0, 1.0], Some(a)),
    ] {
        if let Some(previous) = previous {
            let none = Access {
                stages: Stages::empty(),
                access: AccessFlags::empty(),
                layout: Layout::Undefined,
            };
            let rt = Access {
                stages: Stages::COLOR_OUTPUT,
                access: AccessFlags::COLOR_WRITE,
                layout: Layout::ColorAttachment,
            };
            list.push(Command::Barrier {
                buffers: vec![],
                textures: vec![TextureBarrier {
                    texture,
                    before: none,
                    after: rt,
                    aliasing: Some(previous),
                }],
            });
        }
        list.push(pass(texture, clear(rgba), None, None))
            .push(Command::EndRenderPass)
            .push(Command::CopyTextureToBuffer {
                src: texture,
                mip: 0,
                dst: readback,
                offset: 0,
                bytes_per_row: SIZE * 4,
            });
    }
    run(device, &list);
    assert_eq!(&device.read_buffer(ra, 0, 4).unwrap()[..], [255, 0, 0, 255]);
    assert_eq!(&device.read_buffer(rb, 0, 4).unwrap()[..], [0, 255, 0, 255]);
}

/// Values 1..=10 signalled in order are observed in order; the completed value never decreases.
pub fn timeline_fence_signals_monotonically(device: &dyn Device, _: &Harness<'_>) {
    let timeline = device.create_timeline(0).unwrap();
    let empty = CommandList::new("tick");
    for value in 1..=10 {
        let signal = [(timeline, value)];
        device
            .submit(Submit {
                queue: QueueKind::Graphics,
                commands: &empty,
                wait: &[],
                signal: &signal,
            })
            .unwrap();
    }
    let mut last = 0;
    for value in 1..=10 {
        assert!(
            device.wait_timeline(timeline, value, WAIT).unwrap(),
            "timeline reached {value}"
        );
        let now = device.timeline_value(timeline).unwrap();
        assert!(now >= last && now >= value, "{last} → {now}");
        last = now;
    }
}

/// Two frames in flight with their own readbacks: submitting frame 2 does not wait for frame 1.
pub fn readback_of_frame_n_does_not_block_frame_n_plus_1_submission(
    device: &dyn Device,
    _: &Harness<'_>,
) {
    let timeline = device.create_timeline(0).unwrap();
    let frames: Vec<(Texture, Buffer, [f32; 4])> = [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]]
        .into_iter()
        .map(|rgba| {
            let t = target(device, SIZE, SampleCount::X1);
            let b = buffer(
                device,
                u64::from(SIZE * SIZE * 4),
                BufferUsage::COPY_DST,
                MemoryLocation::Readback,
            );
            (t, b, rgba)
        })
        .collect();
    for (n, (texture, readback, rgba)) in frames.iter().enumerate() {
        let mut list = CommandList::new("frame");
        list.push(pass(*texture, clear(*rgba), None, None))
            .push(Command::EndRenderPass)
            .push(Command::CopyTextureToBuffer {
                src: *texture,
                mip: 0,
                dst: *readback,
                offset: 0,
                bytes_per_row: SIZE * 4,
            });
        let signal = [(timeline, n as u64 + 1)];
        device
            .submit(Submit {
                queue: QueueKind::Graphics,
                commands: &list,
                wait: &[],
                signal: &signal,
            })
            .unwrap();
    }
    for (n, (_, readback, rgba)) in frames.iter().enumerate() {
        assert!(device.wait_timeline(timeline, n as u64 + 1, WAIT).unwrap());
        let expected: Vec<u8> = rgba.iter().map(|c| (c * 255.0) as u8).collect();
        assert_eq!(
            device.read_buffer(*readback, 0, 4).unwrap(),
            expected,
            "frame {n}"
        );
    }
}

/// Broken shader source is a `PipelineCompile` error carrying the compiler log.
pub fn pipeline_creation_error_is_reported_not_panicked(device: &dyn Device, _: &Harness<'_>) {
    let broken = match device.caps().backend {
        crate::caps::Backend::Metal => ShaderSource::Msl {
            source: "kernel void broken( { this is not MSL".into(),
            entry: "broken".into(),
        },
        crate::caps::Backend::Vulkan => ShaderSource::SpirV {
            words: vec![0xdead_beef],
            entry: "broken".into(),
        },
    };
    let err = device
        .create_compute_pipeline(&ComputePipelineDesc {
            shader: broken,
            threads_per_group: [1, 1, 1],
            label: "broken".into(),
        })
        .unwrap_err();
    match err {
        RhiError::PipelineCompile { log, label } => {
            assert_eq!(label, "broken");
            assert!(!log.trim().is_empty());
        }
        other => panic!("expected PipelineCompile, got {other}"),
    }
}

/// Invalid descriptors, stale handles and invalid command lists are errors, never panics.
pub fn invalid_descriptors_and_stale_handles_are_errors(device: &dyn Device, _: &Harness<'_>) {
    let mut bad = texture_desc(
        8,
        Format::Rgba8Unorm,
        SampleCount::X1,
        TextureUsage::SAMPLED,
    );
    bad.mip_levels = std::num::NonZeroU32::new(9).unwrap();
    assert!(matches!(
        device.create_texture(&bad),
        Err(RhiError::Invalid { .. })
    ));
    let b = buffer(device, 16, BufferUsage::STORAGE, MemoryLocation::GpuOnly);
    assert!(device.buffer_index(b).is_err(), "not created with BINDLESS");
    device.destroy_buffer(b);
    assert!(matches!(
        device.read_buffer(b, 0, 4),
        Err(RhiError::StaleHandle { .. })
    ));
    let heap = device
        .create_heap(&HeapDesc {
            size: std::num::NonZeroU64::new(1 << 16).unwrap(),
            label: "destroyed".into(),
        })
        .unwrap();
    device.destroy_heap(heap);
    let placed = texture_desc(
        8,
        Format::Rgba8Unorm,
        SampleCount::X1,
        TextureUsage::SAMPLED,
    );
    assert!(matches!(
        device.create_placed_texture(heap, 0, &placed),
        Err(RhiError::StaleHandle { .. })
    ));
    let mut list = CommandList::new("bad");
    list.push(Command::Draw {
        vertices: 0..3,
        instances: 0..1,
    });
    let submit = Submit {
        queue: QueueKind::Graphics,
        commands: &list,
        wait: &[],
        signal: &[],
    };
    assert!(matches!(
        device.submit(submit),
        Err(RhiError::Invalid { .. })
    ));
}

/// Uploads a 2×2 RGBA8 texture whose left column is black and right column white.
fn black_white_texture(device: &dyn Device) -> u32 {
    let usage = TextureUsage::SAMPLED | TextureUsage::COPY_DST | TextureUsage::BINDLESS;
    let texture = device
        .create_texture(&texture_desc(2, Format::Rgba8Unorm, SampleCount::X1, usage))
        .expect("create texture");
    let texels = [[0u8, 0, 0, 255], [255, 255, 255, 255]].repeat(2).concat();
    let staging = buffer(
        device,
        texels.len() as u64,
        BufferUsage::COPY_SRC,
        MemoryLocation::Upload,
    );
    device
        .write_buffer(staging, 0, &texels)
        .expect("write staging");
    let mut list = CommandList::new("upload texture");
    list.push(Command::CopyBufferToTexture {
        src: staging,
        offset: 0,
        bytes_per_row: 8,
        dst: texture,
        mip: 0,
    });
    run(device, &list);
    device.texture_index(texture).expect("bindless index").0
}

/// A bindless texture slot that was never written, or whose texture was destroyed, reads as zero
/// (black, alpha 0 or 1) instead of faulting: Metal's nil entries, Vulkan's null descriptors.
pub fn unwritten_and_freed_texture_slots_read_zero(device: &dyn Device, h: &Harness<'_>) {
    let usage = TextureUsage::SAMPLED | TextureUsage::COPY_DST | TextureUsage::BINDLESS;
    let texture = device
        .create_texture(&texture_desc(2, Format::Rgba8Unorm, SampleCount::X1, usage))
        .expect("create texture");
    let freed = device.texture_index(texture).expect("bindless index").0;
    device.destroy_texture(texture);
    device.wait_idle().expect("wait idle");
    let unwritten = device.caps().max_bindless_textures.min(16_384) - 1;
    let reader = graphics(
        device,
        h,
        "sample_texture",
        ("fullscreen", "sample_texture"),
        SampleCount::X1,
        None,
    );
    for (name, index) in [("freed", freed), ("never written", unwritten)] {
        let color = target(device, SIZE, SampleCount::X1);
        let mut list = CommandList::new("read an empty slot");
        list.push(pass(color, clear([1.0, 0.0, 1.0, 1.0]), None, None))
            .push(Command::SetPipeline(reader))
            .push(push(&[index, 0, 0, 0], &[0.0; 4]))
            .push(Command::Draw {
                vertices: 0..3,
                instances: 0..1,
            })
            .push(Command::EndRenderPass);
        run(device, &list);
        let got = pixel(&read_texture(device, color, SIZE), 100, 100);
        assert!(
            got[..3] == [0, 0, 0] && (got[3] == 0 || got[3] == 255),
            "{name} slot {index} read {got:?}"
        );
    }
}

/// Every static sampler filters and addresses as named (and buffer-to-texture uploads land).
pub fn static_samplers_filter_and_address_as_named(device: &dyn Device, h: &Harness<'_>) {
    let source = black_white_texture(device);
    let pipeline = graphics(
        device,
        h,
        "sample_texture",
        ("fullscreen", "sample_with_sampler"),
        SampleCount::X1,
        None,
    );
    let color = target(device, SIZE, SampleCount::X1);
    let sample = |sampler: StaticSampler, u: f32| {
        let mut list = CommandList::new("sample");
        list.push(pass(color, LoadOp::DontCare, None, None))
            .push(Command::SetPipeline(pipeline))
            .push(push(&[source, sampler.index()], &[u, 0.5]))
            .push(Command::Draw {
                vertices: 0..3,
                instances: 0..1,
            })
            .push(Command::EndRenderPass);
        run(device, &list);
        pixel(&read_texture(device, color, SIZE), 10, 10)[0]
    };
    let near = |got: u8, want: u8| got.abs_diff(want) <= 2;
    assert_eq!(
        sample(StaticSampler::NearestClamp, 0.3),
        0,
        "nearest picks the left texel"
    );
    assert_eq!(
        sample(StaticSampler::NearestClamp, 0.7),
        255,
        "nearest picks the right texel"
    );
    assert_eq!(
        sample(StaticSampler::NearestClamp, 1.3),
        255,
        "clamp holds the edge"
    );
    assert_eq!(
        sample(StaticSampler::NearestRepeat, 1.3),
        0,
        "repeat wraps 1.3 to 0.3"
    );
    let mid = sample(StaticSampler::LinearClamp, 0.5);
    assert!(near(mid, 128), "linear blends the two texels: {mid}");
    assert_eq!(
        sample(StaticSampler::LinearClamp, 0.0),
        0,
        "linear clamp at the edge"
    );
    let wrapped = sample(StaticSampler::LinearRepeat, 0.0);
    assert!(
        near(wrapped, 128),
        "linear repeat blends across the wrap: {wrapped}"
    );
}

/// Additive blending adds each draw to the target: a clear plus two equal draws is their sum.
pub fn additive_blend_sums_overlapping_draws(device: &dyn Device, h: &Harness<'_>) {
    let pipeline = graphics_blended(
        device,
        h,
        "triangle",
        ("vs", "fs"),
        (SampleCount::X1, None),
        BlendMode::Additive,
    );
    let color = target(device, SIZE, SampleCount::X1);
    let vb = upload(device, &vertices(&triangle(0.5, [0.2; 4])));
    let mut list = CommandList::new("additive");
    list.push(pass(color, clear([0.1, 0.2, 0.3, 0.4]), None, None))
        .push(Command::SetPipeline(pipeline));
    for _ in 0..2 {
        list.push(push(
            &[device.buffer_index(vb).unwrap().0, 0, 0, 0],
            &[1.0; 4],
        ))
        .push(Command::Draw {
            vertices: 0..3,
            instances: 0..1,
        });
    }
    list.push(Command::EndRenderPass);
    run(device, &list);
    let got = pixel(&read_texture(device, color, SIZE), SIZE / 2, SIZE / 2);
    for (g, want) in got.iter().zip([0.5f32, 0.6, 0.7, 0.8]) {
        let want = (want * 255.0).round() as u8;
        assert!(g.abs_diff(want) <= 1, "{got:?}: channel {g} vs {want}");
    }
}

/// `submit_timed` calls back exactly once after the work: a finite, non-negative total in ms, or
/// `None` when the backend cannot measure it.
pub fn submit_timed_reports_the_gpu_time_once(device: &dyn Device, _: &Harness<'_>) {
    let color = target(device, SIZE, SampleCount::X1);
    let mut list = CommandList::new("timed");
    list.push(pass(color, clear([0.0, 0.0, 0.0, 1.0]), None, None))
        .push(Command::EndRenderPass);
    let (tx, rx) = std::sync::mpsc::channel();
    device
        .submit_timed(
            Submit {
                queue: QueueKind::Graphics,
                commands: &list,
                wait: &[],
                signal: &[],
            },
            Box::new(move |timing| tx.send(timing.total_ms).expect("one report")),
        )
        .expect("submit");
    device.wait_idle().expect("wait idle");
    let ms = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the GPU time is reported");
    assert!(ms.is_none_or(|ms| ms.is_finite() && ms >= 0.0), "{ms:?}");
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(100))
            .is_err(),
        "reported once"
    );
}

/// Timer scopes around a render pass, a dispatch and a copy each report once, in command order,
/// when the device times passes; otherwise none are reported.
pub fn timer_scopes_report_each_scope_once_in_order(device: &dyn Device, h: &Harness<'_>) {
    const N: u32 = 65_536;
    let color = target(device, SIZE, SampleCount::X1);
    let out = upload(device, &vec![0u8; N as usize * 4]);
    let copy = upload(device, &vec![0u8; N as usize * 4]);
    let pipeline = compute(device, h, "fill");
    let mut list = CommandList::new("timed scopes");
    list.push(Command::BeginTimer(TimerScope(7)))
        .push(pass(color, clear([0.2, 0.3, 0.4, 1.0]), None, None))
        .push(Command::EndRenderPass)
        .push(Command::EndTimer)
        .push(Command::BeginTimer(TimerScope(3)))
        .push(Command::SetPipeline(pipeline))
        .push(push(&[device.buffer_index(out).unwrap().0, N], &[]))
        .push(Command::Dispatch {
            groups: [N / 64, 1, 1],
        })
        .push(Command::EndTimer)
        .push(Command::BeginTimer(TimerScope(5)))
        .push(Command::CopyBuffer {
            src: out,
            src_offset: 0,
            dst: copy,
            dst_offset: 0,
            size: u64::from(N) * 4,
        })
        .push(Command::EndTimer);
    let (tx, rx) = std::sync::mpsc::channel();
    device
        .submit_timed(
            Submit {
                queue: QueueKind::Graphics,
                commands: &list,
                wait: &[],
                signal: &[],
            },
            Box::new(move |timing| tx.send(timing).expect("one report")),
        )
        .expect("submit");
    device.wait_idle().expect("wait idle");
    let timing = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the timing is reported");
    let got = words(&download(device, copy, 16));
    assert_eq!(got, [1, 4, 7, 10], "timed work still runs");
    if !device.caps().timestamp_granularity.times_passes() {
        assert!(timing.scopes.is_empty(), "{timing:?}");
        return;
    }
    let scopes: Vec<u32> = timing.scopes.iter().map(|s| s.scope.0).collect();
    assert_eq!(scopes, [7, 3, 5], "each scope once, in command order");
    for s in &timing.scopes {
        assert!(s.start_ms.is_finite() && s.start_ms >= 0.0, "{s:?}");
        assert!(s.end_ms.is_finite() && s.end_ms >= s.start_ms, "{s:?}");
    }
    assert!(
        timing
            .scopes
            .windows(2)
            .all(|w| w[0].start_ms <= w[1].start_ms),
        "scopes start in order: {timing:?}"
    );
    assert!(
        timing.scopes[1].ms() > 0.0,
        "a 64k-thread dispatch takes measurable time: {timing:?}"
    );
}

/// Clip-space Y points up and `CullMode::Back` keeps counter-clockwise triangles: a CCW triangle
/// high in clip space lands in the image's top half; a CW one low in clip space is culled.
pub fn clip_space_y_is_up_and_back_faces_cull(device: &dyn Device, h: &Harness<'_>) {
    use super::ShaderStage;
    use crate::pipeline::{CullMode, GraphicsPipelineDesc};
    let red = [1.0, 0.0, 0.0, 1.0];
    let green = [0.0, 1.0, 0.0, 1.0];
    let data = vertices(&[
        ([-0.5, 0.2, 0.5, 1.0], red),
        ([0.5, 0.2, 0.5, 1.0], red),
        ([0.0, 0.9, 0.5, 1.0], red),
        ([-0.5, -0.2, 0.5, 1.0], green),
        ([0.5, -0.2, 0.5, 1.0], green),
        ([0.0, -0.9, 0.5, 1.0], green),
    ]);
    let pipeline = device
        .create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: h.shaders.shader("triangle", "vs", ShaderStage::Vertex).0,
            fragment: Some(h.shaders.shader("triangle", "fs", ShaderStage::Fragment).0),
            color_formats: vec![Format::Rgba8Unorm],
            depth: None,
            samples: SampleCount::X1,
            cull: CullMode::Back,
            blend: BlendMode::Opaque,
            color_writes: crate::pipeline::ColorWrites::ALL,
            label: "culled triangles".into(),
        })
        .expect("graphics pipeline");
    let color = target(device, SIZE, SampleCount::X1);
    let vb = upload(device, &data);
    let index = device.buffer_index(vb).expect("bindless index").0;
    let mut list = CommandList::new("culling");
    list.push(pass(color, clear([0.0, 0.0, 0.0, 1.0]), None, None))
        .push(Command::SetPipeline(pipeline))
        .push(push(&[index, 0, 0, 0], &[1.0, 1.0, 1.0, 1.0]))
        .push(Command::Draw {
            vertices: 0..6,
            instances: 0..1,
        })
        .push(Command::EndRenderPass);
    run(device, &list);
    let image = read_texture(device, color, SIZE);
    assert_eq!(
        pixel(&image, SIZE / 2, SIZE / 4),
        [255, 0, 0, 255],
        "CCW triangle at the top"
    );
    assert_eq!(
        pixel(&image, SIZE / 2, SIZE * 3 / 4),
        [0, 0, 0, 255],
        "CW triangle culled"
    );
}
