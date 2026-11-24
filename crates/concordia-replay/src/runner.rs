//! `replay.run`: loading a replay onto a fresh play world and checking its checkpoints.

use std::collections::BTreeMap;
use std::time::Instant;

use concordia_api::{ApiError, ApiErrorKind, Engine, WorldSel};
use concordia_core::time::TickRate;
use concordia_input::InputSampler;

use crate::format::{Checkpoint, Replay};
use crate::methods::{Divergence, Mismatch, ReplayRunResult, RunMode, TickHash};
use crate::record::{checkpoint, tick_of};
use crate::verify::{current_header, diverged_parts, mismatches};

/// Starts a fresh play world at the replay's rate and loads its inputs; returns what differs
/// from the recording (header fields, then unknown actions).
pub(crate) fn prepare(engine: &mut Engine, replay: &Replay) -> Result<Vec<Mismatch>, ApiError> {
    let header = &replay.header;
    let mut found = mismatches(
        header,
        &current_header(engine, String::new(), header.checkpoint_every, None)?,
    );
    let rate = TickRate::from_hz(header.rate_hz).map_err(|e| {
        ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("rate_hz {}: {e}", header.rate_hz),
        )
    })?;
    let world = engine.restart_play(Some(rate))?;
    if replay.inputs.is_empty() {
        return Ok(found);
    }
    let changes: BTreeMap<u64, _> = replay
        .inputs
        .iter()
        .map(|i| (i.tick, i.actions.clone()))
        .collect();
    let text: BTreeMap<u64, _> = replay
        .inputs
        .iter()
        .filter(|i| !i.text.is_empty())
        .map(|i| (i.tick, i.text.clone()))
        .collect();
    let played = match world.resource_mut::<InputSampler>() {
        Some(sampler) => sampler
            .play_back(changes, replay.ticks)
            .map(|()| sampler.play_back_text(text))
            .map_err(|e| e.to_string()),
        None => Err("this project has no [input] map".to_owned()),
    };
    if let Err(why) = played {
        let actions: std::collections::BTreeSet<&String> = replay
            .inputs
            .iter()
            .flat_map(|i| i.actions.keys())
            .collect();
        found.push(Mismatch {
            field: "inputs".into(),
            recorded: format!("actions {actions:?}"),
            current: why,
        });
    }
    Ok(found)
}

/// Steps the play world checkpoint by checkpoint, comparing (verify) or collecting (resimulate).
pub(crate) fn run(
    engine: &mut Engine,
    replay: &Replay,
    mode: RunMode,
    mismatches: Vec<Mismatch>,
) -> Result<ReplayRunResult, ApiError> {
    let started = Instant::now();
    let mut matched = 0u32;
    let mut last_matching = None;
    let mut divergence = None;
    let mut checkpoints = Vec::new();
    let per_tick = mode == RunMode::verify && !replay.hashes.is_empty();
    let mut first: Option<u64> = None;
    for expected in &replay.checkpoints {
        if per_tick && first.is_none() {
            first = step_checking(engine, replay, expected.tick)?;
        }
        let actual = advance_to(engine, expected.tick)?;
        if mode == RunMode::resimulate {
            checkpoints.push(TickHash {
                tick: actual.tick,
                world_hash: actual.world.clone(),
            });
            continue;
        }
        if actual.world == expected.world && first.is_none() {
            matched += 1;
            last_matching = Some(expected.tick);
            continue;
        }
        divergence = Some(Divergence {
            tick: first.unwrap_or(expected.tick),
            last_matching_tick: first.map_or(last_matching, |t| t.checked_sub(1)),
            parts_tick: expected.tick,
            parts: diverged_parts(expected, &actual),
            expected: expected.world.clone(),
            actual: actual.world,
        });
        break;
    }
    if divergence.is_none() {
        advance_to(engine, replay.ticks)?;
    }
    let world = engine.world(WorldSel::play)?;
    let ticks = tick_of(world);
    let world_hash = world
        .state_hash()
        .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))?
        .to_string();
    Ok(ReplayRunResult {
        ok: divergence.is_none(),
        ticks,
        matched,
        divergence,
        mismatches,
        checkpoints,
        world_hash,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Steps the play world one tick at a time up to `to`, comparing each tick's hash with the
/// recorded one; returns the first tick that differs (the rest are stepped unchecked).
fn step_checking(engine: &mut Engine, replay: &Replay, to: u64) -> Result<Option<u64>, ApiError> {
    let mut tick = tick_of(engine.world(WorldSel::play)?);
    while tick < to {
        let hash = engine.step_play(1)?.to_string();
        tick += 1;
        if replay
            .tick_hash(tick)
            .is_some_and(|want| !hash.starts_with(want))
        {
            return Ok(Some(tick));
        }
    }
    Ok(None)
}

/// Steps the play world to `tick` (if it is behind) and returns its checkpoint there.
fn advance_to(engine: &mut Engine, tick: u64) -> Result<Checkpoint, ApiError> {
    let now = tick_of(engine.world(WorldSel::play)?);
    let mut left = tick.saturating_sub(now);
    while left > 0 {
        let step = u32::try_from(left).unwrap_or(u32::MAX);
        engine.step_play(step)?;
        left -= u64::from(step);
    }
    checkpoint(engine.world(WorldSel::play)?)
        .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))
}
