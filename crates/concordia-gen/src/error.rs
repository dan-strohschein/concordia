//! Generation errors.

/// A request a provider can't fulfil.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GenError {
    /// A request field is out of range or malformed.
    #[error("{field}: {reason}")]
    Invalid {
        /// The field, e.g. `radius`.
        field: String,
        /// Why.
        reason: String,
    },
    /// The output could not be written.
    #[error("{0}")]
    Output(String),
}

impl GenError {
    /// An invalid `field`.
    pub fn invalid(field: &str, reason: impl Into<String>) -> Self {
        Self::Invalid {
            field: field.to_owned(),
            reason: reason.into(),
        }
    }
}
