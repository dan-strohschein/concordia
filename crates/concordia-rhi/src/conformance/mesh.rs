//! Conformance cases for mesh pipelines and mesh-task draws (M4 4.3b). A device without mesh
//! shaders (MoltenVK, a masked device) must refuse mesh pipelines; the drawing cases then end.

use crate::caps::SampleCount;
use crate::command::{ClearValue, Command, CommandList, DepthAttachment, LoadOp, StoreOp};
use crate::device::{Device, QueueKind, Submit};
use crate::error::RhiError;
use crate::format::Format;
use crate::handle::Pipeline;
use crate::pipeline::{
    BlendMode, CompareOp, CullMode, DepthState, MeshPipelineDesc, MeshStage, ObjectStage,
};
use crate::resource::TextureUsage;

use super::cases::{SIZE, clear, pass, pixel, vertices};
use super::util::{graphics, push, read_texture, run, target, texture_desc, upload};
use super::{Harness, ShaderStage};

const BLACK: [u8; 4] = [0, 0, 0, 255];
/// `ms_half`'s colour.
const HALF: [u8; 4] = [200, 0, 0, 255];
/// Entries `object_main` spreads over two object groups (32 + 4).
const ENTRIES: u32 = 36;

fn depth_state() -> DepthState {
    DepthState {
        format: Format::Depth32Float,
        compare: CompareOp::GreaterEqual,
        write: true,
    }
}

/// `mesh.slang`'s pipeline: `ms_half` alone, or `object_main` + `ms_column`.
fn desc(h: &Harness<'_>, object: bool, depth: Option<DepthState>) -> MeshPipelineDesc {
    let (mesh_entry, vertices, primitives) = match object {
        true => ("ms_column", 4, 2),
        false => ("ms_half", 128, 128),
    };
    let (shader, threads) = h.shaders.shader("mesh", mesh_entry, ShaderStage::Mesh);
    let object = object.then(|| {
        let (shader, threads) = h.shaders.shader("mesh", "object_main", ShaderStage::Object);
        ObjectStage {
            shader,
            threads_per_group: threads.expect("object stages report their thread group size"),
            payload_bytes: 32 * 8,
        }
    });
    MeshPipelineDesc {
        object,
        mesh: MeshStage {
            shader,
            threads_per_group: threads.expect("mesh stages report their thread group size"),
            max_vertices: vertices,
            max_primitives: primitives,
        },
        fragment: Some(h.shaders.shader("mesh", "fs", ShaderStage::Fragment).0),
        color_formats: vec![Format::Rgba8Unorm],
        depth,
        samples: SampleCount::X1,
        cull: CullMode::None,
        blend: BlendMode::Opaque,
        label: "mesh".into(),
    }
}

/// The pipeline, or `None` after checking that a device without mesh shaders refuses it.
fn mesh_pipeline(
    device: &dyn Device,
    h: &Harness<'_>,
    object: bool,
    depth: Option<DepthState>,
) -> Option<Pipeline> {
    let result = device.create_mesh_pipeline(&desc(h, object, depth));
    match device.caps().mesh_shaders {
        true => Some(result.expect("mesh pipeline")),
        false => {
            assert!(
                matches!(result, Err(RhiError::Unsupported(_))),
                "a device without mesh shaders must refuse mesh pipelines: {result:?}"
            );
            None
        }
    }
}

fn mesh_push(values: u32, count: u32, depth: f32) -> Command {
    push(&[values, count], &[depth, 0.0])
}

/// Draws `commands` inside one pass over a cleared target (and depth, if given); returns the image.
fn render(device: &dyn Device, depth: bool, commands: &[Command]) -> Vec<u8> {
    let color = target(device, SIZE, SampleCount::X1);
    let depth = depth.then(|| DepthAttachment {
        texture: device
            .create_texture(&texture_desc(
                SIZE,
                Format::Depth32Float,
                SampleCount::X1,
                TextureUsage::RENDER_TARGET,
            ))
            .expect("depth texture"),
        load: LoadOp::Clear(ClearValue::Depth(0.0)),
        store: StoreOp::DontCare,
    });
    let mut list = CommandList::new("mesh");
    list.push(pass(color, clear([0.0, 0.0, 0.0, 1.0]), None, depth));
    commands.iter().for_each(|c| {
        list.push(c.clone());
    });
    list.push(Command::EndRenderPass);
    run(device, &list);
    read_texture(device, color, SIZE)
}

/// A mesh-only pipeline draws what its mesh stage emits.
pub fn mesh_stage_draws_without_an_object_stage(device: &dyn Device, h: &Harness<'_>) {
    let Some(pipeline) = mesh_pipeline(device, h, false, None) else {
        return;
    };
    let image = render(
        device,
        false,
        &[
            Command::SetPipeline(pipeline),
            mesh_push(0, 0, 0.5),
            Command::DrawMeshTasks { groups: [1, 1, 1] },
        ],
    );
    assert_eq!(pixel(&image, SIZE / 4, SIZE / 2), HALF, "the left half");
    assert_eq!(
        pixel(&image, SIZE * 3 / 4, SIZE / 2),
        BLACK,
        "the right half"
    );
}

/// An object stage's payload (an array filled from a bindless buffer) reaches each of the mesh
/// groups it launches, across object groups, and flat per-vertex values reach the fragment stage.
pub fn object_payload_reaches_every_mesh_group(device: &dyn Device, h: &Harness<'_>) {
    let Some(pipeline) = mesh_pipeline(device, h, true, None) else {
        return;
    };
    let values: Vec<u32> = (0..ENTRIES).map(|k| k * 7 + 3 + (k << 8)).collect();
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let buffer = upload(device, &bytes);
    let index = device.buffer_index(buffer).expect("bindless index").0;
    let image = render(
        device,
        false,
        &[
            Command::SetPipeline(pipeline),
            mesh_push(index, ENTRIES, 0.5),
            Command::DrawMeshTasks {
                groups: [ENTRIES.div_ceil(32), 1, 1],
            },
        ],
    );
    for (k, value) in values.iter().enumerate() {
        let want = [*value as u8, (*value >> 8) as u8, k as u8, 255];
        assert_eq!(pixel(&image, k as u32, SIZE / 2), want, "entry {k}");
    }
    assert_eq!(
        pixel(&image, ENTRIES, SIZE / 2),
        BLACK,
        "past the last entry"
    );
}

/// Output counts set below the declared maximum draw only those primitives.
pub fn mesh_output_counts_below_the_declared_maximum_draw_only_those(
    device: &dyn Device,
    h: &Harness<'_>,
) {
    let Some(pipeline) = mesh_pipeline(device, h, false, None) else {
        return;
    };
    let image = render(
        device,
        false,
        &[
            Command::SetPipeline(pipeline),
            mesh_push(0, 0, 0.5),
            Command::DrawMeshTasks { groups: [1, 1, 1] },
        ],
    );
    let drawn = (0..SIZE)
        .flat_map(|y| (0..SIZE).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(&image, x, y) != BLACK)
        .count();
    assert_eq!(
        drawn,
        (SIZE * SIZE / 2) as usize,
        "exactly two triangles' pixels"
    );
}

/// Mesh and vertex draws in one pass test against the same depth buffer, in either order.
pub fn mesh_and_vertex_draws_share_one_depth_buffer(device: &dyn Device, h: &Harness<'_>) {
    let Some(mesh) = mesh_pipeline(device, h, false, Some(depth_state())) else {
        return;
    };
    let triangle = graphics(
        device,
        h,
        "triangle",
        ("vs", "fs"),
        SampleCount::X1,
        Some(depth_state()),
    );
    let blue = [0.0, 0.0, 1.0, 1.0];
    let full = |z: f32| {
        vertices(&[
            ([-3.0, -1.0, z, 1.0], blue),
            ([3.0, -1.0, z, 1.0], blue),
            ([0.0, 3.0, z, 1.0], blue),
        ])
    };
    let draw_triangle = |z: f32| {
        let vb = upload(device, &full(z));
        [
            Command::SetPipeline(triangle),
            push(&[device.buffer_index(vb).unwrap().0, 0, 0, 0], &[1.0; 4]),
            Command::Draw {
                vertices: 0..3,
                instances: 0..1,
            },
        ]
    };
    let draw_mesh = |z: f32| {
        [
            Command::SetPipeline(mesh),
            mesh_push(0, 0, z),
            Command::DrawMeshTasks { groups: [1, 1, 1] },
        ]
    };
    let left = (SIZE / 4, SIZE / 2);
    let mesh_near: Vec<Command> = [draw_mesh(0.9), draw_triangle(0.1)].concat();
    let image = render(device, true, &mesh_near);
    assert_eq!(
        pixel(&image, left.0, left.1),
        HALF,
        "the nearer mesh, drawn first"
    );
    let triangle_near: Vec<Command> = [draw_triangle(0.9), draw_mesh(0.1)].concat();
    let image = render(device, true, &triangle_near);
    assert_eq!(
        pixel(&image, left.0, left.1),
        [0, 0, 255, 255],
        "the nearer triangle, drawn first"
    );
}

/// Outputs beyond the caps are refused at creation; a mesh draw with a vertex pipeline bound, and a
/// vertex draw with a mesh pipeline bound, are errors at submit.
pub fn mesh_pipeline_misuse_is_reported(device: &dyn Device, h: &Harness<'_>) {
    let mut over = desc(h, false, None);
    over.mesh.max_vertices = device.caps().max_mesh_output_vertices + 1;
    let result = device.create_mesh_pipeline(&over);
    assert!(result.is_err(), "outputs beyond the caps were accepted");
    let Some(mesh) = mesh_pipeline(device, h, false, None) else {
        return;
    };
    let triangle = graphics(device, h, "triangle", ("vs", "fs"), SampleCount::X1, None);
    let color = target(device, SIZE, SampleCount::X1);
    let draws = [
        (triangle, Command::DrawMeshTasks { groups: [1, 1, 1] }),
        (
            mesh,
            Command::Draw {
                vertices: 0..3,
                instances: 0..1,
            },
        ),
    ];
    for (pipeline, draw) in draws {
        let mut list = CommandList::new("mesh misuse");
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
