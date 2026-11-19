//! Shader errors.

use std::path::PathBuf;

/// One compiler message with its position.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    /// Source file.
    pub file: PathBuf,
    /// 1-based line.
    pub line: u32,
    /// 1-based column.
    pub col: u32,
    /// The message.
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.file.display(),
            self.line,
            self.col,
            self.message
        )
    }
}

/// Something went wrong compiling or checking a shader.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ShaderError {
    /// The Slang toolchain is missing or unusable.
    #[error("Slang toolchain: {0}")]
    Toolchain(String),
    /// A file could not be read or written.
    #[error("shader io: {0}")]
    Io(String),
    /// The shader did not compile.
    #[error("{path} failed to compile:\n{}", diagnostics.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))]
    Compile {
        /// Root source file.
        path: PathBuf,
        /// Compiler messages.
        diagnostics: Vec<Diagnostic>,
    },
    /// Reflection output could not be understood.
    #[error("shader reflection: {0}")]
    Reflection(String),
    /// A Rust struct does not match its Slang counterpart.
    #[error("layout of {name}: {reason}")]
    Layout {
        /// Rust type.
        name: String,
        /// What differs.
        reason: String,
    },
}

pub(crate) fn io(path: &std::path::Path, e: impl std::fmt::Display) -> ShaderError {
    ShaderError::Io(format!("{}: {e}", path.display()))
}
