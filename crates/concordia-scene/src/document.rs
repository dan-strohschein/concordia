//! Typed RON documents other than scenes (input maps, materials, sidecars), in canonical form.

use concordia_reflect::{Reflect, find_non_finite, validate_json};

use crate::convert::Converter;
use crate::error::SceneError;
use crate::read::{convert_error, reflect_error, syntax};
use crate::syntax::parse;

/// Reads a RON document as `T`, validated against `T`'s reflected schema (errors carry line, column and pointer).
pub fn read_document<T: Reflect>(text: &str) -> Result<T, SceneError> {
    let info = T::type_info();
    let root = parse(text).map_err(syntax)?;
    let mut converter = Converter::default();
    let json = converter
        .value(&root, info, "")
        .map_err(|e| convert_error(info.name, e))?;
    validate_json(&json, info).map_err(|e| reflect_error(info.name, &e, &converter))?;
    serde_json::from_value(json).map_err(|e| SceneError::Parse {
        line: root.pos.line,
        col: root.pos.col,
        message: format!("{}: {e}", info.name),
    })
}

/// Writes `value` as canonical RON: sorted fields and map keys, 4-space indent, trailing commas.
pub fn write_document<T: Reflect>(value: &T) -> Result<String, SceneError> {
    let info = T::type_info();
    if let Some(pointer) = find_non_finite(value) {
        return Err(SceneError::NonFiniteValue {
            document: info.name.to_owned(),
            pointer,
        });
    }
    let json = serde_json::to_value(value).map_err(|e| SceneError::Parse {
        line: 0,
        col: 0,
        message: format!("{}: {e}", info.name),
    })?;
    Ok(crate::write::document(&json, info))
}
