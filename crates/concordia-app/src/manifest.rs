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
    /// `[studio]`; Agent Studio (`task.*`, `studio.*`) runs only if this section exists.
    pub studio: Option<StudioSection>,
    /// `[scripts]`; gameplay modules and `script.reload` run only if this section exists.
    pub scripts: Option<ScriptsSection>,
    /// `[replay]`; `replay.record` and `replay.run` exist only if this section exists.
    pub replay: Option<ReplaySection>,
    /// `[gen]`: generated content (`gen.list`, `gen.run`; M3 3.7).
    #[serde(rename = "gen")]
    pub generators: Option<concordia_gen::GenConfig>,
    /// `[anim]`; animation components and play processors exist only if this section exists (needs `[assets]`).
    pub anim: Option<AnimSection>,
    /// `[audio]`; audio components, `audio.*` and the mixer exist only if this section exists
    /// (cues load through `[assets]`).
    pub audio: Option<AudioSection>,
    /// `[nav]`; navigation components and `nav.*` exist only if this section exists (navmeshes are
    /// built from `[physics]` colliders).
    pub nav: Option<NavSection>,
    /// `[ui]`; UI components, `ui.inspect` and the UI pass exist only if this section exists
    /// (images, fonts and themes load through `[assets]`).
    pub ui: Option<UiSection>,
}

/// `[ui]` (no settings yet; ADR-0060).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiSection {}

/// `[nav]` (ADR-0077).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NavSection {
    /// Most agent path plans per tick (at least 1; default 32).
    pub plans_per_tick: std::num::NonZeroU32,
}

impl Default for NavSection {
    fn default() -> Self {
        Self {
            plans_per_tick: concordia_nav::AgentConfig::default().plans_per_tick,
        }
    }
}

/// `[audio]` (ADR-0066).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AudioSection {
    /// The voice pool (1–256; default 32).
    pub voices: u16,
    /// A project-relative `.buses.ron` layout (default: Master, SFX and Music).
    pub buses: Option<String>,
    /// Where the mix goes.
    pub backend: AudioBackendChoice,
}

impl Default for AudioSection {
    fn default() -> Self {
        Self {
            voices: 32,
            buses: None,
            backend: AudioBackendChoice::Auto,
        }
    }
}

/// `[audio] backend`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioBackendChoice {
    /// The default device in `concordia run`; offline everywhere else.
    #[default]
    Auto,
    /// Always offline (sample-exact, no device).
    Offline,
    /// No mixing.
    None,
}

/// `[anim]` (no settings yet; ADR-0074).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnimSection {}

/// `[replay]` (no settings yet; ADR-0072).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReplaySection {}

/// `[scripts]`: the project's gameplay modules (ADR-0061, ADR-0062).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScriptsSection {
    /// `[[scripts.module]]`, in load order.
    pub module: Vec<ScriptModuleSection>,
}

/// How a gameplay module runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptModuleKind {
    /// A WebAssembly component (sandboxed).
    Wasm,
    /// A native module library (trusted, hot-reloadable).
    Native,
    /// A `.graph.ron` Event graph (compiled to WASM; M4 4.10b).
    Graph,
}

/// `[[scripts.module]]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptModuleSection {
    /// `wasm`, `native` or `graph`.
    pub kind: ScriptModuleKind,
    /// Project-relative module file (a native library may omit its extension).
    pub path: String,
    /// A native module's source folder, checked for hidden state before every load.
    pub source: Option<String>,
}

/// `[studio]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StudioSection {
    /// The Studio folder (project-relative): team.toml, agents/, skills/, tasks/.
    pub dir: String,
}

impl Default for StudioSection {
    fn default() -> Self {
        Self {
            dir: "studio".into(),
        }
    }
}

/// `[render]` (the device is created on the first capture).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenderSection {
    /// Closures a pixel may evaluate, 1..=8; only lowers the platform's budget (ADR-0068).
    pub closure_budget: Option<u8>,
    /// Time each pass of every frame (`render.stats` `passes`; `concordia perf run` sets it).
    pub perf_mode: bool,
    /// GPU backend: `auto` (Metal on macOS, Vulkan elsewhere), `metal` or `vulkan`;
    /// `CONCORDIA_BACKEND` overrides it.
    pub backend: concordia_rhi_select::BackendChoice,
    /// Draw static meshes with meshlets through the meshlet path (ADR-0100); on by default.
    pub meshlets: bool,
    /// Cull the meshlet path's instances and meshlets on the GPU (M4 4.4); on by default.
    pub culling: bool,
    /// The temporal resolve: `off` (the default), `own` or `metalfx` (M4 4.6).
    pub temporal: concordia_render::temporal::TemporalBackend,
    /// Output over internal size while `temporal` is on: 1.0 (the default), 1.5 or 2.0.
    pub upscale: concordia_render::temporal::Upscale,
    /// The sun's shadows: `csm` (the default during M4 4.7) or `vsm` (falls back to `csm` where
    /// the device can't run virtual shadow maps).
    pub shadows: concordia_render::vsm::ShadowTechnique,
    /// `[render.vsm]`: the virtual shadow maps' pool, levels and biases.
    pub vsm: concordia_render::vsm::VsmSettings,
    /// Diffuse GI: `off` (the default) or `probes` (M4 4.8).
    pub gi: concordia_render::gi::GiTechnique,
    /// `[render.probes]`: GI's probe volume (rays, probe counts, spacing, an optional fixed centre).
    pub probes: concordia_render::gi::GiSettings,
}

impl Default for RenderSection {
    fn default() -> Self {
        Self {
            closure_budget: None,
            perf_mode: false,
            backend: Default::default(),
            meshlets: true,
            culling: true,
            temporal: Default::default(),
            upscale: Default::default(),
            shadows: Default::default(),
            vsm: Default::default(),
            gi: Default::default(),
            probes: Default::default(),
        }
    }
}

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
