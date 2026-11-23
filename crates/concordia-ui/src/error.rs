//! UI errors: each names the component and field at fault.

use thiserror::Error;

/// Why a UI component, theme or font was refused.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum UiError {
    /// A field breaks a rule.
    #[error("{component}.{field}: {reason}")]
    Field {
        /// The component (or file) type.
        component: &'static str,
        /// The field's path.
        field: String,
        /// What is wrong.
        reason: String,
    },
    /// A file or asset couldn't be read.
    #[error("{0}")]
    Read(String),
}

impl UiError {
    /// A field error.
    pub fn field(
        component: &'static str,
        field: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::Field {
            component,
            field: field.into(),
            reason: reason.into(),
        }
    }

    /// The field at fault, if this is a field error.
    pub fn field_name(&self) -> Option<&str> {
        match self {
            Self::Field { field, .. } => Some(field),
            Self::Read(_) => None,
        }
    }
}

/// Checks that `value` is finite and within `[min, max]`.
pub(crate) fn in_range(
    component: &'static str,
    field: &str,
    value: f32,
    min: f32,
    max: f32,
) -> Result<(), UiError> {
    match value.is_finite() && (min..=max).contains(&value) {
        true => Ok(()),
        false => Err(UiError::field(
            component,
            field,
            format!("{value} must be in [{min}, {max}]"),
        )),
    }
}

/// Checks that every value is finite.
pub(crate) fn finite(component: &'static str, field: &str, values: &[f32]) -> Result<(), UiError> {
    match values.iter().all(|v| v.is_finite()) {
        true => Ok(()),
        false => Err(UiError::field(component, field, "must be finite")),
    }
}
