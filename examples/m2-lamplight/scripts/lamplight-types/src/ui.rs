//! Engine UI types the rules read or write, mirrored field for field: widget values, the node
//! (for `visible`), progress bars, and the signals controls raise.

use concordia_reflect::Reflect;
use concordia_script_guest::EntityId;
use serde::{Deserialize, Serialize};

use crate::engine::Wrap;

/// The engine's `Parent` (a hierarchy link).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Parent")]
pub struct Parent(pub EntityId);

/// The engine's `UiPressed` signal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "UiPressed", default)]
pub struct UiPressed;

/// The engine's `UiValueChanged` signal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "UiValueChanged", default)]
pub struct UiValueChanged {
    /// The new value.
    pub value: f32,
}

/// The engine's `UiDropdown`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "UiDropdown")]
pub struct UiDropdown {
    /// The options.
    pub options: Vec<String>,
    /// The chosen option.
    pub selected: Option<u32>,
    /// Open.
    pub open: bool,
    /// Rows shown before scrolling.
    pub max_visible: u32,
}

/// The engine's slider `Orientation`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Orientation")]
pub enum Orientation {
    /// Left to right.
    Horizontal,
    /// Bottom to top.
    Vertical,
}

/// The engine's `UiSlider`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "UiSlider")]
pub struct UiSlider {
    /// The value.
    pub value: f32,
    /// The minimum.
    pub min: f32,
    /// The maximum.
    pub max: f32,
    /// The step.
    pub step: f32,
    /// Its direction.
    pub orientation: Orientation,
    /// The page step.
    pub page: f32,
}

/// The engine's `UiSpinBox`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "UiSpinBox")]
pub struct UiSpinBox {
    /// The value.
    pub value: f32,
    /// The minimum.
    pub min: f32,
    /// The maximum.
    pub max: f32,
    /// The step.
    pub step: f32,
    /// Decimals shown.
    pub decimals: u32,
}

/// The engine's `UiTextBox`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "UiTextBox")]
pub struct UiTextBox {
    /// The text.
    pub text: String,
    /// Shown while empty.
    pub placeholder: String,
    /// Several lines.
    pub multiline: bool,
    /// The longest text, in characters.
    pub max_len: Option<u32>,
    /// Masked.
    pub password: bool,
    /// Editable.
    pub editable: bool,
    /// The caret (byte offset).
    pub caret: u32,
    /// The selection's other end (byte offset).
    pub anchor: u32,
    /// Wrapping.
    pub wrap: Wrap,
}

/// The engine's `Edges`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Edges")]
pub struct Edges {
    /// Left.
    pub left: f32,
    /// Top.
    pub top: f32,
    /// Right.
    pub right: f32,
    /// Bottom.
    pub bottom: f32,
}

/// The engine's `Length`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Length")]
pub enum Length {
    /// From content.
    Auto,
    /// Pixels.
    Px(f32),
    /// Percent of the parent.
    Percent(f32),
}

/// The engine's `Direction`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Direction")]
pub enum Direction {
    /// Left to right.
    Row,
    /// Top to bottom.
    Column,
    /// Right to left.
    RowReverse,
    /// Bottom to top.
    ColumnReverse,
}

/// The engine's `Align`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Align")]
pub enum Align {
    /// Fill.
    Stretch,
    /// Start.
    Start,
    /// End.
    End,
    /// Centre.
    Center,
}

/// The engine's `Justify`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Justify")]
pub enum Justify {
    /// Start.
    Start,
    /// End.
    End,
    /// Centre.
    Center,
    /// Space between.
    SpaceBetween,
    /// Space around.
    SpaceAround,
    /// Space evenly.
    SpaceEvenly,
}

/// The engine's `Track`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Track")]
pub enum Track {
    /// Pixels.
    Px(f32),
    /// A fraction of the rest.
    Fr(f32),
    /// Percent.
    Percent(f32),
    /// From content.
    Auto,
}

/// The engine's `Flex`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Flex")]
pub struct Flex {
    /// The main axis.
    pub direction: Direction,
    /// Wraps lines.
    pub wrap: bool,
    /// Along the main axis.
    pub justify: Justify,
    /// Across it.
    pub align_items: Align,
    /// Lines across it.
    pub align_content: Justify,
}

/// The engine's `Grid`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Grid")]
pub struct Grid {
    /// Columns.
    pub columns: Vec<Track>,
    /// Rows.
    pub rows: Vec<Track>,
    /// Items along rows.
    pub justify_items: Align,
    /// Items along columns.
    pub align_items: Align,
}

/// The engine's `Layout`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Layout")]
pub enum Layout {
    /// A flex box.
    Flex(Flex),
    /// A grid.
    Grid(Grid),
    /// Stacked.
    Stack,
}

/// The engine's `Grow`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Grow")]
pub enum Grow {
    /// Toward the start.
    Begin,
    /// Toward the end.
    End,
    /// Both ways.
    Both,
}

/// The engine's `Anchored`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Anchored")]
pub struct Anchored {
    /// Anchors (fractions of the parent).
    pub anchors: Edges,
    /// Offsets from the anchors (pixels).
    pub offsets: Edges,
    /// Horizontal growth.
    pub grow_h: Grow,
    /// Vertical growth.
    pub grow_v: Grow,
}

/// The engine's `Position`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Position")]
pub enum Position {
    /// In the parent's layout.
    InFlow,
    /// Anchored to the parent.
    Anchored(Anchored),
}

/// The engine's `PointerFilter`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "PointerFilter")]
pub enum PointerFilter {
    /// By content.
    Auto,
    /// Takes the pointer.
    Stop,
    /// Lets it through.
    Ignore,
}

/// The engine's `UiNode` (the rules change only `visible`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "UiNode")]
pub struct UiNode {
    /// Width.
    pub width: Length,
    /// Height.
    pub height: Length,
    /// Least width.
    pub min_width: Length,
    /// Least height.
    pub min_height: Length,
    /// Most width.
    pub max_width: Length,
    /// Most height.
    pub max_height: Length,
    /// Outside space.
    pub margin: Edges,
    /// Inside space.
    pub padding: Edges,
    /// Gaps between children.
    pub gap: [f32; 2],
    /// How children are laid out.
    pub layout: Layout,
    /// Where it sits.
    pub position: Position,
    /// Growth share.
    pub grow: f32,
    /// Shrink share.
    pub shrink: f32,
    /// Order among siblings.
    pub order: i32,
    /// Shown.
    pub visible: bool,
    /// Clips children.
    pub clip: bool,
    /// Draw order.
    pub z: i32,
    /// Pointer handling.
    pub pointer: PointerFilter,
}

/// The engine's `GradientStop`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "GradientStop")]
pub struct GradientStop {
    /// Position, 0..1.
    pub offset: f32,
    /// Linear RGBA.
    pub color: [f32; 4],
}

/// The engine's `Background`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "Background")]
pub enum Background {
    /// One colour.
    Solid([f32; 4]),
    /// A linear gradient.
    Linear {
        /// Direction in degrees.
        angle_deg: f32,
        /// Stops.
        stops: Vec<GradientStop>,
    },
    /// A radial gradient.
    Radial {
        /// Centre (fractions of the box).
        center: [f32; 2],
        /// Radii (fractions of the box).
        radius: [f32; 2],
        /// Stops.
        stops: Vec<GradientStop>,
    },
}

/// The engine's `FillMode`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "FillMode")]
pub enum FillMode {
    /// Left to right.
    LeftToRight,
    /// Right to left.
    RightToLeft,
    /// Top to bottom.
    TopToBottom,
    /// Bottom to top.
    BottomToTop,
    /// Clockwise.
    Clockwise,
    /// Counter-clockwise.
    CounterClockwise,
    /// Both ways horizontally.
    BilinearH,
    /// Both ways vertically.
    BilinearV,
    /// Clockwise and counter.
    ClockwiseAndCounter,
}

/// The engine's `RadialFill`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "RadialFill")]
pub struct RadialFill {
    /// Start angle.
    pub start_deg: f32,
    /// Sweep.
    pub sweep_deg: f32,
    /// Centre.
    pub center: [f32; 2],
}

/// The engine's `UiProgress` (the rules change only `value`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "UiProgress")]
pub struct UiProgress {
    /// The value.
    pub value: f32,
    /// The minimum.
    pub min: f32,
    /// The maximum.
    pub max: f32,
    /// How it fills.
    pub fill: FillMode,
    /// Radial settings.
    pub radial: RadialFill,
    /// Texture under.
    pub under: Option<String>,
    /// Texture of the fill.
    pub progress: Option<String>,
    /// Texture over.
    pub over: Option<String>,
    /// The track's fill.
    pub track: Option<Background>,
    /// The bar's fill.
    pub bar: Option<Background>,
    /// Tint.
    pub tint: [f32; 4],
}
