//! The `graph.*` methods (M4 4.10c, J18–J19): agents list nodes, edit graph files as undoable
//! transactions, check, compile and trace them, all as text over the Engine API.

use std::time::Instant;

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, EntityRef, Method, WorldSel};
use concordia_command::{Actor, EditOp};
use concordia_graph::{
    Diagnostic, Graph, GraphEdit, GraphKind, apply_edits, check, compile_event, fresh_id,
    new_graph, read_graph, write_graph,
};
use concordia_graph_script::{TickTrace, engine_library};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::reload::reload_graph;
use crate::state::{ScriptsState, graph_module_name};

/// Graph files end in this.
const EXTENSION: &str = ".graph.ron";

fn invalid(message: String) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidParams, message)
}

/// A graph file's text, if it exists.
fn read_file(engine: &Engine, path: &str) -> Result<Option<String>, ApiError> {
    let file = engine.project_file(path, EXTENSION)?;
    match std::fs::read_to_string(&file) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ApiError::new(ApiErrorKind::SceneIo, format!("{path}: {e}"))),
    }
}

fn parse(text: &str) -> Result<Graph, ApiError> {
    read_graph(text).map_err(|e| invalid(e.to_string()))
}

/// The graph at `path` (which must exist).
fn graph_at(engine: &Engine, path: &str) -> Result<Graph, ApiError> {
    let text = read_file(engine, path)?.ok_or_else(|| {
        invalid(format!("{path} doesn't exist")).suggest([
            "create it with graph.apply {\"path\": ..., \"kind\": \"Event\", \"edits\": [...]}"
                .to_owned(),
        ])
    })?;
    parse(&text)
}

/// List every node a graph can use in this project: the built-in core, the vector functions, and
/// for every registered component `get.<C>`, `set.<C>`, `get.<C>.<field>`, `set.<C>.<field>` and
/// for every signal `raise.<S>` and `on.<S>`, with their typed pins and docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphListNodes;

/// `graph.list_nodes` params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GraphListNodesParams {}

/// `graph.list_nodes` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphListNodesResult {
    /// Nodes by op: category, docs, purity, exec pins and typed data pins.
    pub nodes: Value,
}

impl Method for GraphListNodes {
    const NAME: &'static str = "graph.list_nodes";
    const MUTATES: bool = false;
    type Params = GraphListNodesParams;
    type Result = GraphListNodesResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        _: GraphListNodesParams,
    ) -> Result<GraphListNodesResult, ApiError> {
        Ok(GraphListNodesResult {
            nodes: engine_library(engine.registry()).to_json(),
        })
    }
}

/// Type-check a graph (a `.graph.ron` file, or its text) against this project's nodes. Each
/// diagnostic names the node and pin to fix, with the closest valid names as a suggestion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphCheck;

/// `graph.check` params: `path` or `text`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GraphCheckParams {
    /// Project-relative `.graph.ron` file.
    pub path: Option<String>,
    /// Graph text (instead of a file).
    pub text: Option<String>,
}

/// `graph.check` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphCheckResult {
    /// No diagnostics.
    pub ok: bool,
    /// What to fix.
    pub diagnostics: Vec<Diagnostic>,
}

impl Method for GraphCheck {
    const NAME: &'static str = "graph.check";
    const MUTATES: bool = false;
    type Params = GraphCheckParams;
    type Result = GraphCheckResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: GraphCheckParams,
    ) -> Result<GraphCheckResult, ApiError> {
        let graph = match (&p.path, &p.text) {
            (Some(path), None) => graph_at(engine, path)?,
            (None, Some(text)) => parse(text)?,
            _ => return Err(invalid("give exactly one of `path` or `text`".into())),
        };
        let diagnostics = check(&graph, &engine_library(engine.registry())).diagnostics;
        Ok(GraphCheckResult {
            ok: diagnostics.is_empty(),
            diagnostics,
        })
    }
}

/// Compile an Event graph file to WASM processors (without loading it): its diagnostics, or each
/// processor with what it reads and writes, its module size and the compile time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphCompile;

/// `graph.compile` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GraphCompileParams {
    /// Project-relative `.graph.ron` file.
    pub path: String,
}

/// One compiled processor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct CompiledProcessor {
    /// Processor name (`graph.<module>.tick`, `graph.<module>.on.<Signal>`).
    pub name: String,
    /// Components read only.
    pub reads: Vec<String>,
    /// Components written.
    pub writes: Vec<String>,
    /// Signal it runs on, if any.
    pub receives: Option<String>,
    /// WASM module bytes.
    pub wasm_bytes: u64,
}

/// `graph.compile` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphCompileResult {
    /// Compiled.
    pub ok: bool,
    /// Why not (checker and compiler diagnostics).
    pub diagnostics: Vec<Diagnostic>,
    /// The processors.
    pub processors: Vec<CompiledProcessor>,
    /// The `graph.<module>.Vars` component that attaches the graph.
    pub vars: Option<String>,
    /// Milliseconds to check and compile.
    pub compile_ms: f64,
}

impl Method for GraphCompile {
    const NAME: &'static str = "graph.compile";
    const MUTATES: bool = false;
    type Params = GraphCompileParams;
    type Result = GraphCompileResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: GraphCompileParams,
    ) -> Result<GraphCompileResult, ApiError> {
        let graph = graph_at(engine, &p.path)?;
        let name = graph_module_name(&engine.project_file(&p.path, EXTENSION)?);
        let start = Instant::now();
        let compiled = compile_event(
            &name,
            &graph,
            &engine_library(engine.registry()),
            engine.registry(),
        );
        let compile_ms = start.elapsed().as_secs_f64() * 1e3;
        Ok(match compiled {
            Ok(module) => GraphCompileResult {
                ok: true,
                diagnostics: Vec::new(),
                processors: module
                    .processors
                    .iter()
                    .map(|program| CompiledProcessor {
                        name: program.name.clone(),
                        reads: program.reads.clone(),
                        writes: program.writes.clone(),
                        receives: match &program.entry {
                            concordia_graph::runtime::Entry::Signal(s) => Some(s.clone()),
                            concordia_graph::runtime::Entry::Tick => None,
                        },
                        wasm_bytes: program.wasm().len() as u64,
                    })
                    .collect(),
                vars: Some(module.vars.name),
                compile_ms,
            },
            Err(diagnostics) => GraphCompileResult {
                ok: false,
                diagnostics,
                processors: Vec::new(),
                vars: None,
                compile_ms,
            },
        })
    }
}

/// Edit a graph file with structured edits (add/remove nodes, connect/disconnect data and exec
/// pins, set constants, declare variables), as one undoable transaction by your actor. A missing
/// file starts a new graph of `kind`. The result is written even when it doesn't check yet (build
/// graphs in steps); its diagnostics come back, and once it checks clean a graph listed in
/// `[scripts]` is reloaded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphApply;

/// `graph.apply` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GraphApplyParams {
    /// Project-relative `.graph.ron` file.
    pub path: String,
    /// The edits, applied in order, all or nothing.
    pub edits: Vec<GraphEdit>,
    /// A new graph's kind (default `Event`).
    pub kind: Option<GraphKind>,
    /// Who is editing (default: the session's actor).
    pub actor: Option<Actor>,
}

/// `graph.apply` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphApplyResult {
    /// The graph checks clean.
    pub ok: bool,
    /// What to fix next.
    pub diagnostics: Vec<Diagnostic>,
    /// Ids of the nodes added, in order.
    pub added: Vec<String>,
    /// The graph's module was reloaded (or joined) with this version.
    pub reloaded: bool,
    /// Why a reload failed (the edit itself was committed).
    pub reload_error: Option<String>,
}

impl Method for GraphApply {
    const NAME: &'static str = "graph.apply";
    const MUTATES: bool = true;
    type Params = GraphApplyParams;
    type Result = GraphApplyResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: GraphApplyParams,
    ) -> Result<GraphApplyResult, ApiError> {
        let file = engine.project_file(&p.path, EXTENSION)?;
        let before = read_file(engine, &p.path)?;
        let mut graph = match &before {
            Some(text) => parse(text)?,
            None => new_graph(p.kind.unwrap_or(GraphKind::Event), fresh_id()),
        };
        let added = apply_edits(&mut graph, &p.edits)
            .map_err(|e| invalid(e.to_string()).at(format!("/edits/{}", e.index)))?;
        let after = write_graph(&graph).map_err(|e| invalid(e.to_string()))?;
        if before.as_deref() != Some(after.as_str()) {
            let op = EditOp::Document {
                path: p.path.clone(),
                before,
                after: Some(after),
            };
            engine.commit_edits(ctx, "graph.apply", vec![op])?;
        }
        let diagnostics = check(&graph, &engine_library(engine.registry())).diagnostics;
        let ok = diagnostics.is_empty();
        let (reloaded, reload_error) = match ok {
            true => reload(engine, ctx, &graph_module_name(&file)),
            false => (false, None),
        };
        Ok(GraphApplyResult {
            ok,
            diagnostics,
            added,
            reloaded,
            reload_error,
        })
    }
}

/// Reloads module `name` if it is configured: (reloaded, why not).
fn reload(engine: &mut Engine, ctx: &CallCtx, name: &str) -> (bool, Option<String>) {
    let Some(handle) = engine.state::<ScriptsState>().map(|s| s.0.clone()) else {
        return (false, None);
    };
    let actor = ctx.actor().map(ToString::to_string).unwrap_or_default();
    let mut scripts = handle.lock();
    match reload_graph(engine, &mut scripts, name, &actor) {
        Ok(reloaded) => (reloaded, None),
        Err(e) => (false, Some(e.message)),
    }
}

/// Step play `ticks` ticks while graph module `module` records, for `entity`, each tick's
/// executed statement nodes (in exec order) and the values on every data pin they used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphTrace;

/// `graph.trace` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GraphTraceParams {
    /// The graph module (its file name without `.graph.ron`).
    pub module: String,
    /// The entity to follow.
    pub entity: EntityRef,
    /// Ticks to step (1 to 600).
    pub ticks: u32,
}

/// `graph.trace` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphTraceResult {
    /// One entry per processor run on the entity, in order.
    pub runs: Vec<TickTrace>,
    /// The play world's hash after stepping.
    pub world_hash: String,
}

impl Method for GraphTrace {
    const NAME: &'static str = "graph.trace";
    const MUTATES: bool = false;
    type Params = GraphTraceParams;
    type Result = GraphTraceResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: GraphTraceParams,
    ) -> Result<GraphTraceResult, ApiError> {
        if !(1..=600).contains(&p.ticks) {
            return Err(invalid(format!("ticks must be 1 to 600, not {}", p.ticks)).at("/ticks"));
        }
        let entity = p.entity.resolve(engine.world(WorldSel::edit)?)?;
        let handle = engine
            .state::<ScriptsState>()
            .map(|s| s.0.clone())
            .ok_or_else(|| invalid("the scripts module is not running".into()))?;
        // Clone the module out: stepping may start play, which locks the scripts state.
        let graph = {
            let scripts = handle.lock();
            scripts.graphs.get(&p.module).cloned().ok_or_else(|| {
                let known: Vec<&String> = scripts.graphs.keys().collect();
                invalid(format!(
                    "no loaded graph module {:?}; loaded: {known:?}",
                    p.module
                ))
                .at("/module")
            })?
        };
        graph.start_trace(entity);
        let stepped = engine.step_play(p.ticks);
        let runs = graph.take_trace();
        Ok(GraphTraceResult {
            runs,
            world_hash: stepped?.to_string(),
        })
    }
}
