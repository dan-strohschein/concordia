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
    /// `[physics]`; the physics module runs only if this section exists.
    pub physics: Option<PhysicsSection>,
    /// `[assets]`; the asset database (image and glTF importers) runs only if this section exists.
    pub assets: Option<AssetsSection>,
    /// `[render]`; render components and `capture.screenshot` exist only if this section exists.
    pub render: Option<RenderSection>,
}

/// `[render]` (no settings yet; the device is created on the first capture).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenderSection {}

/// `[assets]` (no settings yet).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AssetsSection {}

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

/// `[physics]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicsSection {
    /// Gravity in m/s².
    pub gravity: [f32; 3],
    /// Most bodies per world (1..=65536).
    pub max_bodies: u32,
}

impl Default for PhysicsSection {
    fn default() -> Self {
        Self {
            gravity: [0.0, -9.81, 0.0],
            max_bodies: 16_384,
        }
    }
}

/// `[input]`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InputSection {
    /// Input map (`.input.ron`, project-relative); none means no actions.
    pub map: Option<String>,
}
