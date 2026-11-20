//! `replay.record` and `replay.run`.

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method, MethodInfo};
use concordia_command::Actor;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::format::read_replay;
use crate::recorder::{start, stop};
use crate::runner::{prepare, run};
use crate::verify::mismatch_error;

/// The replay file extension.
pub(crate) const EXTENSION: &str = ".replay.ron";

/// A `replay.record` action.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum RecordAction {
    /// Record a fresh play world.
    #[default]
    start,
    /// Write the replay.
    stop,
}

/// Record play to a .replay.ron file: start begins a fresh play world from the open scene (drive it with input.inject and sim.step), stop writes the file. Never changes the scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ReplayRecord;

/// `replay.record` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ReplayRecordParams {
    /// start (default) or stop.
    pub action: Option<RecordAction>,
    /// With start: e.g. tests/replays/jump.replay.ron.
    pub path: Option<String>,
    /// Ticks between checkpoint hashes (default 60).
    #[reflect(range(min = 1.0, max = 1000000.0))]
    pub checkpoint_every: Option<u32>,
    /// Who is recording (log and replay header).
    pub actor: Option<Actor>,
}

/// `replay.record` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ReplayRecordResult {
    /// recording or stopped.
    pub state: String,
    /// The replay file.
    pub path: String,
    /// Ticks recorded.
    pub tick: u64,
    /// Ticks with input changes.
    pub inputs: u32,
    /// Checkpoints taken.
    pub checkpoints: u32,
}

/// How `replay.run` checks.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum RunMode {
    /// Compare checkpoints; stop at the first divergence.
    #[default]
    verify,
    /// Report new checkpoint hashes.
    resimulate,
}

/// Replay a .replay.ron file headless (faster than real time) on a fresh play world from the open scene and check its checkpoint hashes; the play world stays for queries. Never changes the scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ReplayRun;

/// `replay.run` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ReplayRunParams {
    /// The replay file.
    pub path: String,
    /// verify (default) or resimulate.
    pub mode: Option<RunMode>,
    /// Run even if the engine, scene, seed or code differ.
    pub force: Option<bool>,
    /// Who is asking (log only).
    pub actor: Option<Actor>,
}

/// A header field that differs from the recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Mismatch {
    /// e.g. scene_hash or modules.
    pub field: String,
    /// Recorded value.
    pub recorded: String,
    /// This engine's value.
    pub current: String,
}

/// The first checkpoint that differs from the recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Divergence {
    /// Its tick.
    pub tick: u64,
    /// The checkpoint before it, which matched.
    pub last_matching_tick: Option<u64>,
    /// Differing parts: components, @entities, @clock, @rng, @signals.
    pub parts: Vec<String>,
    /// Recorded world hash.
    pub expected: String,
    /// Replayed world hash.
    pub actual: String,
}

/// A checkpoint's world hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TickHash {
    /// Tick.
    pub tick: u64,
    /// World hash.
    pub world_hash: String,
}

/// `replay.run` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ReplayRunResult {
    /// No divergence.
    pub ok: bool,
    /// Ticks simulated.
    pub ticks: u64,
    /// Checkpoints that matched (verify).
    pub matched: u32,
    /// The first divergence (verify).
    pub divergence: Option<Divergence>,
    /// Header differences run anyway (force).
    pub mismatches: Vec<Mismatch>,
    /// New checkpoint hashes (resimulate).
    pub checkpoints: Vec<TickHash>,
    /// Final play-world hash.
    pub world_hash: String,
    /// Wall-clock milliseconds.
    pub elapsed_ms: f64,
}

impl Method for ReplayRecord {
    const NAME: &'static str = "replay.record";
    const MUTATES: bool = false;
    type Params = ReplayRecordParams;
    type Result = ReplayRecordResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: ReplayRecordParams,
    ) -> Result<ReplayRecordResult, ApiError> {
        match p.action.unwrap_or_default() {
            RecordAction::start => start(engine, ctx, &p),
            RecordAction::stop => stop(engine, ctx),
        }
    }
}

impl Method for ReplayRun {
    const NAME: &'static str = "replay.run";
    const MUTATES: bool = false;
    type Params = ReplayRunParams;
    type Result = ReplayRunResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: ReplayRunParams,
    ) -> Result<ReplayRunResult, ApiError> {
        let file = engine.project_file(&p.path, EXTENSION)?;
        let text = std::fs::read_to_string(&file).map_err(|e| {
            ApiError::new(ApiErrorKind::SceneIo, format!("{}: {e}", p.path)).at("/path")
        })?;
        let replay = read_replay(&text).map_err(|e| {
            ApiError::new(ApiErrorKind::InvalidParams, format!("{}: {e}", p.path)).at("/path")
        })?;
        let mode = p.mode.unwrap_or_default();
        let found = prepare(engine, &replay)?;
        if !found.is_empty() && !p.force.unwrap_or(false) {
            return Err(mismatch_error("the replay was recorded differently", &found)
                .suggest(["open the scene and build it was recorded with, or pass \"force\": true to run it anyway".to_owned()]));
        }
        let result = run(engine, &replay, mode, found)?;
        tracing::info!(
            target: "concordia::replay",
            actor = actor_name(ctx),
            path = %p.path,
            mode = ?mode,
            ok = result.ok,
            ticks = result.ticks,
            "replay run"
        );
        Ok(result)
    }
}

pub(crate) fn actor_name(ctx: &CallCtx) -> String {
    ctx.actor
        .as_ref()
        .map_or_else(|| "none".into(), Actor::to_string)
}

/// The module's methods.
pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<ReplayRecord>(),
        MethodInfo::of::<ReplayRun>(),
    ]
}
