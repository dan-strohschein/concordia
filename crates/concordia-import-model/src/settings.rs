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
    /// The exposure (EV100) at which the file's emission looks right: imported emission is screen
    /// brightness at this exposure, converted to nits (ADR-0102). Null: the project's default.
    #[serde(default)]
    pub emissive_reference_ev100: Option<f32>,
    /// Which static meshes get meshlets (`"auto"`, `true` or `false`; M4 4.2).
    #[serde(default)]
    pub meshlets: crate::MeshletMode,
}

impl ModelSettings {
    /// Reads settings from the sidecar's JSON; unknown keys are errors.
    pub fn from_json(json: &serde_json::Value) -> Result<Self, String> {
        let settings = Self::deserialize(json).map_err(|e| format!("import settings: {e}"))?;
        match settings.emissive_reference_ev100 {
            Some(ev) if !(-10.0..=30.0).contains(&ev) => Err(format!(
                "import settings: emissive_reference_ev100 {ev} is outside -10..=30"
            )),
            _ => Ok(settings),
        }
    }

    /// The settings every model importer declares (null: filled from the project's defaults).
    pub fn defaults() -> serde_json::Value {
        serde_json::json!({ "emissive_reference_ev100": null, "meshlets": "auto" })
    }

    /// Nits per unit of imported emission: 1.2 · 2^EV at the reference exposure (EV 0 when unset).
    pub fn emission_scale(&self) -> f32 {
        1.2 * 2f32.powf(self.emissive_reference_ev100.unwrap_or(0.0))
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
