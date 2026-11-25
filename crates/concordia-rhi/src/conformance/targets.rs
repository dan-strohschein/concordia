//! Conformance cases for two-channel float targets and per-target write masks (M4 4.3b2).

use crate::caps::SampleCount;
use crate::command::{ColorAttachment, Command, CommandList, RenderPassDesc, StoreOp};
use crate::device::Device;
use crate::format::Format;
use crate::handle::Pipeline;
use crate::pipeline::{BlendMode, ColorWrites, CullMode, GraphicsPipelineDesc};
use crate::resource::TextureUsage;

use super::cases::{SIZE, clear, pixel};
use super::util::{push, read_texture, run, texture_desc};
use super::{Harness, ShaderStage};

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn texture(device: &dyn Device, format: Format) -> crate::handle::Texture {
    let usage = TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC;
    device
        .create_texture(&texture_desc(SIZE, format, SampleCount::X1, usage))
        .expect("target")
}

fn pipeline(
    device: &dyn Device,
    h: &Harness<'_>,
    fragment: &str,
    formats: Vec<Format>,
    color_writes: ColorWrites,
) -> Pipeline {
    device
        .create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: h.shaders.shader("targets", "vs", ShaderStage::Vertex).0,
            fragment: Some(
                h.shaders
                    .shader("targets", fragment, ShaderStage::Fragment)
                    .0,
            ),
            color_formats: formats,
            depth: None,
            samples: SampleCount::X1,
            cull: CullMode::None,
            blend: BlendMode::Opaque,
            color_writes,
            label: format!("targets {fragment}"),
        })
        .expect("pipeline")
}

/// One full-screen draw of `pipeline` into `targets`, each cleared to blue first.
fn draw(
    device: &dyn Device,
    pipeline: Pipeline,
    targets: &[crate::handle::Texture],
    first: [f32; 4],
) {
    let colors = targets
        .iter()
        .map(|t| ColorAttachment {
            texture: *t,
            resolve: None,
            load: clear(BLUE),
            store: StoreOp::Store,
        })
        .collect();
    let mut list = CommandList::new("targets");
    list.push(Command::BeginRenderPass(RenderPassDesc {
        colors,
        depth: None,
    }))
    .push(Command::SetPipeline(pipeline))
    .push(push(&[], &[first, RED].concat()))
    .push(Command::Draw {
        vertices: 0..3,
        instances: 0..1,
    })
    .push(Command::EndRenderPass);
    run(device, &list);
}

/// An `Rg16Float` target takes two half floats per pixel, read back bit for bit.
pub fn rg16_float_renders_and_reads_back(device: &dyn Device, h: &Harness<'_>) {
    let target = texture(device, Format::Rg16Float);
    let pipeline = pipeline(
        device,
        h,
        "fs_rg",
        vec![Format::Rg16Float],
        ColorWrites::ALL,
    );
    draw(device, pipeline, &[target], [0.5, -0.25, 0.0, 0.0]);
    let image = read_texture(device, target, SIZE);
    // 0.5 is 0x3800 and -0.25 is 0xB400 as halves, little-endian.
    assert_eq!(pixel(&image, SIZE / 2, SIZE / 2), [0x00, 0x38, 0x00, 0xB4]);
    assert_eq!(pixel(&image, 3, SIZE - 3), [0x00, 0x38, 0x00, 0xB4]);
}

/// A pipeline that writes only target 0 leaves target 1 as the pass cleared it; one that writes
/// both changes both.
pub fn write_mask_leaves_a_target_untouched(device: &dyn Device, h: &Harness<'_>) {
    let formats = vec![Format::Rgba8Unorm, Format::Rgba8Unorm];
    let only_first = ColorWrites::only(&[0]).expect("target 0");
    for (writes, second) in [
        (only_first, [0, 0, 255, 255]),
        (ColorWrites::ALL, [255, 0, 0, 255]),
    ] {
        let targets = [
            texture(device, Format::Rgba8Unorm),
            texture(device, Format::Rgba8Unorm),
        ];
        let pipeline = pipeline(device, h, "fs_two", formats.clone(), writes);
        draw(device, pipeline, &targets, [0.0, 1.0, 0.0, 1.0]);
        let center = |t| pixel(&read_texture(device, t, SIZE), SIZE / 2, SIZE / 2);
        assert_eq!(center(targets[0]), [0, 255, 0, 255], "{writes:?}: target 0");
        assert_eq!(center(targets[1]), second, "{writes:?}: target 1");
    }
}
