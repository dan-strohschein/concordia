//! Pipeline descriptors.

use crate::caps::{RhiCaps, SampleCount};
use crate::error::RhiError;
use crate::format::Format;

/// Shader code for a backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShaderSource {
    /// Metal Shading Language source and entry point.
    Msl {
        /// Source text.
        source: String,
        /// Entry point.
        entry: String,
    },
    /// SPIR-V words and entry point (Vulkan).
    SpirV {
        /// Code.
        words: Vec<u32>,
        /// Entry point.
        entry: String,
    },
}

/// A compute pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputePipelineDesc {
    /// The kernel.
    pub shader: ShaderSource,
    /// Threads per group (from the shader's numthreads; Metal needs it at dispatch).
    pub threads_per_group: [u32; 3],
    /// Debug label.
    pub label: String,
}

/// Comparison for depth tests (reverse-Z uses `GreaterEqual`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    /// Never passes.
    Never,
    /// Passes if less.
    Less,
    /// Passes if less or equal.
    LessEqual,
    /// Passes if greater.
    Greater,
    /// Passes if greater or equal.
    GreaterEqual,
    /// Always passes.
    Always,
}

/// Depth testing and writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthState {
    /// Depth format.
    pub format: Format,
    /// Comparison.
    pub compare: CompareOp,
    /// Write depth.
    pub write: bool,
}

/// Face culling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CullMode {
    /// Draw both faces.
    None,
    /// Cull back faces (counter-clockwise is front).
    Back,
    /// Cull front faces.
    Front,
}

/// Color blending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// Overwrite.
    Opaque,
    /// Premultiplied alpha over.
    PremultipliedAlpha,
    /// Add source to destination (color and alpha), e.g. for overdraw counting.
    Additive,
}

/// Which color targets a pipeline writes (bit `i`: target `i`); the others keep their contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorWrites(u8);

impl ColorWrites {
    /// Every target.
    pub const ALL: Self = Self(u8::MAX);

    /// Only `targets` (each below 8); an error names a target past the eighth.
    pub fn only(targets: &[usize]) -> Result<Self, RhiError> {
        targets.iter().try_fold(Self(0), |mask, &t| match t < 8 {
            true => Ok(Self(mask.0 | 1 << t)),
            false => Err(RhiError::invalid(
                "color writes",
                format!("target {t}: there are at most 8"),
            )),
        })
    }

    /// Whether target `target` is written.
    pub fn writes(self, target: usize) -> bool {
        target < 8 && self.0 & (1 << target) != 0
    }
}

impl Default for ColorWrites {
    fn default() -> Self {
        Self::ALL
    }
}

/// A graphics pipeline (vertex pulling: no vertex input layout).
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsPipelineDesc {
    /// Vertex shader.
    pub vertex: ShaderSource,
    /// Fragment shader (none for depth-only).
    pub fragment: Option<ShaderSource>,
    /// Color target formats.
    pub color_formats: Vec<Format>,
    /// Depth state.
    pub depth: Option<DepthState>,
    /// MSAA samples of the targets.
    pub samples: SampleCount,
    /// Culling.
    pub cull: CullMode,
    /// Blending for every color target.
    pub blend: BlendMode,
    /// The color targets it writes.
    pub color_writes: ColorWrites,
    /// Debug label.
    pub label: String,
}

/// A pipeline's targets: 0..=8 colours and an optional depth, and at least one of them unless a
/// fragment stage writes storage instead (attachment-less passes, M4 4.7a).
pub fn pipeline_targets(
    colors: &[Format],
    depth: Option<&DepthState>,
    fragment: bool,
) -> Result<(), String> {
    if colors.len() > 8 {
        return Err(format!("{} color targets; at most 8", colors.len()));
    }
    match (colors.is_empty() && depth.is_none(), fragment) {
        (true, false) => Err("needs a color or depth target, or a fragment stage".into()),
        _ => Ok(()),
    }
}

/// Most threads in one object or mesh threadgroup on every backend (Vulkan's guaranteed minimum).
pub const MAX_MESH_STAGE_THREADS: u32 = 128;
/// Largest object-to-mesh payload in bytes on every backend (Metal's limit, Vulkan's minimum).
pub const MAX_MESH_PAYLOAD_BYTES: u32 = 16 * 1024;

/// The object (task/amplification) stage of a mesh pipeline; needs `RhiCaps::object_shaders`.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectStage {
    /// The shader.
    pub shader: ShaderSource,
    /// Threads per group (from the shader's numthreads; Metal needs it at draw time).
    pub threads_per_group: [u32; 3],
    /// Bytes of the payload it passes to its mesh groups (1..=[`MAX_MESH_PAYLOAD_BYTES`]).
    pub payload_bytes: u32,
}

/// The mesh stage of a mesh pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshStage {
    /// The shader.
    pub shader: ShaderSource,
    /// Threads per group (from the shader's numthreads; Metal needs it at draw time).
    pub threads_per_group: [u32; 3],
    /// Most vertices the shader declares per group (≤ `RhiCaps::max_mesh_output_vertices`).
    pub max_vertices: u32,
    /// Most triangles the shader declares per group (≤ `RhiCaps::max_mesh_output_primitives`).
    pub max_primitives: u32,
}

/// A mesh-shader pipeline (optional object stage, mesh stage, optional fragment stage); needs
/// `RhiCaps::mesh_shaders`. Drawn with `Command::DrawMeshTasks`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshPipelineDesc {
    /// Object (task) stage.
    pub object: Option<ObjectStage>,
    /// Mesh stage.
    pub mesh: MeshStage,
    /// Fragment shader (none for depth-only).
    pub fragment: Option<ShaderSource>,
    /// Color target formats.
    pub color_formats: Vec<Format>,
    /// Depth state.
    pub depth: Option<DepthState>,
    /// MSAA samples of the targets.
    pub samples: SampleCount,
    /// Culling.
    pub cull: CullMode,
    /// Blending for every color target.
    pub blend: BlendMode,
    /// The color targets it writes.
    pub color_writes: ColorWrites,
    /// Debug label.
    pub label: String,
}

impl MeshPipelineDesc {
    /// Checks the stages against `caps` and the portable limits: `Unsupported` without mesh (or
    /// object) shaders, `Invalid` for anything out of range. Never clamps.
    pub fn validate(&self, caps: &RhiCaps) -> Result<(), RhiError> {
        self.supported(caps)?;
        let invalid =
            |why: String| RhiError::invalid("mesh pipeline", format!("{:?}: {why}", self.label));
        pipeline_targets(
            &self.color_formats,
            self.depth.as_ref(),
            self.fragment.is_some(),
        )
        .map_err(invalid)?;
        stage_threads("mesh", self.mesh.threads_per_group).map_err(invalid)?;
        outputs(&self.mesh, caps).map_err(invalid)?;
        if let Some(object) = &self.object {
            stage_threads("object", object.threads_per_group).map_err(invalid)?;
            if !(1..=MAX_MESH_PAYLOAD_BYTES).contains(&object.payload_bytes) {
                return Err(invalid(format!(
                    "payload of {} bytes; 1..={MAX_MESH_PAYLOAD_BYTES} allowed",
                    object.payload_bytes
                )));
            }
        }
        Ok(())
    }

    /// `Unsupported` unless the device has mesh shaders, and object shaders if there is a stage.
    fn supported(&self, caps: &RhiCaps) -> Result<(), RhiError> {
        let missing = match (caps.mesh_shaders, caps.object_shaders, &self.object) {
            (false, _, _) => "mesh shaders",
            (true, false, Some(_)) => "object shaders",
            _ => return Ok(()),
        };
        Err(RhiError::Unsupported(format!(
            "mesh pipeline {:?}: the device has no {missing}",
            self.label
        )))
    }
}

/// A stage's group is non-empty and within [`MAX_MESH_STAGE_THREADS`].
fn stage_threads(stage: &str, threads: [u32; 3]) -> Result<(), String> {
    let total = threads.iter().map(|&t| u64::from(t)).product::<u64>();
    match (1..=u64::from(MAX_MESH_STAGE_THREADS)).contains(&total) {
        true => Ok(()),
        false => Err(format!(
            "{stage} threads {threads:?}: 1..={MAX_MESH_STAGE_THREADS} in total allowed"
        )),
    }
}

/// The mesh stage's declared outputs are non-zero and within the device's caps.
fn outputs(mesh: &MeshStage, caps: &RhiCaps) -> Result<(), String> {
    let within = |n: u32, cap: u32| (1..=cap).contains(&n);
    match within(mesh.max_vertices, caps.max_mesh_output_vertices)
        && within(mesh.max_primitives, caps.max_mesh_output_primitives)
    {
        true => Ok(()),
        false => Err(format!(
            "outputs {} vertices / {} primitives; the device allows 1..={} / 1..={}",
            mesh.max_vertices,
            mesh.max_primitives,
            caps.max_mesh_output_vertices,
            caps.max_mesh_output_primitives
        )),
    }
}

/// The fixed sampler set every backend provides after the bindless textures (ADR-0040);
/// shaders index `g_textures.samplers[...]` with the `SAMPLER_*` constants of `bindless.slang`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StaticSampler {
    /// Trilinear, 8× anisotropic, repeat (material textures).
    LinearRepeat,
    /// Trilinear, clamp to edge (post-processing, LUTs).
    LinearClamp,
    /// Point sampling, clamp to edge (exact texel reads).
    NearestClamp,
    /// Point sampling, repeat (pixel art, tiling masks).
    NearestRepeat,
}

impl StaticSampler {
    /// Every sampler, in index order.
    pub const ALL: [Self; 4] = [
        Self::LinearRepeat,
        Self::LinearClamp,
        Self::NearestClamp,
        Self::NearestRepeat,
    ];

    /// Index into the sampler array.
    pub const fn index(self) -> u32 {
        self as u32
    }

    /// The constant naming it in `bindless.slang`.
    pub const fn slang_name(self) -> &'static str {
        match self {
            Self::LinearRepeat => "SAMPLER_LINEAR_REPEAT",
            Self::LinearClamp => "SAMPLER_LINEAR_CLAMP",
            Self::NearestClamp => "SAMPLER_NEAREST_CLAMP",
            Self::NearestRepeat => "SAMPLER_NEAREST_REPEAT",
        }
    }

    /// True for linear (trilinear) filtering.
    pub const fn is_linear(self) -> bool {
        matches!(self, Self::LinearRepeat | Self::LinearClamp)
    }

    /// True for repeat addressing.
    pub const fn repeats(self) -> bool {
        matches!(self, Self::LinearRepeat | Self::NearestRepeat)
    }
}
