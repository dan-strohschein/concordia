//! Conformance cases for attachment-less passes (M4 4.7a).

use std::num::NonZeroU32;

use crate::caps::SampleCount;
use crate::command::{Command, CommandList, RenderPassDesc, Viewport};
use crate::device::Device;
use crate::pipeline::{BlendMode, ColorWrites, CullMode, GraphicsPipelineDesc};

use super::cases::{triangle, vertices};
use super::util::{download, push, run, upload};
use super::{Harness, ShaderStage};

/// Side of the written region in pixels.
const N: u32 = 64;
/// The virtual extent the case asks for (the sun's virtual shadow map, ADR-0091).
const EXTENT: u32 = 16_384;

/// Two triangles drawn far-then-near and near-then-far into an attachment-less pass of
/// [`EXTENT`]², in a viewport at its far corner: each fragment keeps the largest depth with atomic
/// max, so every covered pixel reads the near depth whatever the order. An extent beyond
/// `max_render_extent` is an error, not a crash.
pub fn attachmentless_pass_writes_storage_from_fragments(device: &dyn Device, h: &Harness<'_>) {
    let caps = device.caps();
    if !caps.fragment_atomics || caps.max_render_extent < EXTENT {
        return;
    }
    let desc = GraphicsPipelineDesc {
        vertex: h
            .shaders
            .shader("attachmentless", "vs", ShaderStage::Vertex)
            .0,
        fragment: Some(
            h.shaders
                .shader("attachmentless", "fs", ShaderStage::Fragment)
                .0,
        ),
        color_formats: vec![],
        depth: None,
        samples: SampleCount::X1,
        cull: CullMode::None,
        blend: BlendMode::Opaque,
        color_writes: ColorWrites::ALL,
        label: "attachmentless".into(),
    };
    let pipeline = device
        .create_graphics_pipeline(&desc)
        .expect("a pipeline without targets");
    let (near, far) = (0.75f32, 0.25f32);
    let mut points = triangle(far, [1.0; 4]);
    points.extend(triangle(near, [1.0; 4]));
    points.extend(triangle(far, [1.0; 4]));
    let vb = upload(device, &vertices(&points));
    let bytes = u64::from(N * N * 4);
    let out = upload(device, &vec![0u8; bytes as usize]);
    let origin = (EXTENT - N) as f32;
    let side = NonZeroU32::new(EXTENT).unwrap();
    let mut list = CommandList::new("attachmentless");
    list.push(Command::BeginRenderPass(RenderPassDesc {
        colors: vec![],
        depth: None,
        extent: Some([side, side]),
    }))
    .push(Command::SetPipeline(pipeline))
    .push(Command::SetViewport(Viewport {
        x: origin,
        y: origin,
        width: N as f32,
        height: N as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    }))
    .push(push(
        &[
            device.buffer_index(vb).unwrap().0,
            device.buffer_index(out).unwrap().0,
            N,
            0,
        ],
        &[origin, origin],
    ))
    .push(Command::Draw {
        vertices: 0..9,
        instances: 0..1,
    })
    .push(Command::EndRenderPass);
    run(device, &list);
    let got: Vec<f32> = download(device, out, bytes)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    let at = |x: u32, y: u32| got[(y * N + x) as usize];
    assert_eq!(at(N / 2, N / 2), near, "the centre keeps the near depth");
    assert_eq!(at(1, 1), 0.0, "outside the triangle stays cleared");
    assert!(
        got.iter().all(|&d| d == 0.0 || d == near),
        "only the near depth or the clear"
    );

    let mut too_big = CommandList::new("too big");
    let past = NonZeroU32::new(caps.max_render_extent.saturating_add(1)).unwrap();
    too_big
        .push(Command::BeginRenderPass(RenderPassDesc {
            colors: vec![],
            depth: None,
            extent: Some([past, side]),
        }))
        .push(Command::EndRenderPass);
    let err = device.submit(crate::device::Submit {
        queue: crate::device::QueueKind::Graphics,
        commands: &too_big,
        wait: &[],
        signal: &[],
    });
    assert!(err.is_err(), "an extent past max_render_extent is an error");
}
