//! The `replay` engine module.

use std::any::Any;
use std::sync::{Arc, Mutex, PoisonError};

use concordia_api::{ApiError, Engine, EngineModule, MethodInfo, PlaySetup};
use concordia_terra::{Schedule, World, tick_end};

use crate::record::{Recording, RecordingHandle};

/// A recording waiting for its play world (set by `replay.record`, taken by `start_play_in`).
#[derive(Clone, Default)]
pub(crate) struct Pending(Arc<Mutex<Option<Recording>>>);

impl Pending {
    pub(crate) fn set(&self, recording: Recording) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(recording);
    }

    pub(crate) fn take(&self) -> Option<Recording> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

/// Serves `replay.record` and `replay.run`; enabled by `[replay]` in `concordia.toml`.
#[derive(Default)]
pub struct ReplayModule {
    pending: Pending,
}

impl std::fmt::Debug for ReplayModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayModule").finish_non_exhaustive()
    }
}

impl ReplayModule {
    /// A replay module.
    pub fn new() -> Self {
        Self::default()
    }
}

impl EngineModule for ReplayModule {
    fn name(&self) -> &'static str {
        "replay"
    }

    fn methods(&self) -> Vec<MethodInfo> {
        crate::methods::methods()
    }

    fn engine_state(&self) -> Option<Box<dyn Any + Send>> {
        Some(Box::new(self.pending.clone()))
    }

    /// A pending recording joins the new play world: an end-of-tick hook fills it.
    fn start_play_in(&self, _: &mut Engine, _: &mut World) -> Result<PlaySetup, ApiError> {
        let Some(recording) = self.pending.take() else {
            return Ok(PlaySetup::default());
        };
        let handle = RecordingHandle::new(recording);
        let hook = handle.clone();
        let mut schedule = Schedule::new();
        schedule.add_tick_end(tick_end("replay.record", move |world: &World| {
            if let Some(recording) = hook.lock().as_mut() {
                recording.after_tick(world);
            }
        }));
        Ok(PlaySetup {
            schedule,
            state: Some(Box::new(handle)),
        })
    }
}
