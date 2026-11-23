//! `.theme.ron`: class names mapped to a base style plus per-state styles (Godot's `Theme`, for
//! semantics), the `theme` importer, and style resolution.

use std::collections::BTreeMap;

use concordia_assets::{
    Artifact, AssetError, AssetKind, ImportInput, ImportOutput, Importer, encode,
};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::UiError;
use crate::style::{ResolvedStyle, UiStyle, WidgetState};

/// One class: a base style and what each state changes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeClass {
    /// Always applies.
    pub base: UiStyle,
    /// Under the pointer.
    pub hover: UiStyle,
    /// Held down.
    pub pressed: UiStyle,
    /// Disabled.
    pub disabled: UiStyle,
    /// Focused.
    pub focused: UiStyle,
}

impl ThemeClass {
    /// The style `state` adds.
    pub fn state(&self, state: WidgetState) -> Option<&UiStyle> {
        match state {
            WidgetState::Normal => None,
            WidgetState::Hover => Some(&self.hover),
            WidgetState::Pressed => Some(&self.pressed),
            WidgetState::Disabled => Some(&self.disabled),
            WidgetState::Focused => Some(&self.focused),
        }
    }
}

/// A `.theme.ron` file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    /// Class name → class.
    pub classes: BTreeMap<String, ThemeClass>,
}

impl Theme {
    /// Checks every class's styles and names.
    pub fn check(&self) -> Result<(), UiError> {
        for (name, class) in &self.classes {
            if name.trim().is_empty() {
                return Err(UiError::field(
                    "Theme",
                    "classes",
                    "class names must not be empty",
                ));
            }
            for (state, style) in [
                ("base", &class.base),
                ("hover", &class.hover),
                ("pressed", &class.pressed),
                ("disabled", &class.disabled),
                ("focused", &class.focused),
            ] {
                style.check().map_err(|e| match e {
                    UiError::Field { field, reason, .. } => {
                        UiError::field("Theme", format!("classes/{name}/{state}/{field}"), reason)
                    }
                    other => other,
                })?;
            }
        }
        Ok(())
    }
}

/// Reads and checks `.theme.ron` text.
pub fn read_theme(text: &str) -> Result<Theme, UiError> {
    let theme =
        concordia_scene::read_document::<Theme>(text).map_err(|e| UiError::Read(e.to_string()))?;
    theme.check()?;
    Ok(theme)
}

/// Writes a theme as canonical `.theme.ron` text.
pub fn write_theme(theme: &Theme) -> Result<String, UiError> {
    theme.check()?;
    concordia_scene::write_document(theme).map_err(|e| UiError::Read(e.to_string()))
}

/// Resolves a widget's style: the inline field, then the class's `state` style, then the
/// class's base; text fields not set anywhere come from `parent`, the rest from the defaults.
pub fn resolve(
    inline: Option<&UiStyle>,
    class: Option<&ThemeClass>,
    state: WidgetState,
    parent: Option<&ResolvedStyle>,
) -> ResolvedStyle {
    let empty = UiStyle::default();
    let inline = inline.unwrap_or(&empty);
    let mut style = inline.clone();
    if let Some(class) = class {
        if let Some(extra) = class.state(state) {
            style = style.or(extra);
        }
        style = style.or(&class.base);
    }
    ResolvedStyle::over(&style, &ResolvedStyle::inherited(parent))
}

/// The theme importer: `.theme.ron` → `Theme`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThemeImporter;

impl Importer for ThemeImporter {
    fn name(&self) -> &'static str {
        "theme"
    }

    fn version(&self) -> u32 {
        1
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["theme.ron"]
    }

    fn default_settings(&self) -> serde_json::Value {
        serde_json::Value::Object(serde_json::Map::new())
    }

    fn import(&self, input: ImportInput<'_>) -> Result<ImportOutput, AssetError> {
        let fail = |reason: String| AssetError::Import {
            path: input.path.to_path_buf(),
            reason,
        };
        let text = std::str::from_utf8(input.bytes).map_err(|e| fail(e.to_string()))?;
        let theme = read_theme(text).map_err(|e| fail(e.to_string()))?;
        let mut output = ImportOutput::default();
        output.artifacts.insert(
            String::new(),
            Artifact {
                kind: AssetKind::Theme,
                bytes: encode(&theme)?,
            },
        );
        Ok(output)
    }
}
