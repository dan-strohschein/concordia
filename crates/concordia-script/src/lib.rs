//! The scripting host seam (ADR-0062): every engine-side scripting feature is written once,
//! here, against [`ScriptBackend`] and [`ScriptModule`]. Backends (WASM, native, the in-process
//! [`TestBackend`]) only load modules and carry the protocol's calls.
//!
//! [`ScriptHost::load`] registers a module's components, signals and traits; its
//! [`ScriptProcessor`]s join Terra's schedule like any processor, one protocol call per
//! processor per tick. A run's column writes and commands are checked in full before any is
//! applied, so a fault applies nothing; it disables that processor and is reported as
//! `script:<module>`.

use std::path::PathBuf;
use std::sync::Arc;

pub use concordia_script_protocol as protocol;
pub use concordia_script_protocol::{
    Batch, Column, Command, ModuleDesc, ProcessorDesc, RunOutput, SignalColumn, Target,
    TickContext, TraitDesc, Value,
};

mod error;
mod host;
mod processor;
mod register;
mod reload;
mod seed;
mod test_backend;

#[cfg(feature = "conformance")]
#[doc(hidden)]
pub mod conformance;

pub use error::{ScriptError, ScriptFault};
pub use host::{
    FaultReport, HostConfig, LogLine, ReloadReport, ScriptHost, Templates, TypeMigration,
};
pub use processor::ScriptProcessor;
pub use register::register_module;
pub use seed::processor_seed;
pub use test_backend::TestBackend;

/// Where a module comes from; each backend accepts the kinds it can load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleSource {
    /// A module file (a `.wasm` component, a native library).
    File(PathBuf),
    /// A module's bytes.
    Bytes(Vec<u8>),
    /// A module linked into the engine, by module name.
    Linked(String),
}

/// One run's result and the log lines the module wrote during it (kept even on a fault).
#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    /// The output, or why the run failed.
    pub result: Result<RunOutput, ScriptFault>,
    /// Log lines, in order.
    pub logs: Vec<String>,
}

/// Boundary crossings so far: calls into the module and calls out of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModuleStats {
    /// `describe` and `run` calls.
    pub calls_in: u64,
    /// Host imports called by the module (`log`).
    pub calls_out: u64,
}

impl ModuleStats {
    /// Every crossing, both directions.
    pub fn crossings(&self) -> u64 {
        self.calls_in + self.calls_out
    }
}

/// A loaded module. Runs share nothing: a module keeps no state between them.
pub trait ScriptModule: Send + Sync {
    /// Declares the module's types, traits and processors.
    fn describe(&self) -> Result<ModuleDesc, ScriptFault>;
    /// Runs processor `processor` once on `batch`.
    fn run(&self, processor: &str, ctx: &TickContext, batch: &Batch) -> RunReport;
    /// Boundary crossings so far.
    fn stats(&self) -> ModuleStats;
}

/// A way of loading modules (WASM, native, in-process).
pub trait ScriptBackend: Send + Sync {
    /// The backend's name, for logs and errors.
    fn name(&self) -> &'static str;
    /// Loads a module; sources this backend can't load are [`ScriptError::Unsupported`].
    fn load(&self, source: &ModuleSource) -> Result<Arc<dyn ScriptModule>, ScriptError>;
}
