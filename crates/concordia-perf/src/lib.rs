//! Perf testing on reference hardware (ADR-0089): per-scene budgets as text, the reference
//! machine, measured reports, verdicts that never claim a pass off the reference, and the runner
//! that measures a scene along a scripted camera path through the Engine API.

pub mod budget;
mod error;
pub mod machine;
pub mod report;
pub mod runner;
pub mod scene;
pub mod verdict;

pub use budget::{BudgetFile, Limit};
pub use error::PerfError;
pub use machine::{HostInfo, HostProbe, ReferenceMachine};
pub use report::{PerfReport, Summary};
pub use runner::{Run, RunOptions, run};
pub use scene::{CameraKey, PerfScene};
pub use verdict::{Stat, Verdict, Violation, check, judge};
