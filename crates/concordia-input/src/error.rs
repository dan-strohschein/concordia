//! Input errors.

/// An input map or event was invalid.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum InputError {
    /// The `.input.ron` text did not parse or match the schema.
    #[error("input map: {0}")]
    Parse(String),
    /// An action name breaks the naming rule.
    #[error("action name {name:?} must match [a-z][a-z0-9_]* and be at most 64 characters")]
    ActionName {
        /// The name.
        name: String,
    },
    /// A binding does not fit the action's kind.
    #[error("action {action:?} is {kind}, which cannot use binding {binding}")]
    BindingKind {
        /// Action name.
        action: String,
        /// The action's kind.
        kind: String,
        /// The binding.
        binding: String,
    },
    /// An event names an action the map does not have.
    #[error("no action named {action:?}; known actions: {}", known.join(", "))]
    UnknownAction {
        /// The name given.
        action: String,
        /// Every action in the map.
        known: Vec<String>,
    },
    /// An action event's value does not fit the action's kind.
    #[error("action {action:?} is {kind}; give a {kind} value")]
    ValueKind {
        /// Action name.
        action: String,
        /// The action's kind.
        kind: String,
    },
    /// An event targets a tick that has already been simulated.
    #[error("tick {tick} has already been simulated; the next tick is {next}")]
    TickInPast {
        /// Requested tick.
        tick: u64,
        /// First tick that can still take input.
        next: u64,
    },
    /// A pointer position or viewport was not usable.
    #[error("invalid pointer input: {0}")]
    Pointer(String),
}
