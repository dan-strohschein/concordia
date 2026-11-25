//! Helpers shared by conformance cases. Cases are tests, so they panic on failure.

use std::num::{NonZeroU32, NonZeroU64};
use std::time::Duration;

use crate::caps::SampleCount;
use crate::command::{Command, CommandList};
use crate::device::{Device, QueueKind, Submit};
use crate::format::Format;
use crate::handle::{Buffer, Pipeline, Texture};
use crate::pipeline::{BlendMode, ComputePipelineDesc, CullMode, DepthState, GraphicsPipelineDesc};
use crate::resource::{BufferDesc, BufferUsage, MemoryLocation, TextureDesc, TextureUsage};

use super::{Harness, ShaderStage};

pub(crate) fn buffer(
    device: &dyn Device,
    size: u64,
    usage: BufferUsage,
    memory: MemoryLocation,
) -> Buffer {
    let desc = BufferDesc {
        size: NonZeroU64::new(size).expect("non-zero size"),
        usage,
        memory,
        label: "conformance".into(),
    };
    device.create_buffer(&desc).expect("create buffer")
}

pub(crate) fn texture_desc(
    size: u32,
    format: Format,
    samples: SampleCount,
    usage: TextureUsage,
) -> TextureDesc {
    TextureDesc {
        width: NonZeroU32::new(size).expect("non-zero"),
        height: NonZeroU32::new(size).expect("non-zero"),
        mip_levels: NonZeroU32::MIN,
        samples,
        format,
        usage,
        label: "conformance".into(),
    }
}

pub(crate) fn target(device: &dyn Device, size: u32, samples: SampleCount) -> Texture {
    let usage = TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC;
    device
        .create_texture(&texture_desc(size, Format::Rgba8Unorm, samples, usage))
        .expect("create target")
}

/// Submits `commands` and waits until they finish.
pub(crate) fn run(device: &dyn Device, commands: &CommandList) {
    device
        .submit(Submit {
            queue: QueueKind::Graphics,
            commands,
            wait: &[],
            signal: &[],
        })
        .expect("submit");
    device.wait_idle().expect("wait idle");
}

/// Copies a single-sample RGBA8 texture to the CPU.
pub(crate) fn read_texture(device: &dyn Device, texture: Texture, size: u32) -> Vec<u8> {
    let bytes = u64::from(size) * u64::from(size) * 4;
    let readback = buffer(
        device,
        bytes,
        BufferUsage::COPY_DST,
        MemoryLocation::Readback,
    );
    let mut list = CommandList::new("readback");
    list.push(Command::CopyTextureToBuffer {
        src: texture,
        mip: 0,
        dst: readback,
        offset: 0,
        bytes_per_row: size * 4,
    });
    run(device, &list);
    device.read_buffer(readback, 0, bytes).expect("read buffer")
}

pub(crate) fn compute(device: &dyn Device, h: &Harness<'_>, name: &str) -> Pipeline {
    compute_entry(device, h, name, name)
}

/// The compute pipeline of `entry` in conformance shader `name`.
pub(crate) fn compute_entry(
    device: &dyn Device,
    h: &Harness<'_>,
    name: &str,
    entry: &str,
) -> Pipeline {
    let (shader, threads) = h.shaders.shader(name, entry, ShaderStage::Compute);
    let desc = ComputePipelineDesc {
        shader,
        threads_per_group: threads.expect("compute shaders report their thread group size"),
        label: name.into(),
    };
    device
        .create_compute_pipeline(&desc)
        .expect("compute pipeline")
}

pub(crate) fn graphics(
    device: &dyn Device,
    h: &Harness<'_>,
    name: &str,
    entries: (&str, &str),
    samples: SampleCount,
    depth: Option<DepthState>,
) -> Pipeline {
    graphics_blended(
        device,
        h,
        name,
        entries,
        (samples, depth),
        BlendMode::Opaque,
    )
}

/// [`graphics`] with a blend mode.
pub(crate) fn graphics_blended(
    device: &dyn Device,
    h: &Harness<'_>,
    name: &str,
    entries: (&str, &str),
    (samples, depth): (SampleCount, Option<DepthState>),
    blend: BlendMode,
) -> Pipeline {
    let desc = GraphicsPipelineDesc {
        vertex: h.shaders.shader(name, entries.0, ShaderStage::Vertex).0,
        fragment: Some(h.shaders.shader(name, entries.1, ShaderStage::Fragment).0),
        color_formats: vec![Format::Rgba8Unorm],
        depth,
        samples,
        cull: CullMode::None,
        blend,
        color_writes: crate::pipeline::ColorWrites::ALL,
        label: name.into(),
    };
    device
        .create_graphics_pipeline(&desc)
        .expect("graphics pipeline")
}

/// Push-constant bytes from u32 words followed by f32s.
pub(crate) fn push(words: &[u32], floats: &[f32]) -> Command {
    let mut bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    bytes.extend(floats.iter().flat_map(|f| f.to_le_bytes()));
    Command::PushConstants(bytes)
}

/// A bindless buffer filled with `data` through a staging upload.
pub(crate) fn upload(device: &dyn Device, data: &[u8]) -> Buffer {
    upload_with(device, data, BufferUsage::empty())
}

/// [`upload`] with `extra` usages (e.g. `INDIRECT`, `INDEX`).
pub(crate) fn upload_with(device: &dyn Device, data: &[u8], extra: BufferUsage) -> Buffer {
    let size = data.len() as u64;
    let staging = buffer(device, size, BufferUsage::COPY_SRC, MemoryLocation::Upload);
    device
        .write_buffer(staging, 0, data)
        .expect("write staging");
    let usage = BufferUsage::STORAGE
        | BufferUsage::BINDLESS
        | BufferUsage::COPY_DST
        | BufferUsage::COPY_SRC
        | extra;
    let gpu = buffer(device, size, usage, MemoryLocation::GpuOnly);
    let mut list = CommandList::new("upload");
    list.push(Command::CopyBuffer {
        src: staging,
        src_offset: 0,
        dst: gpu,
        dst_offset: 0,
        size,
    });
    run(device, &list);
    gpu
}

/// Reads back a GPU buffer through a staging copy.
pub(crate) fn download(device: &dyn Device, gpu: Buffer, size: u64) -> Vec<u8> {
    let readback = buffer(
        device,
        size,
        BufferUsage::COPY_DST,
        MemoryLocation::Readback,
    );
    let mut list = CommandList::new("download");
    list.push(Command::CopyBuffer {
        src: gpu,
        src_offset: 0,
        dst: readback,
        dst_offset: 0,
        size,
    });
    run(device, &list);
    device.read_buffer(readback, 0, size).expect("read back")
}

pub(crate) fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect()
}

pub(crate) const WAIT: Duration = Duration::from_secs(10);
