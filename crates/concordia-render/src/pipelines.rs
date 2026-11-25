//! Shaders and pipelines: forward shading (per sample count, culling, blending and program
//! shape) and tonemapping.

use std::collections::BTreeMap;
use std::path::PathBuf;

use concordia_rhi::{
    BlendMode, ColorWrites, CompareOp, ComputePipelineDesc, CullMode, DepthState, Device, Format,
    GraphicsPipelineDesc, MeshPipelineDesc, MeshStage, ObjectStage, Pipeline, RhiError,
    SampleCount, ShaderSource,
};
use concordia_shaders::{CompileRequest, ShaderCompiler, ShaderModule, Stage, Target};

use crate::RenderError;
use crate::shape::ProgramShape;

/// HDR scene color.
pub(crate) const HDR: Format = Format::Rgba16Float;
/// Depth (reverse-Z).
pub(crate) const DEPTH: Format = Format::Depth32Float;
/// Motion vectors (UV units, y down; M4 4.3b2), the forward pass's second color target.
pub(crate) const MOTION: Format = Format::Rg16Float;
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
    let target = Target::from(device.caps().backend);
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

/// Where a forward draw's vertices come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Geometry {
    /// The mesh's index and vertex streams (`vs`, the M1 path).
    Mesh,
    /// Meshlet entries drawn indirectly (`meshlet_vs`, ADR-0100).
    Meshlets,
    /// Meshlets drawn by the object and mesh stages (`meshlet_object`, `meshlet_mesh`; 4.3b).
    MeshTasks,
}

/// The mesh-shader path's stages, compiled when the device takes the path (4.3b).
struct MeshStages {
    object: ObjectStage,
    mesh: MeshStage,
    /// Depth-only mesh pipeline for meshlet shadow casters (`meshlet_shadow_mesh`).
    shadow: Pipeline,
}

/// What a forward-pass pipeline is, apart from where its vertices come from.
struct Raster {
    fragment: Option<ShaderSource>,
    depth: DepthState,
    samples: SampleCount,
    cull: CullMode,
    blend: BlendMode,
    color_writes: ColorWrites,
    label: String,
}

/// The forward pass's color target only (blended draws, debug views and the grid leave motion as
/// the opaque surfaces wrote it).
fn color_only() -> ColorWrites {
    ColorWrites::only(&[0]).expect("target 0 exists")
}

/// Compiled shaders and lazily created pipelines.
pub(crate) struct Pipelines {
    /// Kept to compile a forward fragment variant per program shape on first use.
    compiler: Box<dyn ShaderCompiler>,
    forward: (ShaderSource, ShaderSource),
    /// `meshlet_vs`, paired with the forward fragment shaders.
    meshlet_vs: ShaderSource,
    /// The forward fragment shader specialized to each layered program shape seen.
    shaped: BTreeMap<ProgramShape, ShaderSource>,
    tonemap: (ShaderSource, ShaderSource),
    tonemaps: BTreeMap<u8, Pipeline>,
    overlay: (ShaderSource, ShaderSource),
    overlays: BTreeMap<u8, Pipeline>,
    ui: (ShaderSource, ShaderSource),
    uis: BTreeMap<u8, Pipeline>,
    shadow: Pipeline,
    meshlet_shadow: Pipeline,
    expand: Pipeline,
    /// The mesh-shader path's stages, when the device takes it.
    mesh_stages: Option<MeshStages>,
    sky: (ShaderSource, ShaderSource),
    skies: BTreeMap<u8, Pipeline>,
    grid: (ShaderSource, ShaderSource),
    grids: BTreeMap<u8, Pipeline>,
    variants: BTreeMap<(u8, bool, bool, Option<ProgramShape>, Geometry), Pipeline>,
    debug: BTreeMap<(u8, bool, Geometry), Pipeline>,
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
        let shadow_depth = |vertex: ShaderSource, label: &str| {
            device.create_graphics_pipeline(&GraphicsPipelineDesc {
                vertex,
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
                color_writes: concordia_rhi::ColorWrites::ALL,
                label: label.into(),
            })
        };
        let shadow = shadow_depth(
            compile("forward.slang", "shadow_vs", Stage::Vertex)?,
            "shadow depth",
        )?;
        let meshlet_shadow = shadow_depth(
            compile("meshlet.slang", "meshlet_shadow_vs", Stage::Vertex)?,
            "meshlet shadow depth",
        )?;
        let expand_module = compile_with(
            device,
            compiler.as_ref(),
            "meshlet.slang",
            "meshlet_expand",
            Stage::Compute,
            BTreeMap::new(),
        )?;
        let expand = device.create_compute_pipeline(&ComputePipelineDesc {
            shader: expand_module.source(),
            threads_per_group: expand_module.reflection.threads_per_group.ok_or_else(|| {
                RenderError::Shader("meshlet.expand: no thread group size".into())
            })?,
            label: "meshlet.expand".into(),
        })?;
        let meshlet_vs = compile("meshlet.slang", "meshlet_vs", Stage::Vertex)?;
        let mesh_stages = match crate::meshlet::route(device.caps(), crate::ViewMode::lit) {
            crate::meshlet::Route::MeshTasks => Some(mesh_stages(device, compiler.as_ref())?),
            crate::meshlet::Route::ComputeIndirect => None,
        };
        let sky = (
            compile("forward.slang", "sky_vs", Stage::Vertex)?,
            compile("forward.slang", "sky_fs", Stage::Fragment)?,
        );
        Ok(Self {
            compiler,
            forward,
            meshlet_vs,
            shaped: BTreeMap::new(),
            tonemap,
            tonemaps: BTreeMap::new(),
            shadow,
            meshlet_shadow,
            expand,
            mesh_stages,
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
            color_writes: concordia_rhi::ColorWrites::ALL,
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
            color_writes: concordia_rhi::ColorWrites::ALL,
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
            color_writes: concordia_rhi::ColorWrites::ALL,
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
            color_formats: vec![HDR, MOTION],
            depth: Some(DepthState {
                format: DEPTH,
                compare: CompareOp::Always,
                write: false,
            }),
            samples,
            cull: CullMode::None,
            blend: BlendMode::Opaque,
            color_writes: concordia_rhi::ColorWrites::ALL,
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
            color_formats: vec![HDR, MOTION],
            depth: Some(DepthState {
                format: DEPTH,
                compare: CompareOp::GreaterEqual,
                write: false,
            }),
            samples,
            cull: CullMode::None,
            blend: BlendMode::PremultipliedAlpha,
            color_writes: color_only(),
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

    /// Depth-only pipeline for meshlet shadow casters (`meshlet_shadow_vs`).
    pub(crate) fn meshlet_shadow(&self) -> Pipeline {
        self.meshlet_shadow
    }

    /// Depth-only mesh pipeline for meshlet shadow casters on the mesh-shader path, if the device
    /// takes it.
    pub(crate) fn meshlet_shadow_mesh(&self) -> Option<Pipeline> {
        self.mesh_stages.as_ref().map(|m| m.shadow)
    }

    /// The `meshlet.expand` compute pipeline.
    pub(crate) fn expand(&self) -> Pipeline {
        self.expand
    }

    /// Creates `raster` with `geometry`'s vertex source: a vertex shader, or the object and mesh
    /// stages (an error when the device doesn't take the mesh-shader path).
    fn create(
        &self,
        device: &dyn Device,
        geometry: Geometry,
        raster: Raster,
    ) -> Result<Pipeline, RenderError> {
        let vertex = match geometry {
            Geometry::Mesh => self.forward.0.clone(),
            Geometry::Meshlets => self.meshlet_vs.clone(),
            Geometry::MeshTasks => {
                let stages = self.mesh_stages.as_ref().ok_or_else(|| {
                    RhiError::Unsupported(format!("{}: no mesh-shader path", raster.label))
                })?;
                return Ok(device.create_mesh_pipeline(&MeshPipelineDesc {
                    object: Some(stages.object.clone()),
                    mesh: stages.mesh.clone(),
                    fragment: raster.fragment,
                    color_formats: vec![HDR, MOTION],
                    depth: Some(raster.depth),
                    samples: raster.samples,
                    cull: raster.cull,
                    blend: raster.blend,
                    color_writes: raster.color_writes,
                    label: raster.label,
                })?);
            }
        };
        Ok(device.create_graphics_pipeline(&GraphicsPipelineDesc {
            vertex,
            fragment: raster.fragment,
            color_formats: vec![HDR, MOTION],
            depth: Some(raster.depth),
            samples: raster.samples,
            cull: raster.cull,
            blend: raster.blend,
            color_writes: raster.color_writes,
            label: raster.label,
        })?)
    }

    /// The forward pipeline for the wireframe (`additive` false) and overdraw (`additive` true) view
    /// modes: every face of every surface, no depth test or write.
    pub(crate) fn debug(
        &mut self,
        device: &dyn Device,
        samples: SampleCount,
        additive: bool,
        geometry: Geometry,
    ) -> Result<Pipeline, RenderError> {
        let key = (samples.get(), additive, geometry);
        if let Some(p) = self.debug.get(&key) {
            return Ok(*p);
        }
        let raster = Raster {
            fragment: Some(self.forward.1.clone()),
            depth: DepthState {
                format: DEPTH,
                compare: CompareOp::Always,
                write: false,
            },
            samples,
            cull: CullMode::None,
            blend: if additive {
                BlendMode::Additive
            } else {
                BlendMode::Opaque
            },
            color_writes: color_only(),
            label: format!(
                "forward x{} {} {geometry:?}",
                samples.get(),
                if additive { "overdraw" } else { "wireframe" }
            ),
        };
        let pipeline = self.create(device, geometry, raster)?;
        self.debug.insert(key, pipeline);
        Ok(pipeline)
    }

    /// The pipeline a draw uses in `view_mode`: the debug pipelines for wireframe and overdraw,
    /// else the forward variant.
    pub(crate) fn for_view(
        &mut self,
        device: &dyn Device,
        view_mode: crate::ViewMode,
        variant: (SampleCount, bool, bool),
        shape: Option<&ProgramShape>,
        geometry: Geometry,
    ) -> Result<Pipeline, RenderError> {
        match view_mode {
            crate::ViewMode::wireframe => self.debug(device, variant.0, false, geometry),
            crate::ViewMode::overdraw => self.debug(device, variant.0, true, geometry),
            _ => self.forward(device, variant, shape, geometry),
        }
    }

    /// The forward pipeline for a sample count, face culling, blending (premultiplied alpha, no
    /// depth write), program shape and vertex source.
    pub(crate) fn forward(
        &mut self,
        device: &dyn Device,
        (samples, double_sided, blend): (SampleCount, bool, bool),
        shape: Option<&ProgramShape>,
        geometry: Geometry,
    ) -> Result<Pipeline, RenderError> {
        let key = (samples.get(), double_sided, blend, shape.cloned(), geometry);
        if let Some(p) = self.variants.get(&key) {
            return Ok(*p);
        }
        let fragment = self.fragment(device, shape)?;
        let raster = Raster {
            fragment: Some(fragment),
            depth: DepthState {
                format: DEPTH,
                compare: CompareOp::GreaterEqual,
                write: !blend,
            },
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
            color_writes: if blend {
                color_only()
            } else {
                ColorWrites::ALL
            },
            label: format!(
                "forward x{}{}{}{} {geometry:?}",
                samples.get(),
                if double_sided { " double-sided" } else { "" },
                if blend { " blended" } else { "" },
                if shape.is_some() { " layered" } else { "" }
            ),
        };
        let pipeline = self.create(device, geometry, raster)?;
        self.variants.insert(key, pipeline);
        Ok(pipeline)
    }
}

/// Compiles the mesh-shader path's stages and creates its shadow pipeline (4.3b).
fn mesh_stages(
    device: &dyn Device,
    compiler: &dyn ShaderCompiler,
) -> Result<MeshStages, RenderError> {
    let stage = |entry: &str, stage: Stage| -> Result<(ShaderSource, [u32; 3]), RenderError> {
        let module = compile(device, compiler, "meshlet.slang", entry, stage)?;
        let threads = module
            .reflection
            .threads_per_group
            .ok_or_else(|| RenderError::Shader(format!("{entry}: no thread group size")))?;
        Ok((module.source(), threads))
    };
    let (object, threads) = stage("meshlet_object", Stage::Amplification)?;
    let object = ObjectStage {
        shader: object,
        threads_per_group: threads,
        payload_bytes: crate::meshlet::OBJECT_ENTRIES * 8,
    };
    let mesh_stage = |entry: &str| -> Result<MeshStage, RenderError> {
        let (shader, threads) = stage(entry, Stage::Mesh)?;
        Ok(MeshStage {
            shader,
            threads_per_group: threads,
            max_vertices: crate::meshlet::MESH_OUTPUTS,
            max_primitives: crate::meshlet::MESH_OUTPUTS,
        })
    };
    let mesh = mesh_stage("meshlet_mesh")?;
    let shadow = device.create_mesh_pipeline(&MeshPipelineDesc {
        object: Some(object.clone()),
        mesh: mesh_stage("meshlet_shadow_mesh")?,
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
        color_writes: concordia_rhi::ColorWrites::ALL,
        label: "meshlet shadow depth (mesh)".into(),
    })?;
    Ok(MeshStages {
        object,
        mesh,
        shadow,
    })
}
