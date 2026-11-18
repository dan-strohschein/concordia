//! Errors from dynamic (de)serialization and hashing.

/// A reflected value could not be built, serialized or hashed.
///
/// `pointer` fields are RFC 6901 JSON Pointers into the input (`""` is the root).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ReflectError {
    /// No type is registered under `name`.
    #[error("no type named {name:?} is registered")]
    UnknownType {
        /// The requested name.
        name: String,
    },
    /// A value has the wrong JSON type.
    #[error("at {pointer:?}: expected {expected}, found {found}")]
    Invalid {
        /// Where.
        pointer: String,
        /// What the schema wants.
        expected: String,
        /// What was given.
        found: String,
    },
    /// A required field is absent.
    #[error("at {pointer:?}: missing required field {field:?}")]
    MissingField {
        /// The containing object.
        pointer: String,
        /// The missing field.
        field: String,
    },
    /// An object has a field the type does not define.
    #[error("at {pointer:?}: unknown field {field:?}; expected one of {expected:?}")]
    UnknownField {
        /// The containing object.
        pointer: String,
        /// The extra field.
        field: String,
        /// The fields the type defines.
        expected: Vec<&'static str>,
    },
    /// A number is outside its declared range.
    #[error("at {pointer:?}: {value} is outside the range [{min}, {max}]")]
    OutOfRange {
        /// Where.
        pointer: String,
        /// The value given.
        value: f64,
        /// Lower bound.
        min: f64,
        /// Upper bound.
        max: f64,
    },
    /// serde rejected a value that passed shape validation.
    #[error("cannot deserialize {type_name}: {message}")]
    Deserialize {
        /// The target type.
        type_name: String,
        /// serde's message.
        message: String,
    },
    /// A value could not be serialized.
    #[error("cannot serialize {type_name}: {message}")]
    Serialize {
        /// The source type.
        type_name: String,
        /// serde's message.
        message: String,
    },
}
