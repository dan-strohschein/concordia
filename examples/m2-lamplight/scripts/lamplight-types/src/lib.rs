//! Lamplight's shared script types (M2 phase 2.10c.1): the engine types both modules use,
//! mirrored field for field (ADR-0062: names resolve against the engine's registry), and the
//! night's broadcast, which the rules module raises and the fog module hears.

pub mod body;
pub mod engine;
pub mod night;
pub mod ui;
