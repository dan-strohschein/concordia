//! Generated content without APIs (M3 phase 3.7): providers turn a request and a seed into a
//! file, `gen.run` writes and imports it with its provenance, and `gen.list` describes them.

pub mod blender;
mod config;
mod error;
pub mod glb;
pub mod mesh;
mod methods;
mod module;
pub mod music;
mod plugin;
mod process;
mod provider;
mod registry;
pub mod sfx;
pub mod texture;
mod wav;

pub use config::{BlenderConfig, GenConfig, PluginConfig};
pub use error::GenError;
pub use methods::{GenList, GenListResult, GenRun, GenRunParams, GenRunResult, ProviderInfo};
pub use module::GenModule;
pub use plugin::PluginProvider;
pub use provider::{ContentProvider, GenContext, GenKind, Generated, inputs_hash};
pub use registry::ProviderRegistry;
