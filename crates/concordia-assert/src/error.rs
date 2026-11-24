//! Parse and type errors, precise enough to point at the fix.

/// Text that is not an assertion.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("columns {start}–{end}: expected {}, found {found:?}", expected.join(" or "))]
pub struct ParseError {
    /// First column of the offending text (1-based).
    pub start: usize,
    /// Last column (inclusive).
    pub end: usize,
    /// What would have been accepted there.
    pub expected: Vec<String>,
    /// What was there (empty at the end of the text).
    pub found: String,
}

/// An assertion that parses but can't run against this project's types.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TypeError {
    /// No such component.
    #[error("no component {name:?}{}", hint(did_you_mean))]
    UnknownComponent {
        /// The name used.
        name: String,
        /// Close names.
        did_you_mean: Vec<String>,
    },
    /// No such field in the component.
    #[error("{component} has no field {field:?}{}", hint(did_you_mean))]
    UnknownField {
        /// The component.
        component: String,
        /// The path used, dotted.
        field: String,
        /// Close names at the failing step, as full paths.
        did_you_mean: Vec<String>,
    },
    /// The field is not a number or a boolean.
    #[error("{component}.{field} is a {found}; assertions compare numbers and booleans")]
    UnsupportedField {
        /// The component.
        component: String,
        /// The path, dotted.
        field: String,
        /// Its JSON type.
        found: String,
    },
    /// A selector that may match several entities, read as one.
    #[error("{selector} may match several entities; write any({selector}).… or all({selector}).…")]
    FieldNeedsQuantifier {
        /// The selector, as written.
        selector: String,
    },
    /// Two sides with different units.
    #[error("unit mismatch: {expected:?} compared with {found:?}")]
    UnitMismatch {
        /// The left side's dimension.
        expected: crate::Dim,
        /// The right side's.
        found: crate::Dim,
    },
    /// A non-boolean used as a condition, or a boolean compared by size.
    #[error("{0}")]
    NotBoolean(String),
    /// A duration that is not a whole number of ticks, or not positive.
    #[error("{0}")]
    Duration(String),
}

fn hint(names: &[String]) -> String {
    match names {
        [] => String::new(),
        names => format!(" (did you mean {}?)", names.join(", ")),
    }
}
