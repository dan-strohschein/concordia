//! The device's temporal scaler (M4 4.6b, ADR-0103).

use std::num::NonZeroU32;

use super::Harness;
use super::util::{buffer, run, texture_desc, upload};
use crate::{
    BufferUsage, ClearValue, Command, CommandList, DepthAttachment, Device, Format, LoadOp,
    MemoryLocation, QueueKind, RenderPassDesc, RhiError, SampleCount, StoreOp, Submit,
    TemporalScale, TemporalScaler, TemporalScalerDesc, Texture, TextureUsage,
};

const INPUT: u32 = 32;
const OUTPUT: u32 = 64;

fn nz(x: u32) -> NonZeroU32 {
    NonZeroU32::new(x).expect("non-zero")
}

fn desc() -> TemporalScalerDesc {
    TemporalScalerDesc {
        input: [nz(INPUT), nz(INPUT)],
        output: [nz(OUTPUT), nz(OUTPUT)],
        color: Format::Rgba16Float,
        depth: Format::Depth32Float,
        motion: Format::Rg16Float,
        reactive: Some(Format::R8Unorm),
        output_format: Format::Rgba16Float,
    }
}

/// The inputs and output of one scale.
struct Frame {
    color: Texture,
    depth: Texture,
    motion: Texture,
    reactive: Texture,
    output: Texture,
}

fn make(device: &dyn Device, size: u32, format: Format, usage: TextureUsage) -> Texture {
    let read = TextureUsage::SAMPLED;
    device
        .create_texture(&texture_desc(size, format, SampleCount::X1, read | usage))
        .expect("create texture")
}

/// Uploads `texel` (bytes) into every texel of `texture`.
fn fill(device: &dyn Device, texture: Texture, size: u32, texel: &[u8]) {
    let data: Vec<u8> = (0..size * size).flat_map(|_| texel.to_vec()).collect();
    let src = upload(device, &data);
    let mut list = CommandList::new("fill");
    list.push(Command::CopyBufferToTexture {
        src,
        offset: 0,
        bytes_per_row: size * texel.len() as u32,
        dst: texture,
        mip: 0,
    });
    run(device, &list);
}

/// A flat color (0.5, 0.25, 0.125, 1), far depth, no motion and no reactive mask.
fn frame(device: &dyn Device) -> Frame {
    let copy = TextureUsage::COPY_DST;
    let f = Frame {
        color: make(device, INPUT, Format::Rgba16Float, copy),
        depth: make(
            device,
            INPUT,
            Format::Depth32Float,
            TextureUsage::RENDER_TARGET,
        ),
        motion: make(device, INPUT, Format::Rg16Float, copy),
        reactive: make(device, INPUT, Format::R8Unorm, copy),
        output: make(
            device,
            OUTPUT,
            Format::Rgba16Float,
            TextureUsage::STORAGE | TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC,
        ),
    };
    fill(
        device,
        f.color,
        INPUT,
        &[0x00, 0x38, 0x00, 0x34, 0x00, 0x30, 0x00, 0x3C],
    );
    fill(device, f.motion, INPUT, &[0; 4]);
    fill(device, f.reactive, INPUT, &[0]);
    let mut list = CommandList::new("clear depth");
    list.push(Command::BeginRenderPass(RenderPassDesc {
        colors: Vec::new(),
        depth: Some(DepthAttachment {
            texture: f.depth,
            load: LoadOp::Clear(ClearValue::Depth(0.0)),
            store: StoreOp::Store,
            resolve: None,
        }),
    }))
    .push(Command::EndRenderPass);
    run(device, &list);
    f
}

fn scale(scaler: TemporalScaler, f: &Frame, output: Texture, reset: bool) -> Command {
    Command::TemporalScale(TemporalScale {
        scaler,
        color: f.color,
        depth: f.depth,
        motion: f.motion,
        reactive: Some(f.reactive),
        output,
        jitter: [0.0, 0.0],
        motion_scale: [-(INPUT as f32), -(INPUT as f32)],
        pre_exposure: 1.0,
        reset,
    })
}

fn submit(device: &dyn Device, command: Command) -> Result<(), RhiError> {
    let mut list = CommandList::new("temporal scale");
    list.push(command);
    let result = device.submit(Submit {
        queue: QueueKind::Graphics,
        commands: &list,
        wait: &[],
        signal: &[],
    });
    device.wait_idle().expect("wait idle");
    result
}

fn half(bits: u16) -> f32 {
    let (sign, exp, frac) = (bits >> 15, (bits >> 10) & 0x1f, f32::from(bits & 0x3ff));
    let magnitude = match exp {
        0 => frac / 1024.0 * 2f32.powi(-14),
        31 => f32::INFINITY,
        e => (1.0 + frac / 1024.0) * 2f32.powi(i32::from(e) - 15),
    };
    if sign == 1 { -magnitude } else { magnitude }
}

/// The output's RGBA texels.
fn read_output(device: &dyn Device, output: Texture) -> Vec<[f32; 4]> {
    let bytes = u64::from(OUTPUT * OUTPUT * 8);
    let readback = buffer(
        device,
        bytes,
        BufferUsage::COPY_DST,
        MemoryLocation::Readback,
    );
    let mut list = CommandList::new("read output");
    list.push(Command::CopyTextureToBuffer {
        src: output,
        mip: 0,
        dst: readback,
        offset: 0,
        bytes_per_row: OUTPUT * 8,
    });
    run(device, &list);
    let data = device.read_buffer(readback, 0, bytes).expect("read buffer");
    data.chunks(8)
        .map(|t| std::array::from_fn(|c| half(u16::from_le_bytes([t[2 * c], t[2 * c + 1]]))))
        .collect()
}

/// A device with a temporal scaler upscales a flat frame to the same flat color, and refuses a
/// scale whose textures don't match it or whose scaler was destroyed; a device without one
/// refuses to create one and to encode a scale.
pub fn temporal_scaler_upscales_a_frame(device: &dyn Device, _: &Harness<'_>) {
    if !device.caps().metalfx_temporal {
        let made = device.create_temporal_scaler(&desc());
        assert!(matches!(made, Err(RhiError::Unsupported(_))), "{made:?}");
        let f = frame(device);
        let refused = submit(
            device,
            scale(TemporalScaler::from_raw(0, 1), &f, f.output, true),
        );
        assert!(refused.is_err(), "a scale without a scaler: {refused:?}");
        return;
    }
    let scaler = device.create_temporal_scaler(&desc()).expect("a scaler");
    let f = frame(device);
    for i in 0..4 {
        submit(device, scale(scaler, &f, f.output, i == 0)).expect("a scale");
    }
    let expected = [0.5, 0.25, 0.125];
    for (i, texel) in read_output(device, f.output).iter().enumerate() {
        let off = (0..3)
            .map(|c| (texel[c] - expected[c]).abs())
            .fold(0.0, f32::max);
        assert!(off < 0.02, "texel {i}: {texel:?}, expected {expected:?}");
    }
    let small = make(device, INPUT, Format::Rgba16Float, TextureUsage::STORAGE);
    let err = submit(device, scale(scaler, &f, small, false)).expect_err("a small output");
    assert!(err.to_string().contains("output"), "{err}");
    device.destroy_temporal_scaler(scaler);
    let err = submit(device, scale(scaler, &f, f.output, false)).expect_err("a destroyed scaler");
    assert!(matches!(err, RhiError::StaleHandle { .. }), "{err}");
}
