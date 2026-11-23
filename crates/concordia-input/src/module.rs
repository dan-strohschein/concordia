//! The input engine module: samples input every tick and serves `input.inject` (ADR-0046).

use concordia_api::{
    ApiError, ApiErrorKind, CallCtx, Engine, EngineModule, Method, MethodInfo, PlaySetup,
};
use concordia_command::Actor;
use concordia_reflect::Reflect;
use concordia_terra::{Phase, Schedule, SystemCtx, World, system};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::consumed::ConsumedActions;
use crate::error::InputError;
use crate::event::InputEvent;
use crate::map::InputMap;
use crate::sampler::InputSampler;
use crate::text::TypedText;

/// Adds input sampling to every play world and the `input.inject` method.
#[derive(Debug, Clone, Default)]
pub struct InputModule {
    map: InputMap,
}

impl InputModule {
    /// A module for a validated input map.
    pub fn new(map: InputMap) -> Self {
        Self { map }
    }
}

impl EngineModule for InputModule {
    fn name(&self) -> &'static str {
        "input"
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![MethodInfo::of::<InputInject>()]
    }

    fn start_play(&self, world: &mut World) -> Result<PlaySetup, ApiError> {
        let sampler = InputSampler::new(self.map.clone());
        world.insert_resource(sampler.states().clone());
        world.insert_resource(ConsumedActions::new(&self.map));
        world.insert_resource(TypedText::default());
        world.insert_resource(sampler);
        let mut schedule = Schedule::new();
        schedule.add_in(Phase::PrePhysics, system("input.sample", sample_input));
        Ok(PlaySetup {
            schedule,
            state: None,
        })
    }
}

/// Samples queued input for the tick, publishes [`ActionStates`] and clears [`ConsumedActions`].
fn sample_input(world: &mut World, ctx: &SystemCtx<'_>) {
    if let Some(consumed) = world.resource_mut::<ConsumedActions>() {
        consumed.clear();
    }
    let Some(sampler) = world.resource_mut::<InputSampler>() else {
        return;
    };
    let states = sampler.sample(ctx.tick.0).clone();
    let typed = TypedText(sampler.typed().to_vec());
    world.insert_resource(states);
    world.insert_resource(typed);
}

/// Queue input for the play world (keys, mouse, pointer, scroll, or action values) at a tick; creates a paused play world if none exists. Never changes the scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct InputInject;

/// One event and the tick it applies at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct InjectedEvent {
    /// Tick to apply at (default: the next tick to simulate).
    pub tick: Option<u64>,
    /// The event.
    pub event: InputEvent,
}

/// `input.inject` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct InputInjectParams {
    /// Events, applied in order within a tick.
    pub events: Vec<InjectedEvent>,
    /// Who is pressing (attribution in the log only).
    pub actor: Option<Actor>,
}

/// `input.inject` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct InputInjectResult {
    /// Events queued.
    pub accepted: u32,
    /// The next tick to simulate.
    pub next_tick: u64,
}

impl Method for InputInject {
    const NAME: &'static str = "input.inject";
    const MUTATES: bool = false;
    type Params = InputInjectParams;
    type Result = InputInjectResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: InputInjectParams,
    ) -> Result<InputInjectResult, ApiError> {
        let sampler = engine
            .play_world_mut()?
            .resource_mut::<InputSampler>()
            .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "input module not started"))?;
        let next = sampler.next_tick();
        let timed: Vec<(u64, InputEvent)> = p
            .events
            .into_iter()
            .map(|e| (e.tick.unwrap_or(next), e.event))
            .collect();
        for (i, (tick, event)) in timed.iter().enumerate() {
            sampler.check(*tick, event).map_err(|e| invalid(e, i))?;
        }
        let accepted = timed.len();
        for (tick, event) in timed {
            sampler.push(tick, event).map_err(|e| invalid(e, 0))?;
        }
        let actor = ctx
            .actor
            .as_ref()
            .map_or_else(|| "none".into(), Actor::to_string);
        tracing::info!(target: "concordia::input", actor, events = accepted, next_tick = next, "input injected");
        Ok(InputInjectResult {
            accepted: u32::try_from(accepted).unwrap_or(u32::MAX),
            next_tick: next,
        })
    }
}

/// An input error as `InvalidParams` pointing at the event.
fn invalid(error: InputError, index: usize) -> ApiError {
    let mut api = ApiError::new(ApiErrorKind::InvalidParams, error.to_string())
        .at(format!("/events/{index}"));
    if let InputError::UnknownAction { known, .. } = &error {
        api = api.with("known_actions", json!(known)).suggest(
            known
                .iter()
                .map(|k| format!("use one of the map's actions, e.g. {k:?}"))
                .take(3),
        );
    }
    if let InputError::TickInPast { next, .. } = error {
        api = api.suggest([format!("use tick >= {next}, or omit tick for the next one")]);
    }
    api
}
