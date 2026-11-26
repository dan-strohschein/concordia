//! Mesh pipelines and mesh-task draws (M4 4.3b): the shared descriptor checks against caps and
//! the portable limits, and command-list structure.

use concordia_rhi::{
    Backend, BlendMode, CapsMask, ClearValue, ColorAttachment, Command, CommandList, CullMode,
    Format, LoadOp, MAX_MESH_PAYLOAD_BYTES, MAX_MESH_STAGE_THREADS, MAX_MESH_TASK_GROUPS,
    MeshPipelineDesc, MeshStage, ObjectStage, RenderPassDesc, RhiCaps, RhiError, SampleCount,
    ShaderSource, StoreOp, Texture, TimestampGranularity,
};

fn caps() -> RhiCaps {
    RhiCaps {
        backend: Backend::Metal,
        device_name: "test".into(),
        unified_memory: true,
        msaa_sample_counts: vec![SampleCount::X1, SampleCount::X4],
        bindless: true,
        mesh_shaders: true,
        object_shaders: true,
        ray_tracing: false,
        hw_ray_tracing: false,
        metalfx_temporal: false,
        depth_resolve_min: false,
        timestamp_granularity: TimestampGranularity::StageBoundary,
        max_mesh_output_vertices: 256,
        max_mesh_output_primitives: 512,
        max_buffer_bytes: 1 << 30,
        max_bindless_textures: 16_384,
        max_bindless_buffers: 16_384,
        feature_sets: vec![],
    }
}

fn source() -> ShaderSource {
    ShaderSource::Msl {
        source: String::new(),
        entry: "main".into(),
    }
}

fn desc() -> MeshPipelineDesc {
    MeshPipelineDesc {
        object: Some(ObjectStage {
            shader: source(),
            threads_per_group: [32, 1, 1],
            payload_bytes: 256,
        }),
        mesh: MeshStage {
            shader: source(),
            threads_per_group: [128, 1, 1],
            max_vertices: 128,
            max_primitives: 128,
        },
        fragment: Some(source()),
        color_formats: vec![Format::Rgba16Float],
        depth: None,
        samples: SampleCount::X4,
        cull: CullMode::Back,
        blend: BlendMode::Opaque,
        color_writes: concordia_rhi::ColorWrites::ALL,
        label: "mesh".into(),
    }
}

fn invalid(result: Result<(), RhiError>, what: &str) {
    assert!(
        matches!(result, Err(RhiError::Invalid { .. })),
        "{what}: {result:?}"
    );
}

#[test]
fn a_mesh_pipeline_within_caps_is_valid() {
    assert_eq!(desc().validate(&caps()), Ok(()));
    let mut no_object = desc();
    no_object.object = None;
    assert_eq!(no_object.validate(&caps()), Ok(()));
}

#[test]
fn mesh_pipeline_desc_outputs_beyond_caps_are_errors() {
    let caps = caps();
    let mut d = desc();
    d.mesh.max_vertices = caps.max_mesh_output_vertices + 1;
    invalid(d.validate(&caps), "vertices over the cap");
    let mut d = desc();
    d.mesh.max_primitives = caps.max_mesh_output_primitives + 1;
    invalid(d.validate(&caps), "primitives over the cap");
    let mut d = desc();
    d.mesh.max_vertices = 0;
    invalid(d.validate(&caps), "no vertices");
    let mut d = desc();
    d.mesh.max_primitives = 0;
    invalid(d.validate(&caps), "no primitives");
    let mut d = desc();
    d.mesh.max_vertices = caps.max_mesh_output_vertices;
    d.mesh.max_primitives = caps.max_mesh_output_primitives;
    assert_eq!(d.validate(&caps), Ok(()), "exactly at the caps");
}

#[test]
fn mesh_stage_threads_and_payload_stay_within_portable_limits() {
    let caps = caps();
    for threads in [[0, 1, 1], [MAX_MESH_STAGE_THREADS + 1, 1, 1], [64, 2, 2]] {
        let mut d = desc();
        d.mesh.threads_per_group = threads;
        invalid(d.validate(&caps), &format!("mesh threads {threads:?}"));
        let mut d = desc();
        d.object.as_mut().unwrap().threads_per_group = threads;
        invalid(d.validate(&caps), &format!("object threads {threads:?}"));
    }
    for bytes in [0, MAX_MESH_PAYLOAD_BYTES + 1] {
        let mut d = desc();
        d.object.as_mut().unwrap().payload_bytes = bytes;
        invalid(d.validate(&caps), &format!("payload {bytes}"));
    }
    let mut d = desc();
    d.object.as_mut().unwrap().payload_bytes = MAX_MESH_PAYLOAD_BYTES;
    d.mesh.threads_per_group = [MAX_MESH_STAGE_THREADS, 1, 1];
    assert_eq!(d.validate(&caps), Ok(()), "exactly at the limits");
}

#[test]
fn mesh_pipelines_need_targets() {
    let mut d = desc();
    d.color_formats.clear();
    invalid(d.validate(&caps()), "no targets");
    d.color_formats = vec![Format::Rgba8Unorm; 9];
    invalid(d.validate(&caps()), "nine colour targets");
}

#[test]
fn masked_caps_refuse_mesh_pipelines() {
    let masked = caps().masked(&CapsMask {
        mesh_shaders: true,
        ..CapsMask::default()
    });
    let mut d = desc();
    assert!(matches!(d.validate(&masked), Err(RhiError::Unsupported(_))));
    d.object = None;
    assert!(matches!(d.validate(&masked), Err(RhiError::Unsupported(_))));
    let mut no_object_shaders = caps();
    no_object_shaders.object_shaders = false;
    assert!(matches!(
        desc().validate(&no_object_shaders),
        Err(RhiError::Unsupported(_))
    ));
    d.object = None;
    assert_eq!(
        d.validate(&no_object_shaders),
        Ok(()),
        "a mesh-only pipeline"
    );
}

fn begin() -> Command {
    Command::BeginRenderPass(RenderPassDesc {
        colors: vec![ColorAttachment {
            texture: Texture::from_raw(1, 1),
            resolve: None,
            load: LoadOp::Clear(ClearValue::Color([0.0; 4])),
            store: StoreOp::Store,
        }],
        depth: None,
    })
}

fn draw(groups: [u32; 3], inside: bool) -> Result<(), RhiError> {
    let mut list = CommandList::new("t");
    if inside {
        list.push(begin());
    }
    list.push(Command::DrawMeshTasks { groups });
    if inside {
        list.push(Command::EndRenderPass);
    }
    list.validate()
}

#[test]
fn draw_mesh_tasks_needs_a_pass_and_non_zero_groups() {
    assert_eq!(draw([3, 1, 1], true), Ok(()));
    invalid(draw([3, 1, 1], false), "outside a pass");
    for groups in [[0, 1, 1], [1, 0, 1], [1, 1, 0]] {
        invalid(draw(groups, true), &format!("{groups:?}"));
    }
    invalid(
        draw([MAX_MESH_TASK_GROUPS + 1, 1, 1], true),
        "past a dimension",
    );
    invalid(draw([4096, 1025, 1], true), "past the total");
    assert_eq!(
        draw([MAX_MESH_TASK_GROUPS, 64, 1], true),
        Ok(()),
        "at the total"
    );
}
