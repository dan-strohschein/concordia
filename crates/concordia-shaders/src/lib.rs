//! Concordia shaders: Slang → MSL / SPIR-V through the pinned `slangc` (ADR-0045), reflection,
//! Rust/Slang layout checks, a content-addressed cache and a hot-reloading library.
//!
//! Shaders `import bindless;` (in [`builtin_dir`]) to reach the RHI's bindless tables.

mod cache;
#[cfg(feature = "conformance")]
pub mod conformance;
mod deps;
mod error;
mod layout;
mod library;
mod reflect;
mod request;
mod slangc;
mod toolchain;

use std::path::PathBuf;

pub use cache::{CachedCompiler, ShaderCache, cache_key};
pub use deps::dependencies;
pub use error::{Diagnostic, ShaderError};
pub use layout::{Layout, ShaderStruct, check_layout};
pub use library::{ReloadOutcome, ShaderLibrary};
pub use reflect::{Binding, FieldLayout, ShaderReflection, StructLayout};
pub use request::{CompileRequest, SPIRV_DEFINE, ShaderCode, ShaderModule, Stage, Target};
pub use slangc::SlangcCompiler;
pub use toolchain::SlangToolchain;

/// Compiles shaders. `slangc` today; an in-process Slang binding could implement it later.
pub trait ShaderCompiler: Send + Sync {
    /// Compiles one entry point.
    fn compile(&self, request: &CompileRequest) -> Result<ShaderModule, ShaderError>;
    /// The compiler version (part of every cache key).
    fn version(&self) -> &str;
}

/// The engine's built-in Slang modules (`bindless.slang`, ...); add it to every request's include dirs.
pub fn builtin_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders"))
}
