//! Replay (ADR-0072; v1 in M3 phase 3.1): record a play session as per-tick action changes,
//! checkpoint hashes and every tick's world hash in a `.replay.ron` file, then replay it on a
//! fresh play world and report the first tick that diverges, naming the parts (components,
//! clock, RNG) that differ.

mod ab;
mod assert_method;
mod diff;
mod error;
mod eval;
mod format;
mod methods;
mod module;
mod monitor;
mod record;
mod recorder;
mod runner;
mod still;
mod verify;

pub use assert_method::{
    AssertEval, AssertEvalParams, AssertEvalResult, AssertOutcome, AssertSource, AssertStatus,
    Playtest,
};
pub use diff::{ReplayDiff, compare_replays};
pub use error::ReplayError;
pub use format::{
    Checkpoint, ExternalOutput, FORMAT_VERSION, InputChange, Replay, ReplayHeader, TICK_HASH_HEX,
    TICKS_PER_LINE, VERSION_1, read_replay, write_replay,
};
pub use methods::{
    Divergence, FieldDiff, Mismatch, RecordAction, ReplayRecord, ReplayRecordParams,
    ReplayRecordResult, ReplayRun, ReplayRunParams, ReplayRunResult, RunMode, TickHash,
};
pub use module::{EngineFactory, ReplayModule};
