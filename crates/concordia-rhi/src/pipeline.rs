//! Pipeline descriptors.

use crate::caps::SampleCount;
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
    /// Debug label.
    pub label: String,
}

/// A mesh-shader pipeline (object + mesh + fragment); needs `DeviceCaps::mesh_shaders`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshPipelineDesc {
    /// Object (task) shader.
    pub object: Option<ShaderSource>,
    /// Mesh shader.
    pub mesh: ShaderSource,
    /// Fragment shader.
    pub fragment: ShaderSource,
    /// Color target formats.
    pub color_formats: Vec<Format>,
    /// Depth state.
    pub depth: Option<DepthState>,
    /// Debug label.
    pub label: String,
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
