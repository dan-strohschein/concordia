//! Conformance cases for viewports, scissors and texture-to-texture copies (M4 4.3a, D5).

use std::num::NonZeroU32;

use crate::caps::SampleCount;
use crate::command::{Command, CommandList, LoadOp, Rect, TextureOrigin, Viewport};
use crate::device::{Device, QueueKind, Submit};
use crate::format::Format;
use crate::handle::Texture;
use crate::resource::{BufferUsage, MemoryLocation, TextureDesc, TextureUsage};

use super::Harness;
use super::cases::{SIZE, clear, pass, pixel, vertices};
use super::util::{buffer, graphics, push, read_texture, run, target, upload};

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const LIT: [u8; 4] = [255, 0, 0, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

fn nz(v: u32) -> NonZeroU32 {
    NonZeroU32::new(v).expect("non-zero")
}

/// A triangle covering all of clip space (y ≥ `bottom` when given a half-space cut).
fn covering(bottom: f32) -> Vec<u8> {
    vertices(&[
        ([-1.0, bottom, 0.5, 1.0], RED),
        ([3.0, bottom, 0.5, 1.0], RED),
        ([-1.0, 3.0, 0.5, 1.0], RED),
    ])
}

fn viewport(x: f32, y: f32, width: f32, height: f32) -> Viewport {
    Viewport {
        x,
        y,
        width,
        height,
        min_depth: 0.0,
        max_depth: 1.0,
    }
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width: nz(width),
        height: nz(height),
    }
}

/// The commands drawing `data` with the triangle pipeline inside a pass on `color`.
fn draw_into(
    device: &dyn Device,
    h: &Harness<'_>,
    color: Texture,
    load: LoadOp,
    data: &[u8],
    state: &[Command],
) -> CommandList {
    let pipeline = graphics(device, h, "triangle", ("vs", "fs"), SampleCount::X1, None);
    let vb = upload(device, data);
    let index = device.buffer_index(vb).expect("bindless index").0;
    let mut list = CommandList::new("regions");
    list.push(pass(color, load, None, None))
        .push(Command::SetPipeline(pipeline));
    state.iter().for_each(|c| {
        list.push(c.clone());
    });
    list.push(push(&[index, 0, 0, 0], &[1.0, 1.0, 1.0, 1.0]))
        .push(Command::Draw {
            vertices: 0..3,
            instances: 0..1,
        })
        .push(Command::EndRenderPass);
    list
}

fn render(device: &dyn Device, h: &Harness<'_>, data: &[u8], state: &[Command]) -> Vec<u8> {
    let color = target(device, SIZE, SampleCount::X1);
    let load = clear([0.0, 0.0, 0.0, 1.0]);
    run(device, &draw_into(device, h, color, load, data, state));
    read_texture(device, color, SIZE)
}

/// Clip space maps into the viewport's rectangle (y down in pixels, y up in clip space).
pub fn viewport_maps_clip_space_into_its_rectangle(device: &dyn Device, h: &Harness<'_>) {
    let q = SIZE as f32 / 2.0;
    let state = [Command::SetViewport(viewport(0.0, 0.0, q, q))];
    let image = render(device, h, &covering(0.0), &state);
    let (s4, s8) = (SIZE / 4, SIZE / 8);
    assert_eq!(
        pixel(&image, s4, s8),
        LIT,
        "upper half of the top-left quadrant"
    );
    assert_eq!(
        pixel(&image, s4, s4 + s8),
        BLACK,
        "lower half of the quadrant: clip y < 0"
    );
    assert_eq!(
        pixel(&image, SIZE / 2 + s4, s8),
        BLACK,
        "outside the viewport"
    );
    assert_eq!(
        pixel(&image, s4, SIZE / 2 + s8),
        BLACK,
        "outside the viewport"
    );
}

/// Fragments outside the scissor rectangle are discarded.
pub fn scissor_discards_outside_its_rectangle(device: &dyn Device, h: &Harness<'_>) {
    let half = SIZE / 2;
    let state = [Command::SetScissor(rect(half, 0, half, SIZE))];
    let image = render(device, h, &covering(-1.0), &state);
    assert_eq!(
        pixel(&image, SIZE / 4, SIZE / 2),
        BLACK,
        "left of the scissor"
    );
    assert_eq!(
        pixel(&image, SIZE * 3 / 4, SIZE / 2),
        LIT,
        "inside the scissor"
    );
}

/// A new pass draws to its whole target whatever the previous pass set.
pub fn each_pass_starts_with_the_full_viewport(device: &dyn Device, h: &Harness<'_>) {
    let color = target(device, SIZE, SampleCount::X1);
    let narrowed = [
        Command::SetViewport(viewport(0.0, 0.0, 8.0, 8.0)),
        Command::SetScissor(rect(0, 0, 8, 8)),
    ];
    let mut list = draw_into(
        device,
        h,
        color,
        clear([0.0, 0.0, 0.0, 1.0]),
        &covering(-1.0),
        &narrowed,
    );
    let second = draw_into(
        device,
        h,
        color,
        clear([0.0, 0.0, 0.0, 1.0]),
        &covering(-1.0),
        &[],
    );
    list.commands.extend(second.commands);
    run(device, &list);
    let image = read_texture(device, color, SIZE);
    for (x, y) in [(SIZE - 1, SIZE - 1), (SIZE / 2, SIZE / 2), (1, SIZE - 2)] {
        assert_eq!(pixel(&image, x, y), LIT, "({x}, {y})");
    }
}

/// A viewport or scissor reaching outside the pass's targets is an error.
pub fn viewport_and_scissor_outside_the_target_are_errors(device: &dyn Device, h: &Harness<'_>) {
    let color = target(device, SIZE, SampleCount::X1);
    let s = SIZE as f32;
    let bad = [
        Command::SetViewport(viewport(1.0, 0.0, s, s)),
        Command::SetScissor(rect(0, 1, SIZE, SIZE)),
    ];
    for state in bad {
        let list = draw_into(
            device,
            h,
            color,
            clear([0.0; 4]),
            &covering(-1.0),
            std::slice::from_ref(&state),
        );
        assert!(submit(device, &list).is_err(), "{state:?} was accepted");
    }
    device.wait_idle().expect("wait idle");
}

fn submit(device: &dyn Device, list: &CommandList) -> Result<(), crate::RhiError> {
    device.submit(Submit {
        queue: QueueKind::Graphics,
        commands: list,
        wait: &[],
        signal: &[],
    })
}

fn texture(device: &dyn Device, size: u32, mips: u32, format: Format) -> Texture {
    device
        .create_texture(&TextureDesc {
            width: nz(size),
            height: nz(size),
            mip_levels: nz(mips),
            samples: SampleCount::X1,
            format,
            usage: TextureUsage::COPY_SRC | TextureUsage::COPY_DST,
            label: "copy".into(),
        })
        .expect("create texture")
}

/// RGBA8 texels of a `size`² mip: (x, y, mip tag, 255).
fn texels(size: u32, tag: u8) -> Vec<u8> {
    (0..size * size)
        .flat_map(|i| [(i % size) as u8, (i / size) as u8, tag, 255])
        .collect()
}

/// Fills every mip of an RGBA8 texture with [`texels`] (tag = mip + 1).
fn fill(device: &dyn Device, texture: Texture, size: u32, mips: u32) {
    let mut list = CommandList::new("fill");
    for mip in 0..mips {
        let side = size >> mip;
        let data = texels(side, mip as u8 + 1);
        let staging = buffer(
            device,
            data.len() as u64,
            BufferUsage::COPY_SRC,
            MemoryLocation::Upload,
        );
        device.write_buffer(staging, 0, &data).expect("write");
        list.push(Command::CopyBufferToTexture {
            src: staging,
            offset: 0,
            bytes_per_row: side * 4,
            dst: texture,
            mip,
        });
    }
    run(device, &list);
}

fn read_mip0(device: &dyn Device, texture: Texture, size: u32) -> Vec<u8> {
    let bytes = u64::from(size * size * 4);
    let readback = buffer(
        device,
        bytes,
        BufferUsage::COPY_DST,
        MemoryLocation::Readback,
    );
    let mut list = CommandList::new("read");
    list.push(Command::CopyTextureToBuffer {
        src: texture,
        mip: 0,
        dst: readback,
        offset: 0,
        bytes_per_row: size * 4,
    });
    run(device, &list);
    device.read_buffer(readback, 0, bytes).expect("read")
}

fn copy(src: Texture, s: TextureOrigin, dst: Texture, d: TextureOrigin, w: u32, h: u32) -> Command {
    Command::CopyTexture {
        src,
        src_origin: s,
        dst,
        dst_origin: d,
        extent: [nz(w), nz(h)],
    }
}

fn at(mip: u32, x: u32, y: u32) -> TextureOrigin {
    TextureOrigin { mip, x, y }
}

/// Regions copy texel for texel from any mip to any position of another texture.
pub fn copy_texture_copies_a_region_between_mips_and_textures(
    device: &dyn Device,
    _: &Harness<'_>,
) {
    let src = texture(device, 16, 2, Format::Rgba8Unorm);
    fill(device, src, 16, 2);
    let dst = texture(device, 8, 1, Format::Rgba8Unorm);
    fill(device, dst, 8, 1);
    let mut list = CommandList::new("copies");
    list.push(copy(src, at(0, 10, 12), dst, at(0, 0, 0), 4, 2))
        .push(copy(src, at(1, 1, 2), dst, at(0, 5, 5), 3, 3));
    run(device, &list);
    let image = read_mip0(device, dst, 8);
    let texel = |x: u32, y: u32| {
        let i = ((y * 8 + x) * 4) as usize;
        [image[i], image[i + 1], image[i + 2], image[i + 3]]
    };
    assert_eq!(texel(0, 0), [10, 12, 1, 255], "mip 0 region's corner");
    assert_eq!(texel(3, 1), [13, 13, 1, 255], "mip 0 region's far corner");
    assert_eq!(texel(4, 0), [4, 0, 1, 255], "untouched beside the region");
    assert_eq!(texel(5, 5), [1, 2, 2, 255], "mip 1 region's corner");
    assert_eq!(texel(7, 7), [3, 4, 2, 255], "mip 1 region's far corner");
    assert_eq!(texel(4, 5), [4, 5, 1, 255], "untouched beside the region");
}

/// Mismatched formats, regions past a mip's edge and freed textures are errors.
pub fn copy_texture_rejects_mismatched_formats_and_out_of_range_regions(
    device: &dyn Device,
    _: &Harness<'_>,
) {
    let src = texture(device, 16, 2, Format::Rgba8Unorm);
    let dst = texture(device, 8, 1, Format::Rgba8Unorm);
    let srgb = texture(device, 8, 1, Format::Rgba8UnormSrgb);
    let freed = texture(device, 8, 1, Format::Rgba8Unorm);
    device.destroy_texture(freed);
    let bad = [
        copy(src, at(0, 0, 0), srgb, at(0, 0, 0), 4, 4),
        copy(src, at(0, 14, 0), dst, at(0, 0, 0), 4, 4),
        copy(src, at(1, 0, 0), dst, at(0, 0, 0), 9, 1),
        copy(src, at(0, 0, 0), dst, at(0, 6, 6), 4, 4),
        copy(src, at(2, 0, 0), dst, at(0, 0, 0), 1, 1),
        copy(src, at(0, 0, 0), freed, at(0, 0, 0), 4, 4),
    ];
    for command in bad {
        let mut list = CommandList::new("bad copy");
        list.push(command.clone());
        assert!(submit(device, &list).is_err(), "{command:?} was accepted");
    }
    device.wait_idle().expect("wait idle");
}
