//! Dynamic diffuse GI from probes (M4 4.8, DDGI: Majercik et al. 2019 and 2021): the CPU model of
//! the math the GPU update and sampling use, so each piece is tested on its own.

mod api;
mod error;
mod gpu;
pub mod octa;
mod probe;
mod scene;
mod settings;
mod state;

pub use api::{RenderSetGi, RenderSetGiParams, RenderSetGiResult};
pub use error::GiError;
pub use gpu::{GiInputs, GiLight, GiProbes, GiSun, ProbeGrid};
pub use probe::{BASE_HYSTERESIS, ProbeRay, ProbeState, chebyshev, hysteresis, relocate};
pub use scene::{GiInstance, GiLook, GiScene, UpdateReport, material, proxy};
pub use settings::{Gi, GiSettings, GiTechnique};
pub(crate) use state::GiState;
