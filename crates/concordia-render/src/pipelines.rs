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
    tonemap: (ShaderSource, ShaderSource),
    tonemaps: BTreeMap<u8, Pipeline>,
    overlay: (ShaderSource, ShaderSource),
    overlays: BTreeMap<u8, Pipeline>,
    shadow: Pipeline,
    sky: (ShaderSource, ShaderSource),
    skies: BTreeMap<u8, Pipeline>,
    grid: (ShaderSource, ShaderSource),
    grids: BTreeMap<u8, Pipeline>,
    variants: BTreeMap<(u8, bool, bool), Pipeline>,
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
        let grid = (
            compile("forward.slang", "grid_vs", Stage::Vertex)?,
            compile("forward.slang", "grid_fs", Stage::Fragment)?,
        );
        let overlay = (
            compile("overlay.slang", "overlay_vs", Stage::Vertex)?,
            compile("overlay.slang", "overlay_fs", Stage::Fragment)?,
        );
        let tonemap = (
            compile("tonemap.slang", "fullscreen", Stage::Vertex)?,
            compile("tonemap.slang", "tonemap", Stage::Fragment)?,
        );
        let shadow = device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: compile("forward.slang", "shadow_vs", Stage::Vertex)?,
            fragment: None,
            color_formats: Vec::new(),
            depth: Some(DepthState {
                format: DEPTH,
                compare: CompareOp::GreaterEqual,
                write: true,
            }),
            samples: SampleCount::X1,
            cull: CullMode::None,
            blend: BlendMode::Opaque,
            label: "shadow depth".into(),
        })?;
        let sky = (
            compile("forward.slang", "sky_vs", Stage::Vertex)?,
            compile("forward.slang", "sky_fs", Stage::Fragment)?,
        );
        Ok(Self {
            forward,
            tonemap,
            tonemaps: BTreeMap::new(),
            shadow,
            sky,
            skies: BTreeMap::new(),
            overlay,
            overlays: BTreeMap::new(),
            grid,
            grids: BTreeMap::new(),
            variants: BTreeMap::new(),
        })
    }

    /// The tonemap pipeline writing `format` (sRGB RGBA for captures, BGRA for window surfaces).
    pub(crate) fn tonemap(
        &mut self,
        device: &dyn Device,
        format: Format,
    ) -> Result<Pipeline, RenderError> {
        let key = format as u8;
        if let Some(p) = self.tonemaps.get(&key) {
            return Ok(*p);
        }
        let pipeline = device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: self.tonemap.0.clone(),
            fragment: Some(self.tonemap.1.clone()),
            color_formats: vec![format],
            depth: None,
            samples: SampleCount::X1,
            cull: CullMode::None,
            blend: BlendMode::Opaque,
            label: format!("tonemap {format:?}"),
        })?;
        self.tonemaps.insert(key, pipeline);
        Ok(pipeline)
    }

    /// The editor-overlay pipeline writing `format` (premultiplied alpha over the image).
    pub(crate) fn overlay(
        &mut self,
        device: &dyn Device,
        format: Format,
    ) -> Result<Pipeline, RenderError> {
        let key = format as u8;
        if let Some(p) = self.overlays.get(&key) {
            return Ok(*p);
        }
        let pipeline = device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: self.overlay.0.clone(),
            fragment: Some(self.overlay.1.clone()),
            color_formats: vec![format],
            depth: None,
            samples: SampleCount::X1,
            cull: CullMode::None,
            blend: BlendMode::PremultipliedAlpha,
            label: format!("overlay {format:?}"),
        })?;
        self.overlays.insert(key, pipeline);
        Ok(pipeline)
    }

    /// The skybox pipeline for a sample count (drawn first, no depth test or write).
    pub(crate) fn sky(
        &mut self,
        device: &dyn Device,
        samples: SampleCount,
    ) -> Result<Pipeline, RenderError> {
        if let Some(p) = self.skies.get(&samples.get()) {
            return Ok(*p);
        }
        let pipeline = device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: self.sky.0.clone(),
            fragment: Some(self.sky.1.clone()),
            color_formats: vec![HDR],
            depth: Some(DepthState {
                format: DEPTH,
                compare: CompareOp::Always,
                write: false,
            }),
            samples,
            cull: CullMode::None,
            blend: BlendMode::Opaque,
            label: format!("sky x{}", samples.get()),
        })?;
        self.skies.insert(samples.get(), pipeline);
        Ok(pipeline)
    }

    /// The editor grid pipeline for a sample count (depth-tested, no depth write, blended).
    pub(crate) fn grid(
        &mut self,
        device: &dyn Device,
        samples: SampleCount,
    ) -> Result<Pipeline, RenderError> {
        if let Some(p) = self.grids.get(&samples.get()) {
            return Ok(*p);
        }
        let pipeline = device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: self.grid.0.clone(),
            fragment: Some(self.grid.1.clone()),
            color_formats: vec![HDR],
            depth: Some(DepthState {
                format: DEPTH,
                compare: CompareOp::GreaterEqual,
                write: false,
            }),
            samples,
            cull: CullMode::None,
            blend: BlendMode::PremultipliedAlpha,
            label: format!("grid x{}", samples.get()),
        })?;
        self.grids.insert(samples.get(), pipeline);
        Ok(pipeline)
    }

    /// Depth-only pipeline for shadow casters.
    pub(crate) fn shadow(&self) -> Pipeline {
        self.shadow
    }

    /// The forward pipeline for a sample count, face culling and blending (premultiplied alpha,
    /// no depth write).
    pub(crate) fn forward(
        &mut self,
        device: &dyn Device,
        samples: SampleCount,
        double_sided: bool,
        blend: bool,
    ) -> Result<Pipeline, RenderError> {
        let key = (samples.get(), double_sided, blend);
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
                write: !blend,
            }),
            samples,
            cull: if double_sided {
                CullMode::None
            } else {
                CullMode::Back
            },
            blend: if blend {
                BlendMode::PremultipliedAlpha
            } else {
                BlendMode::Opaque
            },
            label: format!(
                "forward x{}{}{}",
                samples.get(),
                if double_sided { " double-sided" } else { "" },
                if blend { " blended" } else { "" }
            ),
        })?;
        self.variants.insert(key, pipeline);
        Ok(pipeline)
    }
}
