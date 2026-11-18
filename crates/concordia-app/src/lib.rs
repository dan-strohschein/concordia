//! The composition root: a project's manifest (`concordia.toml`) and the engine modules it runs with.
//!
//! The CLI, the MCP server and the editor all build engines through [`Project`], so the module
//! list lives in one place (ADR-0046).

mod error;
mod manifest;
mod project;

pub use error::AppError;
pub use manifest::{
    InputSection, JournalSection, Manifest, ProjectSection, SceneSection, SimSection,
};
pub use project::Project;
