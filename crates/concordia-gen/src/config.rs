//! `[gen]` in `concordia.toml`: where Blender is, and the opt-in plugins.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::provider::GenKind;

/// The `[gen]` section.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GenConfig {
    /// `[gen.blender]`.
    pub blender: Option<BlenderConfig>,
    /// `[[gen.plugin]]`: out-of-process providers, loaded only when listed here.
    pub plugin: Vec<PluginConfig>,
}

/// `[gen.blender]`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlenderConfig {
    /// The Blender executable (else `BLENDER_PATH`, the default install, then `PATH`).
    pub path: Option<PathBuf>,
}

impl BlenderConfig {
    /// Where Blender usually is, in order.
    pub fn default_candidates() -> Vec<PathBuf> {
        let mut out = vec![PathBuf::from(
            "/Applications/Blender.app/Contents/MacOS/Blender",
        )];
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                out.push(dir.join(if cfg!(windows) {
                    "blender.exe"
                } else {
                    "blender"
                }));
            }
        }
        out
    }
}

/// `[[gen.plugin]]`: a provider run as its own program. It gets `{request, seed, out}` as JSON on
/// stdin, writes its file to `out`, and prints `{"info": …}`. Credentials come from its own
/// environment, never from project files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginConfig {
    /// Its provider id.
    pub id: String,
    /// What it makes.
    pub kind: GenKind,
    /// File extensions it writes.
    pub extensions: Vec<String>,
    /// The program and its arguments.
    pub command: Vec<String>,
    /// Seconds before it is stopped (default 300).
    #[serde(default)]
    pub timeout_s: Option<u32>,
}
