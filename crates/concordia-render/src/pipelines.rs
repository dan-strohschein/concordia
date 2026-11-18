//! Shaders and pipelines: forward PBR (per sample count and culling) and tonemapping.

use std::collections::BTreeMap;
use std::path::PathBuf;

use concordia_rhi::{
    Backend, BlendMode, CompareOp, CullMode, DepthState, Device, Format, GraphicsPipelineDesc,
    Pipeline, SampleCount, ShaderSource,
};
use concordia_shaders::{CompileRequest, ShaderCompiler, ShaderModule, Stage, Target};

use crate::RenderError;

/// HDR scene color.
pub(crate) const HDR: Format = Format::Rgba16Float;
/// Depth (reverse-Z).
pub(crate) const DEPTH: Format = Format::Depth32Float;
/// Display output (sRGB-encoded on write).
pub(crate) const OUTPUT: Format = Format::Rgba8UnormSrgb;

/// The render crate's Slang sources.
pub fn shader_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders"))
}

/// Compiles one entry point of a render shader for the device's backend.
pub(crate) fn compile(
    device: &dyn Device,
    compiler: &dyn ShaderCompiler,
    file: &str,
    entry: &str,
    stage: Stage,
) -> Result<ShaderModule, RenderError> {
    let target = match device.caps().backend {
        Backend::Metal => Target::Msl,
        Backend::Vulkan => Target::SpirV,
    };
    let request = CompileRequest {
        path: shader_dir().join(file),
        entry: entry.into(),
        stage,
        target,
        defines: Default::default(),
        include_dirs: vec![concordia_shaders::builtin_dir()],
    };
    compiler
        .compile(&request)
        .map_err(|e| RenderError::Shader(e.to_string()))
}

/// Compiled shaders and lazily created pipelines.
pub(crate) struct Pipelines {
    forward: (ShaderSource, ShaderSource),
    tonemap: Pipeline,
    variants: BTreeMap<(u8, bool), Pipeline>,
}

impl Pipelines {
    pub(crate) fn new(
        device: &dyn Device,
        compiler: &dyn ShaderCompiler,
    ) -> Result<Self, RenderError> {
        let compile = |file: &str, entry: &str, stage: Stage| {
            compile(device, compiler, file, entry, stage).map(|m| m.source())
        };
        let forward = (
            compile("forward.slang", "vs", Stage::Vertex)?,
            compile("forward.slang", "fs", Stage::Fragment)?,
        );
        let tonemap = device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: compile("tonemap.slang", "fullscreen", Stage::Vertex)?,
            fragment: Some(compile("tonemap.slang", "tonemap", Stage::Fragment)?),
            color_formats: vec![OUTPUT],
            depth: None,
            samples: SampleCount::X1,
            cull: CullMode::None,
            blend: BlendMode::Opaque,
            label: "tonemap".into(),
        })?;
        Ok(Self {
            forward,
            tonemap,
            variants: BTreeMap::new(),
        })
    }

    pub(crate) fn tonemap(&self) -> Pipeline {
        self.tonemap
    }

    /// The forward pipeline for a sample count and face culling.
    pub(crate) fn forward(
        &mut self,
        device: &dyn Device,
        samples: SampleCount,
        double_sided: bool,
    ) -> Result<Pipeline, RenderError> {
        let key = (samples.get(), double_sided);
        if let Some(p) = self.variants.get(&key) {
            return Ok(*p);
        }
        let pipeline = device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: self.forward.0.clone(),
            fragment: Some(self.forward.1.clone()),
            color_formats: vec![HDR],
            depth: Some(DepthState {
                format: DEPTH,
                compare: CompareOp::GreaterEqual,
                write: true,
            }),
            samples,
            cull: if double_sided {
                CullMode::None
            } else {
                CullMode::Back
            },
            blend: BlendMode::Opaque,
            label: format!(
                "forward x{} {}",
                samples.get(),
                if double_sided {
                    "double-sided"
                } else {
                    "culled"
                }
            ),
        })?;
        self.variants.insert(key, pipeline);
        Ok(pipeline)
    }
}
