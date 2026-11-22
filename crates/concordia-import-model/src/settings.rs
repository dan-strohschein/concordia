//! Model import settings (the sidecar's `settings`), shared by the glTF and FBX importers:
//! clip events by clip name (M2 phase 2.4.4).

use std::collections::BTreeMap;

use concordia_assets::{ClipData, ClipEvent, validate_events};
use serde::Deserialize;

/// A model file's import settings.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSettings {
    /// Events to add to each clip, by clip name (seconds from the clip's start).
    #[serde(default)]
    pub clip_events: BTreeMap<String, Vec<ClipEvent>>,
}

impl ModelSettings {
    /// Reads settings from the sidecar's JSON; unknown keys are errors.
    pub fn from_json(json: &serde_json::Value) -> Result<Self, String> {
        Self::deserialize(json).map_err(|e| format!("import settings: {e}"))
    }

    /// Gives `clip` its events (sorted by time, ties in authored order) and validates them.
    pub fn apply(&self, clip: &mut ClipData) -> Result<(), String> {
        let Some(events) = self.clip_events.get(&clip.name) else {
            return Ok(());
        };
        clip.events.clone_from(events);
        clip.events.sort_by(|a, b| a.time.total_cmp(&b.time));
        validate_events(clip).map_err(|e| format!("clip_events {:?}: {e}", clip.name))
    }

    /// Fails when an event names a clip the file doesn't have (`clips` are the file's names).
    pub fn check_clips<'a>(&self, clips: impl IntoIterator<Item = &'a str>) -> Result<(), String> {
        let clips: Vec<&str> = clips.into_iter().collect();
        match self
            .clip_events
            .keys()
            .find(|k| !clips.contains(&k.as_str()))
        {
            Some(unknown) => Err(format!(
                "clip_events names {unknown:?}, which this file doesn't have; its clips are {clips:?}"
            )),
            None => Ok(()),
        }
    }
}
