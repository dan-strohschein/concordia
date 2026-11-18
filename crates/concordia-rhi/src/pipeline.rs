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
