//! `UiStyle`: how a widget looks (background, border, radii, opacity, text defaults), every
//! field optional so a theme can fill it. `ResolvedStyle` is the result after theming.

use concordia_assets::AssetRef;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::{UiError, finite, in_range};

const NAME: &str = "UiStyle";
/// Fewest gradient stops.
pub const MIN_STOPS: usize = 2;
/// Most gradient stops.
pub const MAX_STOPS: usize = 8;
/// The largest font size.
pub const MAX_FONT_SIZE: f32 = 1024.0;

/// The built-in sans-serif family.
pub const SANS: &str = "builtin:font/noto_sans";
/// The built-in monospace family.
pub const MONO: &str = "builtin:font/noto_sans_mono";

/// A box's fill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Background {
    /// One linear RGBA color.
    Solid(#[reflect(widget = "color")] [f32; 4]),
    /// A linear gradient across the box at `angle_deg` (0: bottom to top, 90: left to right,
    /// as CSS `linear-gradient`).
    Linear {
        /// Direction in degrees.
        angle_deg: f32,
        /// 2–8 ascending stops.
        stops: Vec<GradientStop>,
    },
    /// A radial gradient: offset 1 at `radius` (fractions of the box) from `center`.
    Radial {
        /// Center as fractions of the box.
        center: [f32; 2],
        /// Radii as fractions of the box.
        radius: [f32; 2],
        /// 2–8 ascending stops.
        stops: Vec<GradientStop>,
    },
}

impl Background {
    /// Checks finite values and 2–8 ascending stops in 0..1 (`component` and `field` name it).
    pub fn check(&self, component: &'static str, field: &str) -> Result<(), UiError> {
        match self {
            Background::Solid(color) => finite(component, field, color),
            Background::Linear { angle_deg, stops } => {
                finite(component, &format!("{field}/angle_deg"), &[*angle_deg])?;
                check_stops(component, field, stops)
            }
            Background::Radial {
                center,
                radius,
                stops,
            } => {
                finite(component, &format!("{field}/center"), center)?;
                finite(component, &format!("{field}/radius"), radius)?;
                if radius.iter().any(|r| *r <= 0.0) {
                    return Err(UiError::field(
                        component,
                        format!("{field}/radius"),
                        "radii must be positive",
                    ));
                }
                check_stops(component, field, stops)
            }
        }
    }
}

fn check_stops(
    component: &'static str,
    field: &str,
    stops: &[GradientStop],
) -> Result<(), UiError> {
    let at = format!("{field}/stops");
    if !(MIN_STOPS..=MAX_STOPS).contains(&stops.len()) {
        return Err(UiError::field(
            component,
            at,
            format!(
                "{} stops; a gradient has {MIN_STOPS}..={MAX_STOPS}",
                stops.len()
            ),
        ));
    }
    let mut last = 0.0;
    for (i, stop) in stops.iter().enumerate() {
        let here = format!("{at}/{i}");
        in_range(component, &here, stop.offset, 0.0, 1.0)?;
        finite(component, &here, &stop.color)?;
        if stop.offset < last {
            return Err(UiError::field(component, here, "offsets must ascend"));
        }
        last = stop.offset;
    }
    Ok(())
}

/// A gradient stop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GradientStop {
    /// Position, 0..1.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub offset: f32,
    /// Linear RGBA.
    #[reflect(widget = "color")]
    pub color: [f32; 4],
}

/// A border drawn inside the box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Border {
    /// Width in logical pixels.
    #[reflect(range(min = 0.0, max = 1024.0))]
    pub width: f32,
    /// Linear RGBA.
    #[reflect(widget = "color")]
    pub color: [f32; 4],
}

/// The pointer's look over a widget (a presentation hint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Cursor {
    /// The arrow.
    #[default]
    Arrow,
    /// A pointing hand.
    Pointer,
    /// A text beam.
    Text,
}

/// How a widget looks; unset fields come from its theme, then the defaults.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiStyle {
    /// The box's fill.
    pub background: Option<Background>,
    /// The border.
    pub border: Option<Border>,
    /// Corner radii `[top left, top right, bottom right, bottom left]`.
    pub radius: Option<[f32; 4]>,
    /// Multiplies this widget's and its children's alpha (0..1).
    pub opacity: Option<f32>,
    /// The text font family (a font asset).
    pub font: Option<AssetRef>,
    /// The text size in logical pixels.
    pub font_size: Option<f32>,
    /// The text weight (100..900; 400 regular, 700 bold).
    pub weight: Option<f32>,
    /// Italic text.
    pub italic: Option<bool>,
    /// The text color (linear RGBA).
    pub color: Option<[f32; 4]>,
    /// The pointer's look.
    pub cursor: Option<Cursor>,
}

impl UiStyle {
    /// Checks stops, ranges and finite values.
    pub fn check(&self) -> Result<(), UiError> {
        if let Some(background) = &self.background {
            background.check(NAME, "background")?;
        }
        if let Some(border) = &self.border {
            in_range(NAME, "border/width", border.width, 0.0, 1024.0)?;
            finite(NAME, "border/color", &border.color)?;
        }
        if let Some(radius) = self.radius {
            finite(NAME, "radius", &radius)?;
            if radius.iter().any(|r| *r < 0.0) {
                return Err(UiError::field(NAME, "radius", "radii must not be negative"));
            }
        }
        if let Some(opacity) = self.opacity {
            in_range(NAME, "opacity", opacity, 0.0, 1.0)?;
        }
        if let Some(size) = self.font_size {
            in_range(NAME, "font_size", size, 1.0, MAX_FONT_SIZE)?;
        }
        if let Some(weight) = self.weight {
            in_range(NAME, "weight", weight, 100.0, 900.0)?;
        }
        if let Some(color) = self.color {
            finite(NAME, "color", &color)?;
        }
        Ok(())
    }

    /// Fills every unset field from `other`.
    pub fn or(&self, other: &UiStyle) -> UiStyle {
        UiStyle {
            background: self.background.clone().or_else(|| other.background.clone()),
            border: self.border.or(other.border),
            radius: self.radius.or(other.radius),
            opacity: self.opacity.or(other.opacity),
            font: self.font.clone().or_else(|| other.font.clone()),
            font_size: self.font_size.or(other.font_size),
            weight: self.weight.or(other.weight),
            italic: self.italic.or(other.italic),
            color: self.color.or(other.color),
            cursor: self.cursor.or(other.cursor),
        }
    }
}

/// Names a `.theme.ron` and a class in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI")]
#[serde(deny_unknown_fields)]
pub struct UiThemed {
    /// The theme asset.
    pub theme: AssetRef,
    /// The class name.
    pub class: String,
}

impl UiThemed {
    /// Checks the class name isn't empty.
    pub fn check(&self) -> Result<(), UiError> {
        match self.class.trim().is_empty() {
            true => Err(UiError::field("UiThemed", "class", "must not be empty")),
            false => Ok(()),
        }
    }
}

/// A widget's interaction state, for theme lookups (driven by `UiState` in 2.7b.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum WidgetState {
    /// At rest.
    #[default]
    Normal,
    /// Under the pointer.
    Hover,
    /// Held down.
    Pressed,
    /// Disabled.
    Disabled,
    /// Focused.
    Focused,
}

/// A style with every field decided.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ResolvedStyle {
    /// The fill.
    pub background: Option<Background>,
    /// The border.
    pub border: Option<Border>,
    /// Corner radii.
    pub radius: [f32; 4],
    /// Opacity.
    pub opacity: f32,
    /// The font family.
    pub font: AssetRef,
    /// The text size.
    pub font_size: f32,
    /// The text weight.
    pub weight: f32,
    /// Italic.
    pub italic: bool,
    /// The text color.
    pub color: [f32; 4],
    /// The pointer's look.
    pub cursor: Cursor,
}

/// The built-in sans-serif family's reference.
pub(crate) fn sans() -> AssetRef {
    AssetRef::Builtin {
        kind: "font".into(),
        name: "noto_sans".into(),
    }
}

impl Default for ResolvedStyle {
    fn default() -> Self {
        Self {
            background: None,
            border: None,
            radius: [0.0; 4],
            opacity: 1.0,
            font: sans(),
            font_size: 16.0,
            weight: 400.0,
            italic: false,
            color: [1.0, 1.0, 1.0, 1.0],
            cursor: Cursor::Arrow,
        }
    }
}

impl ResolvedStyle {
    /// `style`'s set fields over the defaults.
    pub fn from_style(style: &UiStyle) -> ResolvedStyle {
        Self::over(style, &ResolvedStyle::default())
    }

    /// `style`'s set fields over `base`.
    pub fn over(style: &UiStyle, base: &ResolvedStyle) -> ResolvedStyle {
        ResolvedStyle {
            background: style.background.clone().or_else(|| base.background.clone()),
            border: style.border.or(base.border),
            radius: style.radius.unwrap_or(base.radius),
            opacity: style.opacity.unwrap_or(base.opacity),
            font: style.font.clone().unwrap_or_else(|| base.font.clone()),
            font_size: style.font_size.unwrap_or(base.font_size),
            weight: style.weight.unwrap_or(base.weight),
            italic: style.italic.unwrap_or(base.italic),
            color: style.color.unwrap_or(base.color),
            cursor: style.cursor.unwrap_or(base.cursor),
        }
    }

    /// The defaults, with the text fields taken from `parent`.
    pub fn inherited(parent: Option<&ResolvedStyle>) -> ResolvedStyle {
        let base = ResolvedStyle::default();
        match parent {
            None => base,
            Some(p) => ResolvedStyle {
                font: p.font.clone(),
                font_size: p.font_size,
                weight: p.weight,
                italic: p.italic,
                color: p.color,
                ..base
            },
        }
    }
}
