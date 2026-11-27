//! Dynamic diffuse GI from probes (M4 4.8, DDGI: Majercik et al. 2019 and 2021): the CPU model of
//! the math the GPU update and sampling use, so each piece is tested on its own.

pub mod octa;
mod probe;

pub use probe::{BASE_HYSTERESIS, ProbeRay, ProbeState, chebyshev, hysteresis, relocate};
