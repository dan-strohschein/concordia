//! Hot reload on the GPU (M1 phase 1.3): edits swap pipelines between frames; broken edits keep the last good module.

mod common;

use std::num::{NonZeroU32, NonZeroU64};
use std::path::{Path, PathBuf};
use std::time::Duration;

use concordia_rhi::{
    BlendMode, BufferDesc, BufferUsage, ClearValue, ColorAttachment, Command, CommandList,
    CullMode, Device, Format, GraphicsPipelineDesc, LoadOp, MemoryLocation, QueueKind,
    RenderPassDesc, RhiError, SampleCount, StoreOp, Submit, Texture, TextureDesc, TextureUsage,
    Timeline,
};
use concordia_shaders::{
    CompileRequest, ReloadOutcome, ShaderLibrary, SlangcCompiler, Stage, Target,
};
use concordia_testkit::gpu_required;

const SHADER: &str = r#"
struct VsOut { float4 position : SV_Position; };

[shader("vertex")]
VsOut fullscreen(uint vid : SV_VertexID) {
    float2 uv = float2((vid << 1) & 2, vid & 2);
    VsOut o;
    o.position = float4(uv * 2.0 - 1.0, 0.0, 1.0);
    return o;
}

[shader("fragment")]
float4 paint(VsOut i) : SV_Target { return float4(1.0, 0.0, 0.0, 1.0); }
"#;

fn device() -> Option<Box<dyn Device>> {
    match concordia_rhi_metal::new_headless() {
        Ok(d) => Some(d),
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            None
        }
        Err(e) => panic!("{e}"),
    }
}

fn library(file: &Path) -> ShaderLibrary {
    let mut lib = ShaderLibrary::new(Box::new(SlangcCompiler::new(common::toolchain())));
    for (name, stage) in [("fullscreen", Stage::Vertex), ("paint", Stage::Fragment)] {
        let request = CompileRequest {
            path: file.to_path_buf(),
            entry: name.into(),
            stage,
            target: Target::Msl,
            defines: Default::default(),
            include_dirs: vec![concordia_shaders::builtin_dir()],
        };
        lib.load(name, request).unwrap();
    }
    lib
}

fn pipeline(device: &dyn Device, lib: &ShaderLibrary) -> concordia_rhi::Pipeline {
    let desc = GraphicsPipelineDesc {
        vertex: lib.get("fullscreen").unwrap().source(),
        fragment: Some(lib.get("paint").unwrap().source()),
        color_formats: vec![Format::Rgba8Unorm],
        depth: None,
        samples: SampleCount::X1,
        cull: CullMode::None,
        blend: BlendMode::Opaque,
        color_writes: concordia_rhi::ColorWrites::ALL,
        label: "paint".into(),
    };
    device.create_graphics_pipeline(&desc).unwrap()
}

/// Submits one frame drawing with `pipeline`, copying the result to a readback buffer; does not wait.
fn frame(
    device: &dyn Device,
    pipeline: concordia_rhi::Pipeline,
    timeline: Timeline,
    value: u64,
) -> concordia_rhi::Buffer {
    let target: Texture = device
        .create_texture(&TextureDesc {
            width: NonZeroU32::new(4).unwrap(),
            height: NonZeroU32::new(4).unwrap(),
            mip_levels: NonZeroU32::MIN,
            samples: SampleCount::X1,
            format: Format::Rgba8Unorm,
            usage: TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC,
            label: "frame".into(),
        })
        .unwrap();
    let readback = device
        .create_buffer(&BufferDesc {
            size: NonZeroU64::new(64).unwrap(),
            usage: BufferUsage::COPY_DST,
            memory: MemoryLocation::Readback,
            label: "readback".into(),
        })
        .unwrap();
    let mut list = CommandList::new("frame");
    list.push(Command::BeginRenderPass(RenderPassDesc {
        colors: vec![ColorAttachment {
            texture: target,
            resolve: None,
            load: LoadOp::Clear(ClearValue::Color([0.0, 0.0, 0.0, 1.0])),
            store: StoreOp::Store,
        }],
        depth: None,
        extent: None,
    }))
    .push(Command::SetPipeline(pipeline))
    .push(Command::Draw {
        vertices: 0..3,
        instances: 0..1,
    })
    .push(Command::EndRenderPass)
    .push(Command::CopyTextureToBuffer {
        src: target,
        mip: 0,
        dst: readback,
        offset: 0,
        bytes_per_row: 16,
    });
    let signal = [(timeline, value)];
    device
        .submit(Submit {
            queue: QueueKind::Graphics,
            commands: &list,
            wait: &[],
            signal: &signal,
        })
        .unwrap();
    readback
}

fn pixel(
    device: &dyn Device,
    readback: concordia_rhi::Buffer,
    timeline: Timeline,
    value: u64,
) -> Vec<u8> {
    assert!(
        device
            .wait_timeline(timeline, value, Duration::from_secs(10))
            .unwrap()
    );
    device.read_buffer(readback, 0, 4).unwrap()
}

fn shader_file(dir: &tempfile::TempDir) -> PathBuf {
    let file = dir.path().join("paint.slang");
    std::fs::write(&file, SHADER).unwrap();
    file
}

#[test]
fn hot_reload_swaps_pipeline_at_frame_boundary() {
    let Some(device) = device() else { return };
    let dir = tempfile::tempdir().unwrap();
    let file = shader_file(&dir);
    let mut lib = library(&file);
    let timeline = device.create_timeline(0).unwrap();
    let old = frame(&*device, pipeline(&*device, &lib), timeline, 1);
    std::fs::write(
        &file,
        SHADER.replace("float4(1.0, 0.0, 0.0, 1.0)", "float4(0.0, 1.0, 0.0, 1.0)"),
    )
    .unwrap();
    let outcomes = lib.on_files_changed(std::slice::from_ref(&file));
    assert!(
        outcomes
            .iter()
            .all(|o| matches!(o, ReloadOutcome::Reloaded(_))),
        "{outcomes:?}"
    );
    let new = frame(&*device, pipeline(&*device, &lib), timeline, 2);
    assert_eq!(
        pixel(&*device, old, timeline, 1),
        [255, 0, 0, 255],
        "frame N keeps its pipeline"
    );
    assert_eq!(
        pixel(&*device, new, timeline, 2),
        [0, 255, 0, 255],
        "frame N+1 uses the edit"
    );
}

#[test]
fn hot_reload_keeps_last_good_pipeline_on_error() {
    let Some(device) = device() else { return };
    let dir = tempfile::tempdir().unwrap();
    let file = shader_file(&dir);
    let mut lib = library(&file);
    let before = lib.get("paint").unwrap().clone();
    std::fs::write(
        &file,
        SHADER.replace(
            "return float4(1.0, 0.0, 0.0, 1.0);",
            "return float4(1.0, 0.0, 0.0, 1.0)",
        ),
    )
    .unwrap();
    let outcomes = lib.on_files_changed(std::slice::from_ref(&file));
    let failure = outcomes.iter().find_map(|o| match o {
        ReloadOutcome::Failed(name, e) => Some((name.clone(), e.to_string())),
        ReloadOutcome::Reloaded(_) => None,
    });
    let (name, message) = failure.expect("the broken edit is reported");
    assert!(message.contains("paint.slang"), "{name}: {message}");
    assert_eq!(
        lib.get("paint"),
        Some(&before),
        "the last good module stays in use"
    );
    let timeline = device.create_timeline(0).unwrap();
    let still = frame(&*device, pipeline(&*device, &lib), timeline, 1);
    assert_eq!(pixel(&*device, still, timeline, 1), [255, 0, 0, 255]);
}
