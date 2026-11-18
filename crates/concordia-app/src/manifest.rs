//! `concordia.toml`: every section is optional and unknown keys are errors.

use concordia_platform::WindowDesc;
use serde::{Deserialize, Serialize};

/// The whole manifest.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Manifest {
    /// `[project]`.
    pub project: ProjectSection,
    /// `[scene]`.
    pub scene: SceneSection,
    /// `[sim]`.
    pub sim: SimSection,
    /// `[journal]`.
    pub journal: JournalSection,
    /// `[window]` (used by `concordia run`).
    pub window: WindowDesc,
    /// `[input]`.
    pub input: InputSection,
}

/// `[project]`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectSection {
    /// Project name.
    pub name: String,
}

/// `[scene]`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SceneSection {
    /// Scene opened by default (project-relative).
    pub main: Option<String>,
}

/// `[sim]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SimSection {
    /// Fixed tick rate.
    pub rate_hz: u32,
    /// Play-world RNG seed.
    pub seed: u64,
}

impl Default for SimSection {
    fn default() -> Self {
        Self {
            rate_hz: 60,
            seed: 0,
        }
    }
}

/// `[journal]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JournalSection {
    /// fsync every journal write.
    pub sync: bool,
}

impl Default for JournalSection {
    fn default() -> Self {
        Self { sync: true }
    }
}

/// `[input]`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InputSection {
    /// Input map (`.input.ron`, project-relative); none means no actions.
    pub map: Option<String>,
}
