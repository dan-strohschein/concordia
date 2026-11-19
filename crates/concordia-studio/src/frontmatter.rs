//! Markdown files with YAML frontmatter: `---\n<yaml>---\n<body>` (Claude Code's agent format).

use concordia_core::suggest::did_you_mean;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::StudioError;

const FENCE: &str = "---\n";

/// Splits `text` into its YAML frontmatter and its Markdown body.
pub(crate) fn split(text: &str) -> Result<(&str, &str), StudioError> {
    let rest = text
        .strip_prefix(FENCE)
        .ok_or_else(|| fence_error("the file must start with a `---` line"))?;
    if let Some(body) = rest.strip_prefix(FENCE) {
        return Ok(("", body));
    }
    let end = rest
        .find("\n---\n")
        .map(|i| (i + 1, i + 1 + FENCE.len()))
        .or_else(|| {
            rest.strip_suffix("\n---")
                .map(|y| (y.len() + 1, rest.len()))
        })
        .ok_or_else(|| fence_error("the frontmatter has no closing `---` line"))?;
    Ok((&rest[..end.0], &rest[end.1..]))
}

/// Parses frontmatter, rejecting keys outside `known` with a suggestion.
pub(crate) fn parse<T: DeserializeOwned>(yaml: &str, known: &[&str]) -> Result<T, StudioError> {
    let value: serde_norway::Value = serde_norway::from_str(yaml).map_err(yaml_error)?;
    let map = value
        .as_mapping()
        .ok_or_else(|| fence_error("the frontmatter must be a YAML mapping"))?;
    for key in map.keys() {
        let key = key.as_str().unwrap_or_default();
        if !known.contains(&key) {
            return Err(StudioError::UnknownKey {
                file: String::new(),
                key: key.into(),
                did_you_mean: did_you_mean(key, known.iter().copied())
                    .first()
                    .map(|s| (*s).to_owned()),
            });
        }
    }
    serde_norway::from_value(value).map_err(yaml_error)
}

/// Writes `fields` as frontmatter followed by `body`.
pub(crate) fn join<T: Serialize>(fields: &T, body: &str) -> String {
    let yaml = serde_norway::to_string(fields).unwrap_or_default();
    format!("{FENCE}{yaml}{FENCE}{body}")
}

fn fence_error(message: &str) -> StudioError {
    StudioError::Frontmatter {
        file: String::new(),
        message: message.into(),
    }
}

fn yaml_error(e: serde_norway::Error) -> StudioError {
    StudioError::Frontmatter {
        file: String::new(),
        message: format!("frontmatter: {e}"),
    }
}
