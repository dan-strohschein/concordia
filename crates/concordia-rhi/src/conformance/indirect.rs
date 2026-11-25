//! Conformance cases for indirect draws (M4 4.3a, D5).

use crate::barrier::{Access, AccessFlags, BufferBarrier, Layout, Stages};
use crate::caps::SampleCount;
use crate::command::{Command, CommandList};
use crate::device::{Device, QueueKind, Submit};
use crate::handle::Pipeline;
use crate::resource::{BufferUsage, MemoryLocation};

use super::Harness;
use super::cases::{SIZE, clear, pass, pixel, vertices};
use super::util::{
    buffer, compute_entry, graphics, read_texture, run, target, upload, upload_with,
};

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

/// A small triangle left of center (instance 1 lands right of center with a step of 1).
fn left_triangle() -> Vec<u8> {
    vertices(&[
        ([-0.8, -0.4, 0.5, 1.0], RED),
        ([-0.2, -0.4, 0.5, 1.0], RED),
        ([-0.5, 0.4, 0.5, 1.0], RED),
    ])
}

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Push constants of `indirect.slang`: vertex buffer, instance step, then the compute fields.
fn push(vertices: u32, step: f32, args: u32, count: u32, instances: u32) -> Command {
    let mut bytes = vertices.to_le_bytes().to_vec();
    bytes.extend(step.to_le_bytes());
    bytes.extend(words(&[args, count, instances, 0, 0, 0]));
    Command::PushConstants(bytes)
}

fn pipeline(device: &dyn Device, h: &Harness<'_>) -> Pipeline {
    graphics(device, h, "indirect", ("vs", "fs"), SampleCount::X1, None)
}

/// Draws `draw` with the indirect pipeline over the left triangle; returns the image.
fn render(device: &dyn Device, h: &Harness<'_>, before: &[Command], draw: Command) -> Vec<u8> {
    let color = target(device, SIZE, SampleCount::X1);
    let vb = upload(device, &left_triangle());
    let vb_index = device.buffer_index(vb).expect("bindless index").0;
    let pipeline = pipeline(device, h);
    let mut list = CommandList::new("indirect");
    before.iter().for_each(|c| {
        list.push(c.clone());
    });
    list.push(pass(color, clear([0.0, 0.0, 0.0, 1.0]), None, None))
        .push(Command::SetPipeline(pipeline))
        .push(push(vb_index, 1.0, 0, 0, 0))
        .push(draw)
        .push(Command::EndRenderPass);
    run(device, &list);
    read_texture(device, color, SIZE)
}

const LEFT: (u32, u32) = (SIZE / 4, SIZE / 2);
const RIGHT: (u32, u32) = (SIZE * 3 / 4, SIZE / 2);
const BLACK: [u8; 4] = [0, 0, 0, 255];

fn at(image: &[u8], (x, y): (u32, u32)) -> [u8; 4] {
    pixel(image, x, y)
}

/// A draw's arguments come from a buffer at the given offset.
pub fn draw_indirect_reads_its_arguments_from_a_buffer(device: &dyn Device, h: &Harness<'_>) {
    let args = upload_with(
        device,
        &words(&[0, 0, 0, 0, 3, 1, 0, 0]),
        BufferUsage::INDIRECT,
    );
    let drawn = render(device, h, &[], Command::DrawIndirect { args, offset: 16 });
    assert_eq!(at(&drawn, LEFT), [255, 0, 0, 255], "the record at 16 draws");
    let empty = render(device, h, &[], Command::DrawIndirect { args, offset: 0 });
    assert_eq!(
        at(&empty, LEFT),
        BLACK,
        "the zero record at 0 draws nothing"
    );
}

/// An indexed draw's arguments come from a buffer, with the indices from the index buffer.
pub fn draw_indexed_indirect_reads_its_arguments_from_a_buffer(
    device: &dyn Device,
    h: &Harness<'_>,
) {
    let indices = upload_with(device, &words(&[2, 1, 0, 0]), BufferUsage::INDEX);
    let args = upload_with(device, &words(&[0, 3, 1, 0, 0, 0]), BufferUsage::INDIRECT);
    let draw = Command::DrawIndexedIndirect {
        indices,
        args,
        offset: 4,
    };
    let image = render(device, h, &[], draw);
    assert_eq!(at(&image, LEFT), [255, 0, 0, 255]);
    assert_eq!(at(&image, RIGHT), BLACK);
}

/// A compute pass writes the arguments; a barrier makes them visible to the indirect draw.
pub fn compute_written_arguments_drive_an_indirect_draw(device: &dyn Device, h: &Harness<'_>) {
    let args = buffer(
        device,
        16,
        BufferUsage::INDIRECT | BufferUsage::STORAGE | BufferUsage::BINDLESS,
        MemoryLocation::GpuOnly,
    );
    let args_index = device.buffer_index(args).expect("bindless index").0;
    let before = [
        Command::SetPipeline(compute_entry(device, h, "indirect", "write_args")),
        push(0, 0.0, args_index, 3, 2),
        Command::Dispatch { groups: [1, 1, 1] },
        Command::Barrier {
            buffers: vec![BufferBarrier {
                buffer: args,
                before: Access {
                    stages: Stages::COMPUTE,
                    access: AccessFlags::SHADER_WRITE,
                    layout: Layout::Undefined,
                },
                after: Access {
                    stages: Stages::DRAW_INDIRECT,
                    access: AccessFlags::INDIRECT_READ,
                    layout: Layout::Undefined,
                },
            }],
            textures: vec![],
        },
    ];
    let image = render(
        device,
        h,
        &before,
        Command::DrawIndirect { args, offset: 0 },
    );
    assert_eq!(at(&image, LEFT), [255, 0, 0, 255], "instance 0");
    assert_eq!(at(&image, RIGHT), [0, 255, 0, 255], "instance 1");
}

/// `SV_VertexID` and `SV_InstanceID` count from zero under indirect draws.
pub fn indirect_draws_see_vertex_and_instance_ids_from_zero(device: &dyn Device, h: &Harness<'_>) {
    let args = upload_with(device, &words(&[3, 2, 0, 0]), BufferUsage::INDIRECT);
    let image = render(device, h, &[], Command::DrawIndirect { args, offset: 0 });
    assert_eq!(
        at(&image, LEFT),
        [255, 0, 0, 255],
        "instance 0 reads vertices 0..3"
    );
    assert_eq!(
        at(&image, RIGHT),
        [0, 255, 0, 255],
        "instance 1 is one step right"
    );
}

/// Arguments past the buffer's end, a buffer without `INDIRECT` and a freed buffer are errors.
pub fn indirect_arguments_out_of_range_are_errors(device: &dyn Device, h: &Harness<'_>) {
    let color = target(device, SIZE, SampleCount::X1);
    let pipeline = pipeline(device, h);
    let indirect = upload_with(device, &words(&[3, 1, 0, 0]), BufferUsage::INDIRECT);
    let storage = upload(device, &words(&[3, 1, 0, 0]));
    let freed = upload_with(device, &words(&[3, 1, 0, 0]), BufferUsage::INDIRECT);
    device.destroy_buffer(freed);
    let cases = [
        Command::DrawIndirect {
            args: indirect,
            offset: 4,
        },
        Command::DrawIndexedIndirect {
            indices: indirect,
            args: indirect,
            offset: 0,
        },
        Command::DrawIndirect {
            args: storage,
            offset: 0,
        },
        Command::DrawIndirect {
            args: freed,
            offset: 0,
        },
    ];
    for draw in cases {
        let mut list = CommandList::new("bad indirect");
        list.push(pass(color, clear([0.0; 4]), None, None))
            .push(Command::SetPipeline(pipeline))
            .push(draw.clone())
            .push(Command::EndRenderPass);
        let result = device.submit(Submit {
            queue: QueueKind::Graphics,
            commands: &list,
            wait: &[],
            signal: &[],
        });
        assert!(result.is_err(), "{draw:?} was accepted");
    }
    device.wait_idle().expect("wait idle");
}
