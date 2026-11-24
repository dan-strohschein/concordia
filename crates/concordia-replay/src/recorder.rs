//! `replay.record`: starting a recording on a fresh play world, and writing it on stop.

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, WorldSel};

use crate::format::{Replay, write_replay};
use crate::methods::{EXTENSION, ReplayRecordParams, ReplayRecordResult, actor_name};
use crate::module::Pending;
use crate::record::{Recording, RecordingHandle};
use crate::verify::{current_header, fingerprints, mismatch_error, modules_mismatch};

/// Ticks between checkpoints unless a recording asks otherwise.
const DEFAULT_EVERY: u32 = 60;

/// `replay.record {action: start}`: a fresh play world from the scene, recorded.
pub(crate) fn start(
    engine: &mut Engine,
    ctx: &CallCtx,
    p: &ReplayRecordParams,
) -> Result<ReplayRecordResult, ApiError> {
    let rel = p.path.clone().ok_or_else(|| {
        ApiError::new(ApiErrorKind::InvalidParams, "start needs a path")
            .at("/path")
            .suggest(["e.g. \"path\": \"tests/replays/jump.replay.ron\"".to_owned()])
    })?;
    let file = engine.project_file(&rel, EXTENSION)?;
    let every = p.checkpoint_every.unwrap_or(DEFAULT_EVERY);
    if every == 0 {
        return Err(ApiError::new(
            ApiErrorKind::InvalidParams,
            "checkpoint_every must be at least 1",
        )
        .at("/checkpoint_every"));
    }
    let header = current_header(engine, actor_name(ctx), every, p.task.clone())?;
    let pending = engine
        .state::<Pending>()
        .cloned()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "replay module not started"))?;
    let stills = p
        .stills
        .unwrap_or(true)
        .then(|| engine.project().join(crate::runner::DDC));
    pending.set(Recording::new(header, rel.clone(), file, stills));
    if let Err(e) = engine.restart_play(None) {
        // Don't let the recording join a later, unrelated play world.
        pending.take();
        return Err(e);
    }
    let handle = recording(engine)?;
    let world = engine.world(WorldSel::play)?;
    let mut guard = handle.lock();
    let recording = guard
        .as_mut()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the recording did not start"))?;
    recording.begin(world);
    tracing::info!(target: "concordia::replay", actor = actor_name(ctx), path = %rel, "replay recording started");
    Ok(status("recording", rel, 0, recording.counts()))
}

/// `replay.record {action: stop}`: writes the replay, unless the code changed meanwhile.
pub(crate) fn stop(engine: &mut Engine, ctx: &CallCtx) -> Result<ReplayRecordResult, ApiError> {
    let handle = recording(engine)?;
    let now = fingerprints(engine);
    let mut guard = handle.lock();
    let Some(recording) = guard.as_ref() else {
        return Err(not_recording());
    };
    if recording.header.modules != now {
        let mut current = recording.header.clone();
        current.modules = now;
        let found = vec![modules_mismatch(&recording.header, &current)];
        return Err(mismatch_error(
            "a module's code changed during the recording, so it cannot replay; record again",
            &found,
        ));
    }
    let recording = guard.take().ok_or_else(not_recording)?;
    let (rel, file) = (recording.rel.clone(), recording.file.clone());
    let replay = recording
        .finish(engine.world(WorldSel::play)?)
        .map_err(|e| ApiError::new(ApiErrorKind::Internal, e))?;
    write(&file, &replay)
        .map_err(|e| ApiError::new(ApiErrorKind::SceneIo, format!("{rel}: {e}")))?;
    let counts = (replay.inputs.len(), replay.checkpoints.len());
    tracing::info!(
        target: "concordia::replay",
        actor = actor_name(ctx),
        path = %rel,
        ticks = replay.ticks,
        inputs = counts.0,
        checkpoints = counts.1,
        "replay recorded"
    );
    Ok(status("stopped", rel, replay.ticks, counts))
}

/// The play session's recording handle.
fn recording(engine: &Engine) -> Result<RecordingHandle, ApiError> {
    engine
        .play_state::<RecordingHandle>()
        .cloned()
        .ok_or_else(not_recording)
}

fn not_recording() -> ApiError {
    ApiError::new(
        ApiErrorKind::NotPlaying,
        "nothing is being recorded: recording never started, or its play world was discarded (sim.stop, scene.open)",
    )
    .suggest(["start one with replay.record {\"action\": \"start\", \"path\": \"tests/replays/<name>.replay.ron\"}".to_owned()])
}

/// Writes `replay` to `file`, creating its directory.
pub(crate) fn write(file: &std::path::Path, replay: &Replay) -> Result<(), String> {
    let text = write_replay(replay).map_err(|e| e.to_string())?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, text).map_err(|e| e.to_string())
}

fn status(state: &str, path: String, tick: u64, counts: (usize, usize)) -> ReplayRecordResult {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    ReplayRecordResult {
        state: state.into(),
        path,
        tick,
        inputs: count(counts.0),
        checkpoints: count(counts.1),
    }
}
