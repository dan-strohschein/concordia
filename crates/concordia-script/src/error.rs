//! Script errors: loading and registration ([`ScriptError`]) and run faults ([`ScriptFault`]).

/// Loading, registering or reloading a module failed.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// The backend can't load this kind of source.
    #[error("the {backend} backend can't load {source_kind}")]
    Unsupported {
        /// The backend.
        backend: &'static str,
        /// The source kind.
        source_kind: String,
    },
    /// No linked module has this name.
    #[error("no linked module named {0:?}")]
    UnknownModule(String),
    /// The module couldn't be compiled or instantiated.
    #[error("module failed to load: {0}")]
    Load(String),
    /// `describe` faulted.
    #[error("describe failed: {0}")]
    Describe(ScriptFault),
    /// The description is malformed.
    #[error(transparent)]
    Protocol(#[from] concordia_script_protocol::ProtocolError),
    /// A type couldn't be registered.
    #[error("module {module}: type {name}: {reason}")]
    Type {
        /// The module.
        module: String,
        /// The type.
        name: String,
        /// Why.
        reason: String,
    },
    /// A processor names a component or signal the registry doesn't know.
    #[error("processor {processor} names unknown type {name:?}")]
    UnknownType {
        /// The processor.
        processor: String,
        /// The unknown name.
        name: String,
    },
    /// A reload changed the processor set or access (rebuild the schedule instead).
    #[error("reload of {module} changes its processors: {reason}")]
    ReloadChangesProcessors {
        /// The module.
        module: String,
        /// What changed.
        reason: String,
    },
    /// The DDC failed.
    #[error("derived-data cache: {0}")]
    Cache(String),
}

/// Why a run produced no effects.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptFault {
    /// The module trapped (a WASM trap, a native panic).
    #[error("trap: {0}")]
    Trap(String),
    /// The run used up its fuel.
    #[error("out of fuel: {0}")]
    OutOfFuel(String),
    /// The module returned an error.
    #[error("error: {0}")]
    Returned(String),
    /// The output broke the protocol (an undeclared write, a bad value, a bad target).
    #[error("protocol violation: {0}")]
    Protocol(String),
}
