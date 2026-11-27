//! The module's state: its backends and the loaded modules.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use concordia_graph_script::{GraphBackend, Mode};
use concordia_reflect::TypeRegistry;
use concordia_script::{ModuleSource, ScriptBackend, ScriptHost, ScriptModule, TestBackend};
use concordia_script_native::{NativeBackend, check_module_source};
use concordia_script_wasm::{WasmBackend, WasmConfig};

use crate::config::{ScriptEntry, ScriptKind, ScriptsConfig};

/// Where a loaded module comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    /// A file on disk.
    File(ScriptEntry),
    /// A module linked into the engine, by name.
    Linked(String),
}

impl Source {
    /// The backend's name, for reports.
    pub(crate) fn backend(&self) -> &'static str {
        match self {
            Self::File(entry) => match entry.kind {
                ScriptKind::Wasm => "wasm",
                ScriptKind::Native => "native",
                ScriptKind::Graph => "graph",
            },
            Self::Linked(_) => "linked",
        }
    }
}

/// One loaded module.
pub(crate) struct Loaded {
    pub(crate) source: Source,
    pub(crate) host: ScriptHost,
    /// BLAKE3 of the loaded build (the file, or a linked module's description), as hex.
    pub(crate) hash: String,
}

/// A module build loaded from its source, with its content hash.
pub(crate) struct Build {
    pub(crate) module: Arc<dyn ScriptModule>,
    pub(crate) hash: String,
}

/// Backends and loaded modules.
pub(crate) struct Scripts {
    pub(crate) config: ScriptsConfig,
    pub(crate) linked: TestBackend,
    pub(crate) native: NativeBackend,
    wasm: Option<WasmBackend>,
    graph: Option<GraphBackend>,
    pub(crate) linked_names: Vec<String>,
    pub(crate) loaded: Vec<Loaded>,
    /// Sources that failed to load, with why (reported by `script.reload`).
    pub(crate) failed: Vec<(Source, String)>,
}

/// Shared access to [`Scripts`] (the module, its play setup and its methods).
#[derive(Clone)]
pub(crate) struct Handle(Arc<Mutex<Scripts>>);

impl Handle {
    pub(crate) fn new(config: ScriptsConfig, linked: TestBackend) -> Self {
        let native = NativeBackend::new(config.copies.clone());
        Self(Arc::new(Mutex::new(Scripts {
            config,
            linked,
            native,
            wasm: None,
            graph: None,
            linked_names: Vec::new(),
            loaded: Vec::new(),
            failed: Vec::new(),
        })))
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Scripts> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The engine-lifetime state the module registers, so its methods can find it.
pub(crate) struct ScriptsState(pub(crate) Handle);

impl Scripts {
    /// Loads the current build of `source` from disk (checking a native module's source
    /// first; compiling a graph against `registry`) or from the linked modules, and hashes it.
    pub(crate) fn load(
        &mut self,
        source: &Source,
        registry: &TypeRegistry,
    ) -> Result<Build, String> {
        match source {
            Source::Linked(name) => {
                let module = self
                    .linked
                    .load(&ModuleSource::Linked(name.clone()))
                    .map_err(|e| e.to_string())?;
                let desc = module.describe().map_err(|e| format!("describe: {e}"))?;
                let json = serde_json::to_vec(&desc).map_err(|e| e.to_string())?;
                Ok(Build {
                    module,
                    hash: blake3::hash(&json).to_hex().to_string(),
                })
            }
            Source::File(entry) => {
                let file = entry.file();
                let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
                let module = self.load_file(entry, registry)?;
                Ok(Build {
                    module,
                    hash: blake3::hash(&bytes).to_hex().to_string(),
                })
            }
        }
    }

    fn load_file(
        &mut self,
        entry: &ScriptEntry,
        registry: &TypeRegistry,
    ) -> Result<Arc<dyn ScriptModule>, String> {
        let file = entry.file();
        let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", file.display());
        match entry.kind {
            ScriptKind::Wasm => self
                .wasm()?
                .load(&ModuleSource::File(file.clone()))
                .map_err(|e| fail(&e)),
            ScriptKind::Graph => self
                .graph()?
                .load(&ModuleSource::File(file.clone()), registry)
                .map_err(|e| fail(&e)),
            ScriptKind::Native => {
                if let Some(source) = &entry.source {
                    check_module_source(source).map_err(|e| fail(&e))?;
                }
                self.native
                    .load(&ModuleSource::File(file.clone()))
                    .map_err(|e| fail(&e))
            }
        }
    }

    /// The graph backend, created on first use.
    fn graph(&mut self) -> Result<&GraphBackend, String> {
        if self.graph.is_none() {
            self.graph = Some(GraphBackend::new(Mode::default()).map_err(|e| e.to_string())?);
        }
        self.graph
            .as_ref()
            .ok_or_else(|| "no graph backend".to_owned())
    }

    /// The WASM backend, created on first use.
    fn wasm(&mut self) -> Result<&WasmBackend, String> {
        if self.wasm.is_none() {
            let mut backend = WasmBackend::new(WasmConfig::default()).map_err(|e| e.to_string())?;
            if let Some(dir) = &self.config.cache {
                backend = backend.with_cache(concordia_core::ddc::Ddc::new(dir.clone()));
            }
            self.wasm = Some(backend);
        }
        self.wasm
            .as_ref()
            .ok_or_else(|| "no WASM backend".to_owned())
    }

    /// Every configured source: script files in order, linked modules, then graphs (which may
    /// use the others' types).
    pub(crate) fn sources(&self) -> Vec<Source> {
        let files = |graphs: bool| {
            self.config
                .modules
                .iter()
                .filter(move |m| (m.kind == ScriptKind::Graph) == graphs)
                .cloned()
                .map(Source::File)
        };
        let linked = self.linked_names.iter().cloned().map(Source::Linked);
        files(false).chain(linked).chain(files(true)).collect()
    }
}
