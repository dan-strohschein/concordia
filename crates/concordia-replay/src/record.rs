//! A recording in progress, filled after every tick by an end-of-tick hook.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use concordia_input::{ActionStates, TypedText};
use concordia_terra::{EcsError, SimClock, World};

use crate::format::{
    Checkpoint, FORMAT_VERSION, InputChange, Replay, ReplayHeader, TICK_HASH_HEX, TICKS_PER_LINE,
};

/// Input changes and checkpoints of one recording.
pub(crate) struct Recording {
    pub(crate) header: ReplayHeader,
    /// Project-relative path, as given.
    pub(crate) rel: String,
    /// Where the replay is written.
    pub(crate) file: PathBuf,
    last: Option<ActionStates>,
    inputs: Vec<InputChange>,
    checkpoints: Vec<Checkpoint>,
    /// Each tick's world hash prefix, ticks 1, 2, … in order.
    ticks: Vec<String>,
    error: Option<String>,
}

impl Recording {
    /// An empty recording; [`Recording::begin`] takes its baseline.
    pub(crate) fn new(header: ReplayHeader, rel: String, file: PathBuf) -> Self {
        Self {
            header,
            rel,
            file,
            last: None,
            inputs: Vec::new(),
            checkpoints: Vec::new(),
            ticks: Vec::new(),
            error: None,
        }
    }

    /// Takes the action states before tick 0 and the tick-0 checkpoint.
    pub(crate) fn begin(&mut self, world: &World) {
        self.last = world.resource::<ActionStates>().cloned();
        self.checkpoint(world);
    }

    /// Records the tick just simulated: its input changes, and a checkpoint when one is due.
    pub(crate) fn after_tick(&mut self, world: &World) {
        let tick = tick_of(world);
        if let (Some(last), Some(now)) = (&self.last, world.resource::<ActionStates>()) {
            let actions = now.changed_since(last);
            let text = world
                .resource::<TypedText>()
                .map(|t| t.0.clone())
                .unwrap_or_default();
            if !actions.is_empty() || !text.is_empty() {
                self.inputs.push(InputChange {
                    tick: tick.saturating_sub(1),
                    actions,
                    text,
                });
            }
            self.last = Some(now.clone());
        }
        if tick.is_multiple_of(u64::from(self.header.checkpoint_every.max(1))) {
            self.checkpoint(world);
        }
        self.tick_hash(world, tick);
    }

    /// Notes `tick`'s world hash (reusing a checkpoint taken on it).
    fn tick_hash(&mut self, world: &World, tick: u64) {
        let hash = match self.checkpoints.last().filter(|c| c.tick == tick) {
            Some(c) => c.world.clone(),
            None => match world.state_hash() {
                Ok(h) => h.to_string(),
                Err(e) => {
                    self.error.get_or_insert_with(|| format!("tick hash: {e}"));
                    return;
                }
            },
        };
        self.ticks.push(hash[..TICK_HASH_HEX].to_owned());
    }

    /// Ticks with an input change, and checkpoints taken.
    pub(crate) fn counts(&self) -> (usize, usize) {
        (self.inputs.len(), self.checkpoints.len())
    }

    /// The finished replay, with a last checkpoint at the current tick.
    pub(crate) fn finish(mut self, world: &World) -> Result<Replay, String> {
        let ticks = tick_of(world);
        if self.checkpoints.last().map(|c| c.tick) != Some(ticks) {
            self.checkpoint(world);
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(Replay {
            version: FORMAT_VERSION,
            header: self.header,
            ticks,
            inputs: self.inputs,
            checkpoints: self.checkpoints,
            hashes: self
                .ticks
                .chunks(TICKS_PER_LINE)
                .map(|c| c.concat())
                .collect(),
        })
    }

    fn checkpoint(&mut self, world: &World) {
        match checkpoint(world) {
            Ok(c) => self.checkpoints.push(c),
            Err(e) => {
                self.error.get_or_insert_with(|| format!("checkpoint: {e}"));
            }
        }
    }
}

/// The world's hashes now.
pub(crate) fn checkpoint(world: &World) -> Result<Checkpoint, EcsError> {
    let digest = world.state_digest()?;
    Ok(Checkpoint {
        tick: tick_of(world),
        world: digest.world.to_string(),
        parts: digest
            .parts
            .into_iter()
            .map(|(name, hash)| (name, hash.to_string()))
            .collect(),
    })
}

/// Ticks completed (0 before the clock exists).
pub(crate) fn tick_of(world: &World) -> u64 {
    world.resource::<SimClock>().map_or(0, |c| c.tick.0)
}

/// The current play session's recording, shared with its end-of-tick hook; empty once stopped.
#[derive(Clone, Default)]
pub(crate) struct RecordingHandle(Arc<Mutex<Option<Recording>>>);

impl RecordingHandle {
    pub(crate) fn new(recording: Recording) -> Self {
        Self(Arc::new(Mutex::new(Some(recording))))
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Option<Recording>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
