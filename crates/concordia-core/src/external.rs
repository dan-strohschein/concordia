//! Outputs from outside the simulation (M4 4.12d, D8): an LLM's answer arrives when it arrives,
//! so whoever applies one logs it with its tick, and a replay plays the log back instead of
//! asking again. A world resource (not hashed): the replay crate records and loads it.

use std::collections::BTreeMap;

/// One applied output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalRecord {
    /// The tick it was applied on.
    pub tick: u64,
    /// Who asked (`llm:<entity>:<node>`).
    pub source: String,
    /// What came back, verbatim.
    pub payload: String,
}

/// Applied outputs this session, and (in a replay) the recorded ones to use instead.
#[derive(Debug, Clone, Default)]
pub struct ExternalInputs {
    /// Every output applied so far, in order.
    pub applied: Vec<ExternalRecord>,
    playback: Option<BTreeMap<(u64, String), String>>,
}

impl ExternalInputs {
    /// Plays `records` back: outputs come from them, never from a provider.
    pub fn playing(records: impl IntoIterator<Item = ExternalRecord>) -> Self {
        Self {
            applied: Vec::new(),
            playback: Some(
                records
                    .into_iter()
                    .map(|r| ((r.tick, r.source), r.payload))
                    .collect(),
            ),
        }
    }

    /// Whether outputs come from a recording.
    pub fn is_playback(&self) -> bool {
        self.playback.is_some()
    }

    /// The recorded output for `source` on `tick` (playback only).
    pub fn recorded(&self, tick: u64, source: &str) -> Option<&str> {
        self.playback
            .as_ref()?
            .get(&(tick, source.to_owned()))
            .map(String::as_str)
    }

    /// Logs an applied output.
    pub fn apply(&mut self, tick: u64, source: &str, payload: &str) {
        self.applied.push(ExternalRecord {
            tick,
            source: source.to_owned(),
            payload: payload.to_owned(),
        });
    }
}
