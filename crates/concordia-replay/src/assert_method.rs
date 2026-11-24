//! `assert.eval`: playtest assertions decided over a replay, a `.playtest.ron` scenario, or the
//! current play world (M3 phase 3.2b, ADR-0084).

use std::collections::BTreeMap;

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method, WorldSel};
use concordia_command::Actor;
use concordia_input::{InjectedEvent, InputSampler};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::eval::Eval;
use crate::format::read_replay;
use crate::methods::{EXTENSION, actor_name};
use crate::monitor::Monitor;
use crate::record::tick_of;
use crate::runner::prepare;
use crate::verify::mismatch_error;

/// The playtest file extension.
pub(crate) const PLAYTEST: &str = ".playtest.ron";

/// Check playtest assertions, e.g. "within 600 ticks: distance(tag(\"player\"), tag(\"door\")) < 2m": over a replay (its inputs on a fresh play world), a .playtest.ron scenario, or live on the current play world (the predicate, now). Parse and type errors name the fix. Never changes the scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AssertEval;

/// Where assertions are checked.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum AssertSource {
    /// The current play world, now (its predicate only).
    #[default]
    live,
    /// A replay's inputs on a fresh play world (path: the .replay.ron).
    replay,
    /// A .playtest.ron scenario (path), with its own assertions.
    scenario,
}

/// `assert.eval` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct AssertEvalParams {
    /// Assertions in text (a scenario adds its own).
    #[serde(default)]
    pub assertions: Vec<String>,
    /// live (default), replay or scenario.
    pub source: Option<AssertSource>,
    /// With replay or scenario: the file.
    pub path: Option<String>,
    /// Who is asking (log only).
    pub actor: Option<Actor>,
}

/// How an assertion ended.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum AssertStatus {
    /// It held.
    passed,
    /// It didn't.
    failed,
    /// The run ended before it was decided.
    undecided,
    /// A value couldn't be read (see witness.error).
    error,
}

/// One assertion's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AssertOutcome {
    /// The assertion, in canonical text.
    pub assertion: String,
    /// passed, failed, undecided or error.
    pub status: AssertStatus,
    /// The tick it was decided on.
    pub decided_at: Option<u64>,
    /// Values behind the verdict: the comparison's left side by its text; for a within that
    /// ran out, the closest value and its tick.
    pub witness: BTreeMap<String, Value>,
}

/// `assert.eval` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AssertEvalResult {
    /// Every assertion passed.
    pub ok: bool,
    /// One result per assertion, in order.
    pub results: Vec<AssertOutcome>,
    /// Ticks simulated (0 for live).
    pub ticks: u64,
}

/// A `.playtest.ron` suite: a scene, inputs (or a replay's), assertions and a tick budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Playtest {
    /// The scene it runs on (must be the open one).
    pub scene: String,
    /// Ticks to run at most.
    #[reflect(range(min = 1.0, max = 10000000.0))]
    pub max_ticks: u64,
    /// Input events by tick.
    #[serde(default)]
    pub inputs: Vec<InjectedEvent>,
    /// A replay whose inputs to use instead.
    #[serde(default)]
    pub replay: Option<String>,
    /// The assertions.
    pub assertions: Vec<String>,
}

impl Method for AssertEval {
    const NAME: &'static str = "assert.eval";
    const MUTATES: bool = false;
    type Params = AssertEvalParams;
    type Result = AssertEvalResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: AssertEvalParams,
    ) -> Result<AssertEvalResult, ApiError> {
        let result = match p.source.unwrap_or_default() {
            AssertSource::live => live(engine, &p.assertions)?,
            AssertSource::replay => over_replay(engine, &p.assertions, &path(&p)?)?,
            AssertSource::scenario => scenario(engine, &p.assertions, &path(&p)?)?,
        };
        tracing::info!(
            target: "concordia::replay",
            actor = actor_name(ctx),
            ok = result.ok,
            ticks = result.ticks,
            "assertions evaluated"
        );
        Ok(result)
    }
}

fn path(p: &AssertEvalParams) -> Result<String, ApiError> {
    p.path.clone().ok_or_else(|| {
        ApiError::new(
            ApiErrorKind::InvalidParams,
            "replay and scenario need a path",
        )
        .at("/path")
    })
}

/// Parses and checks each assertion; the first problem names its index and the fix.
fn monitors(engine: &Engine, texts: &[String]) -> Result<Vec<Monitor>, ApiError> {
    if texts.is_empty() {
        return Err(
            ApiError::new(ApiErrorKind::InvalidParams, "no assertions to check").at("/assertions"),
        );
    }
    texts
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let bad = |e: String| {
                ApiError::new(ApiErrorKind::InvalidParams, format!("assertion {i}: {e}"))
                    .at(format!("/assertions/{i}"))
            };
            let parsed =
                concordia_assert::parse(text).map_err(|e| bad(format!("{e} in {text:?}")))?;
            let checked =
                concordia_assert::check(&parsed, engine.registry(), engine.sim_rate().hz())
                    .map_err(|e| bad(e.to_string()))?;
            Ok(Monitor::new(checked))
        })
        .collect()
}

fn sample(engine: &Engine, monitors: &mut [Monitor]) -> Result<(), ApiError> {
    let world = engine.world(WorldSel::play)?;
    let tick = tick_of(world);
    let eval = Eval {
        world,
        rate_hz: f64::from(engine.sim_rate().hz()),
    };
    for m in monitors.iter_mut() {
        let s = eval.sample(&m.assertion.pred);
        m.observe(tick, s);
    }
    Ok(())
}

/// Samples tick 0, then steps one tick at a time until every assertion is decided or `until`.
fn run(
    engine: &mut Engine,
    mut monitors: Vec<Monitor>,
    until: u64,
) -> Result<AssertEvalResult, ApiError> {
    sample(engine, &mut monitors)?;
    let mut tick = tick_of(engine.world(WorldSel::play)?);
    while tick < until && !monitors.iter().all(Monitor::decided) {
        engine.step_play(1)?;
        tick += 1;
        sample(engine, &mut monitors)?;
    }
    for m in &mut monitors {
        m.finish(tick);
    }
    Ok(result(monitors, tick))
}

fn result(monitors: Vec<Monitor>, ticks: u64) -> AssertEvalResult {
    let results: Vec<AssertOutcome> = monitors.into_iter().filter_map(|m| m.outcome).collect();
    AssertEvalResult {
        ok: results.iter().all(|r| r.status == AssertStatus::passed),
        results,
        ticks,
    }
}

/// The predicate on the current play world (the edit world when not playing), now.
fn live(engine: &mut Engine, texts: &[String]) -> Result<AssertEvalResult, ApiError> {
    let monitors = monitors(engine, texts)?;
    let which = if engine.world(WorldSel::play).is_ok() {
        WorldSel::play
    } else {
        WorldSel::edit
    };
    let world = engine.world(which)?;
    let tick = tick_of(world);
    let eval = Eval {
        world,
        rate_hz: f64::from(engine.sim_rate().hz()),
    };
    let results = monitors
        .into_iter()
        .map(|m| {
            let (status, witness) = match eval.sample(&m.assertion.pred) {
                Ok(s) => (
                    if s.holds {
                        AssertStatus::passed
                    } else {
                        AssertStatus::failed
                    },
                    crate::eval::witness(&m.assertion.pred, &s),
                ),
                Err(e) => (
                    AssertStatus::error,
                    BTreeMap::from([("error".into(), Value::from(e))]),
                ),
            };
            AssertOutcome {
                assertion: m.assertion.to_string(),
                status,
                decided_at: Some(tick),
                witness,
            }
        })
        .collect::<Vec<_>>();
    Ok(AssertEvalResult {
        ok: results.iter().all(|r| r.status == AssertStatus::passed),
        results,
        ticks: 0,
    })
}

/// The assertions over a replay's inputs on a fresh play world.
fn over_replay(
    engine: &mut Engine,
    texts: &[String],
    rel: &str,
) -> Result<AssertEvalResult, ApiError> {
    let monitors = monitors(engine, texts)?;
    let file = engine.project_file(rel, EXTENSION)?;
    let text = std::fs::read_to_string(&file)
        .map_err(|e| ApiError::new(ApiErrorKind::SceneIo, format!("{rel}: {e}")).at("/path"))?;
    let replay = read_replay(&text).map_err(|e| {
        ApiError::new(ApiErrorKind::InvalidParams, format!("{rel}: {e}")).at("/path")
    })?;
    let found = prepare(engine, &replay)?;
    if !found.is_empty() {
        return Err(mismatch_error(
            "the replay was recorded differently",
            &found,
        ));
    }
    run(engine, monitors, replay.ticks)
}

/// A `.playtest.ron` scenario on the open scene.
fn scenario(
    engine: &mut Engine,
    extra: &[String],
    rel: &str,
) -> Result<AssertEvalResult, ApiError> {
    let file = engine.project_file(rel, PLAYTEST)?;
    let text = std::fs::read_to_string(&file)
        .map_err(|e| ApiError::new(ApiErrorKind::SceneIo, format!("{rel}: {e}")).at("/path"))?;
    let playtest: Playtest = concordia_scene::read_document(&text).map_err(|e| {
        ApiError::new(ApiErrorKind::InvalidParams, format!("{rel}: {e}")).at("/path")
    })?;
    if engine.scene_path() != Some(playtest.scene.as_str()) {
        return Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("{rel} runs on {}; open it first", playtest.scene),
        )
        .suggest([format!("scene.open {{\"path\": \"{}\"}}", playtest.scene)]));
    }
    let texts: Vec<String> = playtest.assertions.iter().chain(extra).cloned().collect();
    if let Some(replay) = &playtest.replay {
        let mut out = over_replay(engine, &texts, replay)?;
        out.ticks = out.ticks.min(playtest.max_ticks);
        return Ok(out);
    }
    let monitors = monitors(engine, &texts)?;
    let world = engine.restart_play(None)?;
    let sampler = world.resource_mut::<InputSampler>().ok_or_else(|| {
        ApiError::new(
            ApiErrorKind::InvalidParams,
            "scenario inputs need an [input] map",
        )
    })?;
    for e in &playtest.inputs {
        let tick = e.tick.unwrap_or(0);
        sampler
            .check(tick, &e.event)
            .and_then(|()| sampler.push(tick, e.event.clone()))
            .map_err(|err| ApiError::new(ApiErrorKind::InvalidParams, format!("{rel}: {err}")))?;
    }
    run(engine, monitors, playtest.max_ticks)
}
