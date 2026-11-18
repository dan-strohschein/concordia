//! `sim.play`, `sim.pause`, `sim.stop`, `sim.step` (play world only; ADR-0032).

use concordia_core::rng::SimRng;
use concordia_core::time::TickRate;
use concordia_ecs::{SimClock, SimLoop};
use concordia_reflect::Reflect;
use concordia_scene::builtin::headless_schedule;
use serde::{Deserialize, Serialize};

use super::Empty;
use crate::engine::{Engine, Play, internal};
use crate::error::{ApiError, ApiErrorKind};
use crate::method::{CallCtx, Method, MethodInfo};

/// Run the simulation in real time on a play world cloned from the edit world (the scene is never changed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SimPlay;

/// `sim.play` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SimPlayParams {
    /// Fixed tick rate (default 60).
    #[reflect(range(min = 1.0, max = 1000.0))]
    pub rate_hz: Option<u32>,
}

/// Pause the play world (keeps it for reads and sim.step).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SimPause;

/// Discard the play world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SimStop;

/// `sim.play` / `sim.pause` / `sim.stop` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SimState {
    /// "playing", "paused" or "stopped".
    pub state: String,
    /// Ticks simulated in the play world.
    pub tick: u64,
}

/// Advance the play world by exactly N fixed ticks (creating it, paused, if needed) and return its state hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SimStep;

/// `sim.step` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SimStepParams {
    /// Ticks to simulate.
    #[reflect(range(min = 1.0, max = 1000000.0))]
    pub ticks: u32,
}

/// `sim.step` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SimStepResult {
    /// Ticks simulated so far.
    pub tick: u64,
    /// Play world state hash (identical across thread counts).
    pub world_hash: String,
}

impl Method for SimPlay {
    const NAME: &'static str = "sim.play";
    const MUTATES: bool = false;
    type Params = SimPlayParams;
    type Result = SimState;

    fn call(engine: &mut Engine, ctx: &CallCtx, p: SimPlayParams) -> Result<SimState, ApiError> {
        let rate = p
            .rate_hz
            .map(TickRate::from_hz)
            .transpose()
            .map_err(internal)?;
        let play = ensure_play(engine, rate)?;
        play.playing = true;
        play.last = ctx.now;
        Ok(state(engine))
    }
}

impl Method for SimPause {
    const NAME: &'static str = "sim.pause";
    const MUTATES: bool = false;
    type Params = Empty;
    type Result = SimState;

    fn call(engine: &mut Engine, _: &CallCtx, _: Empty) -> Result<SimState, ApiError> {
        let play = engine
            .play
            .as_mut()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NotPlaying))?;
        play.playing = false;
        Ok(state(engine))
    }
}

impl Method for SimStop {
    const NAME: &'static str = "sim.stop";
    const MUTATES: bool = false;
    type Params = Empty;
    type Result = SimState;

    fn call(engine: &mut Engine, _: &CallCtx, _: Empty) -> Result<SimState, ApiError> {
        let tick = tick_of(engine);
        engine.play = None;
        Ok(SimState {
            state: "stopped".into(),
            tick,
        })
    }
}

impl Method for SimStep {
    const NAME: &'static str = "sim.step";
    const MUTATES: bool = false;
    type Params = SimStepParams;
    type Result = SimStepResult;

    fn call(engine: &mut Engine, _: &CallCtx, p: SimStepParams) -> Result<SimStepResult, ApiError> {
        ensure_play(engine, None)?;
        let Engine { play, jobs, .. } = engine;
        let play = play
            .as_mut()
            .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NotPlaying))?;
        play.sim.step(&mut play.world, jobs, p.ticks);
        let world_hash = play.world.state_hash().map_err(internal)?.to_string();
        Ok(SimStepResult {
            tick: tick_of(engine),
            world_hash,
        })
    }
}

/// The play world, cloned from the edit world on first use; `rate` replaces the tick rate.
fn ensure_play(engine: &mut Engine, rate: Option<TickRate>) -> Result<&mut Play, ApiError> {
    if engine.play.is_none() {
        let mut world = engine.scene()?.world.clone();
        world.insert_resource(SimRng::new(engine.config.sim_seed));
        let sim = SimLoop::new(rate.unwrap_or(engine.config.sim_rate), headless_schedule());
        engine.play = Some(Play {
            world,
            sim,
            playing: false,
            last: std::time::Instant::now(),
        });
    }
    let play = engine
        .play
        .as_mut()
        .ok_or_else(|| ApiError::from_kind(ApiErrorKind::NotPlaying))?;
    if let Some(rate) = rate {
        play.sim = SimLoop::new(rate, headless_schedule());
        if let Some(clock) = play.world.resource_mut::<SimClock>() {
            clock.rate = rate;
        }
    }
    Ok(play)
}

fn tick_of(engine: &Engine) -> u64 {
    engine
        .play
        .as_ref()
        .and_then(|p| p.world.resource::<SimClock>())
        .map_or(0, |c| c.tick.0)
}

fn state(engine: &Engine) -> SimState {
    let state = match engine.play.as_ref() {
        Some(p) if p.playing => "playing",
        Some(_) => "paused",
        None => "stopped",
    };
    SimState {
        state: state.into(),
        tick: tick_of(engine),
    }
}

pub(crate) fn methods() -> Vec<MethodInfo> {
    vec![
        MethodInfo::of::<SimPlay>(),
        MethodInfo::of::<SimPause>(),
        MethodInfo::of::<SimStop>(),
        MethodInfo::of::<SimStep>(),
    ]
}
