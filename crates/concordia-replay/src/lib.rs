//! Replay v0 (ADR-0072): record a play session as per-tick action changes plus checkpoint
//! hashes in a `.replay.ron` file, then replay it on a fresh play world and report the first
//! checkpoint that diverges, naming the parts (components, clock, RNG) that differ.

mod error;
mod format;
mod methods;
mod module;
mod record;
mod recorder;
mod runner;
mod verify;

pub use error::ReplayError;
pub use format::{
    Checkpoint, FORMAT_VERSION, InputChange, Replay, ReplayHeader, read_replay, write_replay,
};
pub use methods::{
    Divergence, Mismatch, RecordAction, ReplayRecord, ReplayRecordParams, ReplayRecordResult,
    ReplayRun, ReplayRunParams, ReplayRunResult, RunMode, TickHash,
};
pub use module::ReplayModule;
