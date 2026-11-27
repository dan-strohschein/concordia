//! Loading graphs: check, compile, and (in WASM mode) precompile each processor.

use std::path::Path;
use std::sync::Arc;

use concordia_graph::{NodeLibrary, compile_event, read_graph};
use concordia_reflect::TypeRegistry;
use concordia_script::{ModuleSource, ScriptError, ScriptModule};
use concordia_script_wasm::WasmConfig;

use crate::module::GraphModule;

/// How a graph's processors run.
#[derive(Debug, Clone)]
pub enum Mode {
    /// Compiled to WASM, under the scripts' Wasmtime settings and fuel.
    Wasm(WasmConfig),
    /// The IR interpreter (the oracle and the debugger), with an optional instruction budget.
    Interpret {
        /// Instructions per run, at most.
        budget: Option<u64>,
    },
}

impl Default for Mode {
    fn default() -> Self {
        Self::Wasm(WasmConfig::default())
    }
}

/// Loads `.graph.ron` Event graphs as script modules.
pub struct GraphBackend {
    mode: Mode,
    engine: Option<wasmtime::Engine>,
}

impl std::fmt::Debug for GraphBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphBackend")
            .field("mode", &self.mode)
            .finish()
    }
}

impl GraphBackend {
    /// A backend running graphs in `mode`.
    pub fn new(mode: Mode) -> Result<Self, ScriptError> {
        let engine = match &mode {
            Mode::Wasm(config) => {
                let config = concordia_script_wasm::engine_config(config);
                Some(
                    wasmtime::Engine::new(&config)
                        .map_err(|e| ScriptError::Load(format!("engine: {e}")))?,
                )
            }
            Mode::Interpret { .. } => None,
        };
        Ok(Self { mode, engine })
    }

    /// Loads a graph file (the module is named after it) or graph text.
    pub fn load(
        &self,
        source: &ModuleSource,
        registry: &TypeRegistry,
    ) -> Result<Arc<dyn ScriptModule>, ScriptError> {
        match source {
            ModuleSource::File(path) => {
                let text = std::fs::read_to_string(path)
                    .map_err(|e| ScriptError::Load(format!("{}: {e}", path.display())))?;
                self.load_text(&module_name(path), &text, registry)
            }
            ModuleSource::Bytes(bytes) => {
                let text = std::str::from_utf8(bytes)
                    .map_err(|e| ScriptError::Load(format!("graph text: {e}")))?;
                self.load_text("graph", text, registry)
            }
            ModuleSource::Linked(_) => Err(ScriptError::Unsupported {
                backend: "graph",
                source_kind: "a linked module".into(),
            }),
        }
    }

    /// Loads graph text as module `name`; a graph that doesn't check or compile fails with its
    /// diagnostics.
    pub fn load_text(
        &self,
        name: &str,
        text: &str,
        registry: &TypeRegistry,
    ) -> Result<Arc<dyn ScriptModule>, ScriptError> {
        let module: Arc<dyn ScriptModule> = self.load_graph(name, text, registry)?;
        Ok(module)
    }

    /// As [`GraphBackend::load_text`], as the concrete module (for traces).
    pub fn load_graph(
        &self,
        name: &str,
        text: &str,
        registry: &TypeRegistry,
    ) -> Result<Arc<GraphModule>, ScriptError> {
        if name.is_empty() {
            return Err(ScriptError::Load("a graph module needs a name".into()));
        }
        let graph = read_graph(text).map_err(|e| ScriptError::Load(e.to_string()))?;
        let module = compile_event(name, &graph, &engine_library(registry), registry).map_err(
            |diagnostics| {
                let lines: Vec<String> = diagnostics
                    .iter()
                    .map(|d| match &d.suggestion {
                        Some(s) => format!(
                            "{:?} at {:?}/{:?}: {} (did you mean {s}?)",
                            d.code, d.node_id, d.pin, d.message
                        ),
                        None => {
                            format!("{:?} at {:?}/{:?}: {}", d.code, d.node_id, d.pin, d.message)
                        }
                    })
                    .collect();
                ScriptError::Load(format!("graph {name}:\n{}", lines.join("\n")))
            },
        )?;
        Ok(Arc::new(GraphModule::new(
            module,
            &self.mode,
            self.engine.as_ref(),
        )?))
    }
}

/// The node library over a registry's components (terra's registrations) and signals.
pub fn engine_library(registry: &TypeRegistry) -> NodeLibrary {
    NodeLibrary::build(registry, |r| {
        r.data::<concordia_terra::ReflectComponent>().is_some()
    })
}

/// `enemy.graph.ron` → `enemy`.
fn module_name(path: &Path) -> String {
    let file = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    file.trim_end_matches(".graph.ron")
        .trim_end_matches(".ron")
        .to_owned()
}
