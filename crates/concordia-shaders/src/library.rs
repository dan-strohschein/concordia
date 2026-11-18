//! Named shader modules that recompile when their files change, keeping the last good version on errors.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::ShaderCompiler;
use crate::deps::dependencies;
use crate::error::ShaderError;
use crate::request::{CompileRequest, ShaderModule};

/// What happened to one module after a file change.
#[derive(Debug, Clone, PartialEq)]
pub enum ReloadOutcome {
    /// Recompiled; callers rebuild pipelines that use it (between frames).
    Reloaded(String),
    /// The edit does not compile; the previous module stays in use.
    Failed(String, ShaderError),
}

struct Entry {
    request: CompileRequest,
    module: ShaderModule,
    files: Vec<PathBuf>,
}

/// Compiled modules by name, with hot reload.
pub struct ShaderLibrary {
    compiler: Box<dyn ShaderCompiler>,
    entries: BTreeMap<String, Entry>,
}

impl std::fmt::Debug for ShaderLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShaderLibrary")
            .field("modules", &self.entries.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ShaderLibrary {
    /// An empty library compiling with `compiler`.
    pub fn new(compiler: Box<dyn ShaderCompiler>) -> Self {
        Self {
            compiler,
            entries: BTreeMap::new(),
        }
    }

    /// Compiles `request` and stores it as `name` (replacing any previous one).
    pub fn load(
        &mut self,
        name: &str,
        request: CompileRequest,
    ) -> Result<&ShaderModule, ShaderError> {
        let module = self.compiler.compile(&request)?;
        let files = dependencies(&request.path, &request.include_dirs)?;
        let entry = self.entries.entry(name.to_owned()).insert_entry(Entry {
            request,
            module,
            files,
        });
        Ok(&entry.into_mut().module)
    }

    /// The current (last good) module.
    pub fn get(&self, name: &str) -> Option<&ShaderModule> {
        self.entries.get(name).map(|e| &e.module)
    }

    /// Recompiles every module depending on a changed file; failures keep the previous module and are logged.
    pub fn on_files_changed(&mut self, changed: &[PathBuf]) -> Vec<ReloadOutcome> {
        let changed: Vec<PathBuf> = changed
            .iter()
            .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
            .collect();
        let mut outcomes = Vec::new();
        for (name, entry) in &mut self.entries {
            if !entry.files.iter().any(|f| changed.contains(f)) {
                continue;
            }
            match self.compiler.compile(&entry.request) {
                Ok(module) => {
                    entry.module = module;
                    if let Ok(files) =
                        dependencies(&entry.request.path, &entry.request.include_dirs)
                    {
                        entry.files = files;
                    }
                    outcomes.push(ReloadOutcome::Reloaded(name.clone()));
                }
                Err(e) => {
                    tracing::warn!(target: "concordia::shaders", module = %name, error = %e, "shader reload failed; keeping the last good version");
                    outcomes.push(ReloadOutcome::Failed(name.clone(), e));
                }
            }
        }
        outcomes
    }
}
