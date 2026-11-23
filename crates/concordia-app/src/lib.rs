//! The composition root: a project's manifest (`concordia.toml`) and the engine modules it runs with.
//!
//! The CLI, the MCP server and the editor all build engines through [`Project`], so the module
//! list lives in one place (ADR-0046).

mod error;
mod manifest;
mod player;
mod project;
mod shared;

pub use error::AppError;
pub use manifest::{
    AnimSection, AssetsSection, AudioBackendChoice, AudioSection, InputSection, JournalSection,
    Manifest, NavSection, PhysicsSection, ProjectSection, RenderSection, SceneSection,
    ScriptModuleKind, ScriptModuleSection, ScriptsSection, SimSection, StudioSection, UiSection,
};
pub use player::{ApiCall, Player};
pub use project::{Playback, Project, model_importers};
pub use shared::{HostedEngine, SharedEngine};
