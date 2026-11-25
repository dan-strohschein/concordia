//! Chooses and opens a GPU backend: `auto`, `metal` or `vulkan` (ADR-0088).
//!
//! `CONCORDIA_BACKEND` beats the project's `[render] backend`, which beats `auto` (Metal on
//! macOS, Vulkan elsewhere).

use concordia_rhi::{Backend, CapsMask, Device, RhiCaps, RhiError};
use serde::{Deserialize, Serialize};

/// Environment variable that overrides the project's backend.
pub const BACKEND_ENV: &str = "CONCORDIA_BACKEND";

/// A backend, or `auto` for the platform's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendChoice {
    /// Metal on macOS, Vulkan elsewhere.
    #[default]
    Auto,
    /// Apple Metal.
    Metal,
    /// Khronos Vulkan.
    Vulkan,
}

impl BackendChoice {
    const ALL: [Self; 3] = [Self::Auto, Self::Metal, Self::Vulkan];

    /// The lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Metal => "metal",
            Self::Vulkan => "vulkan",
        }
    }

    /// The backend on this platform.
    pub fn resolve(self) -> Backend {
        self.resolve_for(cfg!(target_os = "macos"))
    }

    /// The backend on macOS (`macos`) or elsewhere.
    pub fn resolve_for(self, macos: bool) -> Backend {
        match self {
            Self::Metal => Backend::Metal,
            Self::Vulkan => Backend::Vulkan,
            Self::Auto if macos => Backend::Metal,
            Self::Auto => Backend::Vulkan,
        }
    }
}

impl std::fmt::Display for BackendChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A name that is no backend choice.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown backend {0:?}; expected one of: auto, metal, vulkan")]
pub struct UnknownChoice(pub String);

impl std::str::FromStr for BackendChoice {
    type Err = UnknownChoice;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|c| c.as_str() == name)
            .ok_or_else(|| UnknownChoice(name.to_owned()))
    }
}

/// Why no backend could be chosen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SelectError {
    /// `CONCORDIA_BACKEND` names no backend.
    #[error("{BACKEND_ENV}: {0}")]
    Env(UnknownChoice),
}

/// The backend for `env` (the value of `CONCORDIA_BACKEND`; empty is unset) over `project`.
pub fn choose(env: Option<&str>, project: BackendChoice) -> Result<Backend, SelectError> {
    let choice = match env.filter(|v| !v.is_empty()) {
        Some(value) => value.parse().map_err(SelectError::Env)?,
        None => project,
    };
    Ok(choice.resolve())
}

/// [`choose`] with this process's `CONCORDIA_BACKEND`.
pub fn choose_from_env(project: BackendChoice) -> Result<Backend, SelectError> {
    choose(std::env::var(BACKEND_ENV).ok().as_deref(), project)
}

/// A headless device on `backend`.
pub fn new_headless(backend: Backend) -> Result<Box<dyn Device>, RhiError> {
    new_headless_masked(backend, &CapsMask::default())
}

/// [`new_headless`] with `mask`'s features hidden (Vulkan validates as
/// `CONCORDIA_VULKAN_VALIDATION` says).
pub fn new_headless_masked(backend: Backend, mask: &CapsMask) -> Result<Box<dyn Device>, RhiError> {
    match backend {
        Backend::Metal => concordia_rhi_metal::new_headless_masked(mask),
        Backend::Vulkan => concordia_rhi_vulkan::new_headless_masked(mask),
    }
}

/// The capabilities of the device [`new_headless`] would open.
pub fn probe(backend: Backend) -> Result<RhiCaps, RhiError> {
    match backend {
        Backend::Metal => concordia_rhi_metal::probe(),
        Backend::Vulkan => concordia_rhi_vulkan::probe(),
    }
}

/// The backend's raw probe record (test fixtures), as JSON.
pub fn probe_record(backend: Backend) -> Result<serde_json::Value, RhiError> {
    let json = |r: Result<serde_json::Value, serde_json::Error>| {
        r.map_err(|e| RhiError::invalid("probe record", e.to_string()))
    };
    match backend {
        Backend::Metal => json(serde_json::to_value(concordia_rhi_metal::probe_record()?)),
        Backend::Vulkan => json(serde_json::to_value(concordia_rhi_vulkan::probe_record()?)),
    }
}
