//! Shaders and pipelines: forward shading (per sample count, culling, blending and program
//! shape) and tonemapping.

use std::collections::BTreeMap;
use std::path::PathBuf;

use concordia_rhi::{
    Backend, BlendMode, CompareOp, CullMode, DepthState, Device, Format, GraphicsPipelineDesc,
    Pipeline, SampleCount, ShaderSource,
};
use concordia_shaders::{CompileRequest, ShaderCompiler, ShaderModule, Stage, Target};

use crate::RenderError;
use crate::shape::ProgramShape;

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

/// Where render shaders' imports resolve: the shader builtins and `concordia-material`'s
/// `material.slang` (the shared BSDF).
pub fn include_dirs() -> Vec<PathBuf> {
    vec![
        concordia_shaders::builtin_dir(),
        concordia_material::shader_dir(),
    ]
}

/// Compiles one entry point of a render shader for the device's backend.
pub(crate) fn compile(
    device: &dyn Device,
    compiler: &dyn ShaderCompiler,
    file: &str,
    entry: &str,
    stage: Stage,
) -> Result<ShaderModule, RenderError> {
    compile_with(device, compiler, file, entry, stage, BTreeMap::new())
}

/// [`compile`] with preprocessor defines.
fn compile_with(
    device: &dyn Device,
    compiler: &dyn ShaderCompiler,
    file: &str,
    entry: &str,
    stage: Stage,
    defines: BTreeMap<String, String>,
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
        defines,
        include_dirs: include_dirs(),
    };
    compiler
        .compile(&request)
        .map_err(|e| RenderError::Shader(e.to_string()))
}

/// Compiled shaders and lazily created pipelines.
pub(crate) struct Pipelines {
    /// Kept to compile a forward fragment variant per program shape on first use.
    compiler: Box<dyn ShaderCompiler>,
    forward: (ShaderSource, ShaderSource),
    /// The forward fragment shader specialized to each layered program shape seen.
    shaped: BTreeMap<ProgramShape, ShaderSource>,
    tonemap: (ShaderSource, ShaderSource),
    tonemaps: BTreeMap<u8, Pipeline>,
    overlay: (ShaderSource, ShaderSource),
    overlays: BTreeMap<u8, Pipeline>,
    ui: (ShaderSource, ShaderSource),
    uis: BTreeMap<u8, Pipeline>,
    shadow: Pipeline,
    sky: (ShaderSource, ShaderSource),
    skies: BTreeMap<u8, Pipeline>,
    grid: (ShaderSource, ShaderSource),
    grids: BTreeMap<u8, Pipeline>,
    variants: BTreeMap<(u8, bool, bool, Option<ProgramShape>), Pipeline>,
    debug: BTreeMap<(u8, bool), Pipeline>,
}

impl Pipelines {
    pub(crate) fn new(
        device: &dyn Device,
        compiler: Box<dyn ShaderCompiler>,
    ) -> Result<Self, RenderError> {
        let compile = |file: &str, entry: &str, stage: Stage| {
            compile(device, compiler.as_ref(), file, entry, stage).map(|m| m.source())
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
        let ui = (
            compile("ui.slang", "ui_vs", Stage::Vertex)?,
            compile("ui.slang", "ui_fs", Stage::Fragment)?,
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
            compiler,
            forward,
            shaped: BTreeMap::new(),
            tonemap,
            tonemaps: BTreeMap::new(),
            shadow,
            sky,
            skies: BTreeMap::new(),
            overlay,
            overlays: BTreeMap::new(),
            ui,
            uis: BTreeMap::new(),
            grid,
            grids: BTreeMap::new(),
            variants: BTreeMap::new(),
            debug: BTreeMap::new(),
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

    /// The in-game UI pipeline writing `format` (premultiplied alpha over the image).
    pub(crate) fn ui(
        &mut self,
        device: &dyn Device,
        format: Format,
    ) -> Result<Pipeline, RenderError> {
        let key = format as u8;
        if let Some(p) = self.uis.get(&key) {
            return Ok(*p);
        }
        let pipeline = device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: self.ui.0.clone(),
            fragment: Some(self.ui.1.clone()),
            color_formats: vec![format],
            depth: None,
            samples: SampleCount::X1,
            cull: CullMode::None,
            blend: BlendMode::PremultipliedAlpha,
            label: format!("ui {format:?}"),
        })?;
        self.uis.insert(key, pipeline);
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

    /// How many shape-specialized forward fragment shaders have been compiled.
    pub(crate) fn material_variants(&self) -> usize {
        self.shaped.len()
    }

    /// The forward fragment shader for `shape` (the base shader for one-slab programs).
    fn fragment(
        &mut self,
        device: &dyn Device,
        shape: Option<&ProgramShape>,
    ) -> Result<ShaderSource, RenderError> {
        let Some(shape) = shape else {
            return Ok(self.forward.1.clone());
        };
        if let Some(source) = self.shaped.get(shape) {
            return Ok(source.clone());
        }
        let module = compile_with(
            device,
            self.compiler.as_ref(),
            "forward.slang",
            "fs",
            Stage::Fragment,
            shape.defines(),
        )?;
        self.shaped.insert(shape.clone(), module.source());
        Ok(module.source())
    }

    /// Depth-only pipeline for shadow casters.
    pub(crate) fn shadow(&self) -> Pipeline {
        self.shadow
    }

    /// The forward pipeline for the wireframe (`additive` false) and overdraw (`additive` true) view
    /// modes: every face of every surface, no depth test or write.
    pub(crate) fn debug(
        &mut self,
        device: &dyn Device,
        samples: SampleCount,
        additive: bool,
    ) -> Result<Pipeline, RenderError> {
        let key = (samples.get(), additive);
        if let Some(p) = self.debug.get(&key) {
            return Ok(*p);
        }
        let pipeline = device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: self.forward.0.clone(),
            fragment: Some(self.forward.1.clone()),
            color_formats: vec![HDR],
            depth: Some(DepthState {
                format: DEPTH,
                compare: CompareOp::Always,
                write: false,
            }),
            samples,
            cull: CullMode::None,
            blend: if additive {
                BlendMode::Additive
            } else {
                BlendMode::Opaque
            },
            label: format!(
                "forward x{} {}",
                samples.get(),
                if additive { "overdraw" } else { "wireframe" }
            ),
        })?;
        self.debug.insert(key, pipeline);
        Ok(pipeline)
    }

    /// The forward pipeline for a sample count, face culling, blending (premultiplied alpha, no
    /// depth write) and program shape.
    pub(crate) fn forward(
        &mut self,
        device: &dyn Device,
        samples: SampleCount,
        double_sided: bool,
        blend: bool,
        shape: Option<&ProgramShape>,
    ) -> Result<Pipeline, RenderError> {
        let key = (samples.get(), double_sided, blend, shape.cloned());
        if let Some(p) = self.variants.get(&key) {
            return Ok(*p);
        }
        let fragment = self.fragment(device, shape)?;
        let pipeline = device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex: self.forward.0.clone(),
            fragment: Some(fragment),
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
                "forward x{}{}{}{}",
                samples.get(),
                if double_sided { " double-sided" } else { "" },
                if blend { " blended" } else { "" },
                if shape.is_some() { " layered" } else { "" }
            ),
        })?;
        self.variants.insert(key, pipeline);
        Ok(pipeline)
    }
}
