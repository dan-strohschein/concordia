//! Per-processor random seeds, so script processors never contend for `SimRng` (now in core).

pub use concordia_core::rng::processor_seed;
