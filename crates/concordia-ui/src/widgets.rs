//! The display widgets (2.7b.1): text, images, nine-patches, progress bars and separators.

use concordia_assets::AssetRef;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::{UiError, finite, in_range};
use crate::style::{Background, MAX_FONT_SIZE};

/// The most text one `UiText` holds, in bytes.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
/// The most tiles a tiled image or nine-patch draws.
pub const MAX_TILES: usize = 4096;

/// A run of text with its own look; unset fields come from the widget's style.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
pub struct TextRun {
    /// The text.
    pub text: String,
    /// The font family.
    pub font: Option<AssetRef>,
    /// The size in logical pixels.
    pub size: Option<f32>,
    /// The weight (100..900).
    pub weight: Option<f32>,
    /// Italic.
    pub italic: Option<bool>,
    /// Linear RGBA.
    pub color: Option<[f32; 4]>,
    /// Underlined.
    pub underline: bool,
}

/// Horizontal text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum TextAlign {
    /// The start edge.
    #[default]
    Start,
    /// Centered.
    Center,
    /// The end edge.
    End,
    /// Both edges (the last line starts).
    Justify,
}

/// Vertical text alignment in the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum VAlign {
    /// The top.
    #[default]
    Top,
    /// Centered.
    Center,
    /// The bottom.
    Bottom,
}

/// Where lines may break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Wrap {
    /// Only at newlines.
    None,
    /// At word boundaries (UAX #14).
    #[default]
    Word,
    /// Anywhere, when a word doesn't fit.
    Anywhere,
}

/// What text past the box does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Overflow {
    /// Drawn anyway.
    #[default]
    Visible,
    /// Cut at the box.
    Clip,
    /// The last line that fits ends with "…".
    Ellipsis,
}

/// A label: styled runs, shaped, wrapped and aligned in the box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiText {
    /// The runs, in order.
    pub runs: Vec<TextRun>,
    /// Horizontal alignment.
    pub align: TextAlign,
    /// Vertical alignment.
    pub valign: VAlign,
    /// Line breaking.
    pub wrap: Wrap,
    /// Past the box.
    pub overflow: Overflow,
    /// Line height as a multiple of the font's (0.5..4).
    #[reflect(range(min = 0.5, max = 4.0))]
    pub line_height: f32,
    /// At most this many lines (none: unlimited).
    pub max_lines: Option<u32>,
}

impl Default for UiText {
    fn default() -> Self {
        Self {
            runs: Vec::new(),
            align: TextAlign::Start,
            valign: VAlign::Top,
            wrap: Wrap::Word,
            overflow: Overflow::Visible,
            line_height: 1.0,
            max_lines: None,
        }
    }
}

impl UiText {
    /// One run of plain text.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            runs: vec![TextRun {
                text: text.into(),
                ..TextRun::default()
            }],
            ..Self::default()
        }
    }

    /// Every run's text, joined.
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Checks the length, sizes, weights and line settings.
    pub fn check(&self) -> Result<(), UiError> {
        const NAME: &str = "UiText";
        let bytes: usize = self.runs.iter().map(|r| r.text.len()).sum();
        if bytes > MAX_TEXT_BYTES {
            return Err(UiError::field(
                NAME,
                "runs",
                format!("{bytes} bytes of text; at most {MAX_TEXT_BYTES}"),
            ));
        }
        for (i, run) in self.runs.iter().enumerate() {
            if let Some(size) = run.size {
                in_range(NAME, &format!("runs/{i}/size"), size, 1.0, MAX_FONT_SIZE)?;
            }
            if let Some(weight) = run.weight {
                in_range(NAME, &format!("runs/{i}/weight"), weight, 100.0, 900.0)?;
            }
            if let Some(color) = run.color {
                finite(NAME, &format!("runs/{i}/color"), &color)?;
            }
        }
        in_range(NAME, "line_height", self.line_height, 0.5, 4.0)?;
        if self.max_lines == Some(0) {
            return Err(UiError::field(NAME, "max_lines", "must be at least 1"));
        }
        Ok(())
    }
}

/// How an image fills its box (Godot's `TextureRect` stretch modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum ImageMode {
    /// Stretched to the box.
    #[default]
    Scale,
    /// Repeated from the top left at its own size.
    Tile,
    /// Its own size at the top left.
    Keep,
    /// Its own size, centered.
    KeepCentered,
    /// Scaled to fit inside, at the top left.
    KeepAspect,
    /// Scaled to fit inside, centered.
    KeepAspectCentered,
    /// Scaled to cover the box, centered and cropped.
    KeepAspectCovered,
}

/// A picture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI")]
#[serde(deny_unknown_fields)]
pub struct UiImage {
    /// The texture asset.
    pub image: AssetRef,
    /// A sub-rect `[x, y, w, h]` in texture pixels (none: all of it).
    #[serde(default)]
    pub region: Option<[u32; 4]>,
    /// How it fills the box.
    #[serde(default)]
    pub mode: ImageMode,
    /// Linear RGBA multiplier.
    #[serde(default = "white")]
    #[reflect(widget = "color")]
    pub tint: [f32; 4],
    /// Mirrored left to right.
    #[serde(default)]
    pub flip_h: bool,
    /// Mirrored top to bottom.
    #[serde(default)]
    pub flip_v: bool,
}

impl UiImage {
    /// Checks the tint and that a region has a size.
    pub fn check(&self) -> Result<(), UiError> {
        check_region("UiImage", self.region)?;
        finite("UiImage", "tint", &self.tint)
    }
}

fn check_region(component: &'static str, region: Option<[u32; 4]>) -> Result<(), UiError> {
    match region {
        Some([_, _, w, h]) if w == 0 || h == 0 => Err(UiError::field(
            component,
            "region",
            "a region needs a width and a height",
        )),
        _ => Ok(()),
    }
}

pub(crate) fn white() -> [f32; 4] {
    [1.0; 4]
}

/// How a nine-patch's edges and center fill their spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum AxisStretch {
    /// Stretched.
    #[default]
    Stretch,
    /// Repeated, the last tile cut.
    Tile,
    /// Repeated a whole number of times, scaled to fit.
    TileFit,
}

/// A picture cut into nine parts whose corners keep their size (Godot's `NinePatchRect`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI")]
#[serde(deny_unknown_fields)]
pub struct UiNinePatch {
    /// The texture asset.
    pub image: AssetRef,
    /// A sub-rect `[x, y, w, h]` in texture pixels (none: all of it).
    #[serde(default)]
    pub region: Option<[u32; 4]>,
    /// The fixed borders in texture pixels: left, top, right, bottom.
    pub margins: [u32; 4],
    /// Horizontal edges and center.
    #[serde(default)]
    pub axis_h: AxisStretch,
    /// Vertical edges and center.
    #[serde(default)]
    pub axis_v: AxisStretch,
    /// Draws the center part.
    #[serde(default = "yes")]
    pub draw_center: bool,
    /// Linear RGBA multiplier.
    #[serde(default = "white")]
    #[reflect(widget = "color")]
    pub tint: [f32; 4],
}

fn yes() -> bool {
    true
}

impl UiNinePatch {
    /// Checks the tint and that a region has a size (margins are checked against the texture
    /// when drawing).
    pub fn check(&self) -> Result<(), UiError> {
        check_region("UiNinePatch", self.region)?;
        finite("UiNinePatch", "tint", &self.tint)
    }
}

/// How a progress bar fills (Godot's `TextureProgressBar` fill modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum FillMode {
    /// From the left.
    #[default]
    LeftToRight,
    /// From the right.
    RightToLeft,
    /// From the top.
    TopToBottom,
    /// From the bottom.
    BottomToTop,
    /// Around the center, clockwise from `radial.start_deg`.
    Clockwise,
    /// Around the center, counterclockwise.
    CounterClockwise,
    /// Outward from the vertical center line.
    BilinearH,
    /// Outward from the horizontal center line.
    BilinearV,
    /// Around the center, both ways from the start.
    ClockwiseAndCounter,
}

/// The radial fill's geometry.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
pub struct RadialFill {
    /// Where the fill starts, in degrees clockwise from up.
    pub start_deg: f32,
    /// The full sweep in degrees (0..360].
    pub sweep_deg: f32,
    /// The pivot as fractions of the box.
    pub center: [f32; 2],
}

impl Default for RadialFill {
    fn default() -> Self {
        Self {
            start_deg: 0.0,
            sweep_deg: 360.0,
            center: [0.5, 0.5],
        }
    }
}

/// A bar or dial showing `value` between `min` and `max`, drawn from textures or fills.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiProgress {
    /// The value.
    pub value: f32,
    /// The empty end.
    pub min: f32,
    /// The full end.
    pub max: f32,
    /// How it fills.
    pub fill: FillMode,
    /// The radial modes' geometry.
    pub radial: RadialFill,
    /// Drawn under the fill (texture mode).
    pub under: Option<AssetRef>,
    /// Revealed by the fill (texture mode).
    pub progress: Option<AssetRef>,
    /// Drawn over the fill (texture mode).
    pub over: Option<AssetRef>,
    /// The empty track (fill mode, when there is no `progress` texture).
    pub track: Option<Background>,
    /// The filled part (fill mode).
    pub bar: Option<Background>,
    /// Multiplies the progress texture or the bar.
    #[reflect(widget = "color")]
    pub tint: [f32; 4],
}

impl Default for UiProgress {
    fn default() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 100.0,
            fill: FillMode::LeftToRight,
            radial: RadialFill::default(),
            under: None,
            progress: None,
            over: None,
            track: Some(Background::Solid([0.1, 0.1, 0.1, 1.0])),
            bar: Some(Background::Solid([0.2, 0.6, 0.2, 1.0])),
            tint: white(),
        }
    }
}

impl UiProgress {
    /// The filled fraction, `clamp((value − min) / (max − min), 0, 1)`.
    pub fn fraction(&self) -> f32 {
        let span = self.max - self.min;
        if span.is_nan() || span <= 0.0 || !self.value.is_finite() {
            return 0.0;
        }
        ((self.value - self.min) / span).clamp(0.0, 1.0)
    }

    /// Checks min < max, finite values and the sweep.
    pub fn check(&self) -> Result<(), UiError> {
        const NAME: &str = "UiProgress";
        finite(NAME, "value", &[self.value])?;
        finite(NAME, "min", &[self.min])?;
        finite(NAME, "max", &[self.max])?;
        if self.min >= self.max {
            return Err(UiError::field(
                NAME,
                "max",
                format!("{} must exceed min {}", self.max, self.min),
            ));
        }
        finite(NAME, "radial/start_deg", &[self.radial.start_deg])?;
        finite(NAME, "radial/center", &self.radial.center)?;
        let sweep = self.radial.sweep_deg;
        if !(sweep.is_finite() && sweep > 0.0 && sweep <= 360.0) {
            return Err(UiError::field(
                NAME,
                "radial/sweep_deg",
                format!("{sweep} must be in (0, 360]"),
            ));
        }
        for (field, bg) in [("track", &self.track), ("bar", &self.bar)] {
            if let Some(bg) = bg {
                bg.check(NAME, field)?;
            }
        }
        finite(NAME, "tint", &self.tint)
    }
}

/// Which way a separator runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Orientation {
    /// Left to right.
    #[default]
    Horizontal,
    /// Top to bottom.
    Vertical,
}

/// A line across its box, centered.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiSeparator {
    /// Its direction.
    pub orientation: Orientation,
    /// Its thickness in logical pixels.
    #[reflect(range(min = 0.0, max = 1024.0))]
    pub thickness: f32,
    /// Linear RGBA.
    #[reflect(widget = "color")]
    pub color: [f32; 4],
}

impl Default for UiSeparator {
    fn default() -> Self {
        Self {
            orientation: Orientation::Horizontal,
            thickness: 1.0,
            color: [0.5, 0.5, 0.5, 1.0],
        }
    }
}

impl UiSeparator {
    /// Checks the thickness and color are finite.
    pub fn check(&self) -> Result<(), UiError> {
        in_range("UiSeparator", "thickness", self.thickness, 0.0, 1024.0)?;
        finite("UiSeparator", "color", &self.color)
    }
}
