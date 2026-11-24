//! `ab` runs: the replay on this engine and on another build (an engine for another project
//! directory, from the injected factory) in lockstep; at the first tick that differs, the parts,
//! the fields and the system after which they first differ.

use std::path::PathBuf;
use std::time::Instant;

use concordia_api::{ApiError, ApiErrorKind, Engine, WorldSel};
use concordia_terra::WorldHash;
use serde_json::json;

use crate::format::Replay;
use crate::methods::{Divergence, Mismatch, ReplayRunResult};
use crate::module::ReplayState;
use crate::record::checkpoint;
use crate::runner::prepare;
use crate::still;
use crate::verify::diverged_parts;

/// Runs `replay` here (expected) and on the build in `other` (actual) in lockstep.
pub(crate) fn run_ab(
    engine: &mut Engine,
    replay: &Replay,
    other: &str,
    mismatches: Vec<Mismatch>,
) -> Result<ReplayRunResult, ApiError> {
    let started = Instant::now();
    let mut b = other_engine(engine, replay, other)?;
    let first = first_difference(engine, &mut b, replay.ticks)?;
    let Some(first) = first else {
        let world_hash = hash_of(engine)?;
        return Ok(ReplayRunResult {
            ok: true,
            ticks: replay.ticks,
            matched: u32::try_from(replay.ticks).unwrap_or(u32::MAX),
            divergence: None,
            mismatches,
            checkpoints: Vec::new(),
            world_hash,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        });
    };
    let divergence = examine(engine, &mut b, replay, first)?;
    Ok(ReplayRunResult {
        ok: false,
        ticks: first,
        matched: u32::try_from(first - 1).unwrap_or(u32::MAX),
        divergence: Some(divergence),
        mismatches,
        checkpoints: Vec::new(),
        world_hash: hash_of(engine)?,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// The other build: an engine for `other` (absolute, or relative to this project) with the
/// replay's scene open and its inputs loaded.
fn other_engine(engine: &Engine, replay: &Replay, other: &str) -> Result<Engine, ApiError> {
    let factory = engine
        .state::<ReplayState>()
        .and_then(|s| s.factory.clone())
        .ok_or_else(|| {
            ApiError::new(
                ApiErrorKind::InvalidParams,
                "ab needs an engine factory to build the other project; engines built for a project (concordia mcp, the CLI, the editor) have one",
            )
        })?;
    let dir = match PathBuf::from(other) {
        p if p.is_absolute() => p,
        p => engine.project().join(p),
    };
    if !dir.is_dir() {
        return Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            format!("other: {} is not a directory", dir.display()),
        )
        .at("/other"));
    }
    let mut b = factory(&dir, engine.threads())?;
    let session = b.open_session();
    b.call(
        session,
        "scene.open",
        json!({ "path": replay.header.scene }),
    )?;
    load(&mut b, replay)?;
    Ok(b)
}

/// A fresh play world with the replay's inputs; unknown actions refuse the run.
fn load(engine: &mut Engine, replay: &Replay) -> Result<(), ApiError> {
    let found = prepare(engine, replay)?;
    match found.iter().find(|m| m.field == "inputs") {
        Some(m) => Err(ApiError::new(
            ApiErrorKind::ReplayMismatch,
            format!("the other build can't play the inputs: {}", m.current),
        )),
        None => Ok(()),
    }
}

/// Steps both a tick at a time; the first tick whose hashes differ.
fn first_difference(a: &mut Engine, b: &mut Engine, ticks: u64) -> Result<Option<u64>, ApiError> {
    for tick in 1..=ticks {
        if a.step_play(1)? != b.step_play(1)? {
            return Ok(Some(tick));
        }
    }
    Ok(None)
}

/// Replays both to just before `tick`, runs it observed, and reports the parts, the fields
/// and the first system after which the two differ.
fn examine(
    a: &mut Engine,
    b: &mut Engine,
    replay: &Replay,
    tick: u64,
) -> Result<Divergence, ApiError> {
    load(a, replay)?;
    load(b, replay)?;
    let before = u32::try_from(tick - 1).unwrap_or(u32::MAX);
    if before > 0 {
        a.step_play(before)?;
        b.step_play(before)?;
    }
    let (seen_a, seen_b) = (observed(a)?, observed(b)?);
    let suspect_system =
        seen_a
            .iter()
            .zip(&seen_b)
            .find(|(x, y)| x != y)
            .map(|((la, _), (lb, _))| {
                if la == lb {
                    la.clone()
                } else {
                    format!("{la} / {lb}")
                }
            });
    let (wa, wb) = (a.world(WorldSel::play)?, b.world(WorldSel::play)?);
    let internal = |e: String| ApiError::new(ApiErrorKind::Internal, e);
    let (ca, cb) = (
        checkpoint(wa).map_err(|e| internal(e.to_string()))?,
        checkpoint(wb).map_err(|e| internal(e.to_string()))?,
    );
    let fields = still::diff(
        &still::take(wa).map_err(internal)?,
        &still::take(wb).map_err(internal)?,
    );
    Ok(Divergence {
        tick,
        last_matching_tick: tick.checked_sub(1),
        parts_tick: tick,
        parts: diverged_parts(&ca, &cb),
        expected: ca.world,
        actual: cb.world,
        fields,
        fields_note: None,
        suspect_system,
    })
}

/// One tick, with the world hash after each wave and phase flush.
fn observed(engine: &mut Engine) -> Result<Vec<(String, WorldHash)>, ApiError> {
    let mut seen = Vec::new();
    let mut failed = None;
    engine.step_play_observed(&mut |label, world| match world.state_hash() {
        Ok(h) => seen.push((label.to_owned(), h)),
        Err(e) => {
            failed.get_or_insert(e.to_string());
        }
    })?;
    match failed {
        Some(e) => Err(ApiError::new(ApiErrorKind::Internal, e)),
        None => Ok(seen),
    }
}

fn hash_of(engine: &Engine) -> Result<String, ApiError> {
    Ok(engine
        .world(WorldSel::play)?
        .state_hash()
        .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))?
        .to_string())
}
