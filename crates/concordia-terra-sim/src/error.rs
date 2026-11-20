//! Template, trait and spawner errors.

use concordia_scene::SceneError;
use concordia_terra::EcsError;

/// A template could not be read, compiled or spawned.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TemplateError {
    /// No registered trait has this name.
    #[error("unknown trait {name:?}{}", hint(did_you_mean))]
    UnknownTrait {
        /// The name used.
        name: String,
        /// The closest trait name.
        did_you_mean: Option<String>,
    },
    /// The name is a registered type, but not a trait.
    #[error("{name} is not a trait; put it under `components` or register it with register_trait")]
    NotATrait {
        /// The type's name.
        name: String,
    },
    /// No registered component has this name.
    #[error("unknown component {name:?}{}", hint(did_you_mean))]
    UnknownComponent {
        /// The name used.
        name: String,
        /// The closest component name.
        did_you_mean: Option<String>,
    },
    /// No template has this name.
    #[error("unknown template {name:?}{}", hint(did_you_mean))]
    UnknownTemplate {
        /// The name used.
        name: String,
        /// The closest template name.
        did_you_mean: Option<String>,
    },
    /// Two traits add the same component.
    #[error(
        "traits {first} and {second} both add {component}; drop one, or set {component} under `components` to choose its value"
    )]
    TraitConflict {
        /// The contested component.
        component: String,
        /// The first trait (by name).
        first: String,
        /// The second trait.
        second: String,
    },
    /// A value is out of range or the wrong shape.
    #[error("{what}: {reason}")]
    InvalidValue {
        /// What was given (a trait, component or parameter).
        what: String,
        /// What is wrong.
        reason: String,
    },
    /// Two templates share a name.
    #[error("two templates are named {name}: {first} and {second}; rename one")]
    Duplicate {
        /// The shared name.
        name: String,
        /// Where the first came from.
        first: String,
        /// Where the second came from.
        second: String,
    },
    /// The schedule lacks processors the template's traits need.
    #[error("template {template} needs processors {processors:?}, which no module provides")]
    MissingProcessors {
        /// The template.
        template: String,
        /// The missing processor or system names.
        processors: Vec<String>,
    },
    /// A template file could not be read.
    #[error("{path}: {message}")]
    Io {
        /// The file.
        path: String,
        /// The error.
        message: String,
    },
    /// A template file is malformed.
    #[error("{path}: {source}")]
    Scene {
        /// The file.
        path: String,
        /// The parse or validation error.
        source: Box<SceneError>,
    },
    /// A world operation failed.
    #[error(transparent)]
    Ecs(#[from] EcsError),
}

fn hint(did_you_mean: &Option<String>) -> String {
    did_you_mean
        .as_ref()
        .map(|n| format!("; did you mean {n:?}?"))
        .unwrap_or_default()
}
