//! Compile requests and compiled modules.

use std::collections::BTreeMap;
use std::path::PathBuf;

use concordia_rhi::ShaderSource;
use serde::{Deserialize, Serialize};

use crate::reflect::ShaderReflection;

/// Pipeline stage of an entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage {
    /// Compute kernel.
    Compute,
    /// Vertex shader.
    Vertex,
    /// Fragment shader.
    Fragment,
}

impl Stage {
    pub(crate) fn slang(self) -> &'static str {
        match self {
            Self::Compute => "compute",
            Self::Vertex => "vertex",
            Self::Fragment => "fragment",
        }
    }
}

/// Output language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    /// Metal Shading Language (Metal backend).
    Msl,
    /// SPIR-V (Vulkan backend).
    SpirV,
}

impl Target {
    pub(crate) fn slang(self) -> &'static str {
        match self {
            Self::Msl => "metal",
            Self::SpirV => "spirv",
        }
    }
}

/// One entry point to compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileRequest {
    /// Root `.slang` file.
    pub path: PathBuf,
    /// Entry point name.
    pub entry: String,
    /// Its stage.
    pub stage: Stage,
    /// Output language.
    pub target: Target,
    /// Preprocessor defines.
    pub defines: BTreeMap<String, String>,
    /// Directories searched by `import` and `#include` (include [`crate::builtin_dir`]).
    pub include_dirs: Vec<PathBuf>,
}

/// Compiled code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShaderCode {
    /// MSL source.
    Msl(String),
    /// SPIR-V words.
    SpirV(Vec<u32>),
}

/// A compiled entry point and its reflection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderModule {
    /// Entry point name in the generated code.
    pub entry: String,
    /// The code.
    pub code: ShaderCode,
    /// Bindings, push constants, thread group size.
    pub reflection: ShaderReflection,
}

impl ShaderModule {
    /// The RHI shader source for pipeline creation.
    pub fn source(&self) -> ShaderSource {
        match &self.code {
            ShaderCode::Msl(source) => ShaderSource::Msl {
                source: source.clone(),
                entry: self.entry.clone(),
            },
            ShaderCode::SpirV(words) => ShaderSource::SpirV {
                words: words.clone(),
                entry: self.entry.clone(),
            },
        }
    }
}
