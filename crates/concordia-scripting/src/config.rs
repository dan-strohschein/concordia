//! What the module loads: the project's `[scripts]` section, resolved to absolute paths.

use std::path::{Path, PathBuf};

/// How a module file is run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptKind {
    /// A WebAssembly component (sandboxed; ADR-0062).
    Wasm,
    /// A native module library (trusted; ADR-0061).
    Native,
    /// A `.graph.ron` Event graph, compiled to WASM (sandboxed; M4 4.10b). Loads after the
    /// other modules, so it can use their types.
    Graph,
}

/// One module file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptEntry {
    /// How it runs.
    pub kind: ScriptKind,
    /// The `.wasm` file, or the native library (without an extension, the platform's is added).
    pub path: PathBuf,
    /// A native module's source folder, checked for hidden state before every load.
    pub source: Option<PathBuf>,
}

impl ScriptEntry {
    /// The file to load: `path`, with the platform's library extension for a native module that
    /// names none.
    pub fn file(&self) -> PathBuf {
        let no_extension = self.path.extension().is_none();
        match (self.kind, no_extension) {
            (ScriptKind::Native, true) => self.path.with_extension(std::env::consts::DLL_EXTENSION),
            _ => self.path.clone(),
        }
    }
}

/// The module's settings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptsConfig {
    /// Module files, in load order.
    pub modules: Vec<ScriptEntry>,
    /// Derived-data cache for precompiled WASM (`<project>/.concordia/ddc`); `None` compiles
    /// every time.
    pub cache: Option<PathBuf>,
    /// Where native libraries are copied before loading (`<project>/.concordia/native`).
    pub copies: PathBuf,
    /// The session seed processor seeds derive from (`[sim] seed`).
    pub session_seed: u64,
}

impl ScriptsConfig {
    /// Defaults for a project at `root`: the DDC and the native copy folder under
    /// `.concordia/`.
    pub fn for_project(root: &Path, session_seed: u64) -> Self {
        Self {
            modules: Vec::new(),
            cache: Some(root.join(".concordia/ddc")),
            copies: root.join(".concordia/native"),
            session_seed,
        }
    }
}
