//! `UiNode`: a widget's box. Its size, spacing and how it lays out its children (taffy flex,
//! grid or stack), or anchors that pin it inside its parent (Godot's `Control` semantics).

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::{UiError, finite, in_range};
use crate::geom::Edges;

const NAME: &str = "UiNode";
/// The most tracks a grid may declare per axis.
pub const MAX_TRACKS: usize = 64;

/// A length along one axis.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub enum Length {
    /// From the content or the layout.
    #[default]
    Auto,
    /// Logical pixels (≥ 0).
    Px(f32),
    /// Percent of the parent's content box (0..100).
    Percent(f32),
}

impl Length {
    fn check(&self, field: &str) -> Result<(), UiError> {
        match *self {
            Length::Auto => Ok(()),
            Length::Px(v) => in_range(NAME, field, v, 0.0, f32::MAX),
            Length::Percent(v) => in_range(NAME, field, v, 0.0, 100.0),
        }
    }

    /// The pixel value, if this is `Px`.
    pub fn px(&self) -> Option<f32> {
        match *self {
            Length::Px(v) => Some(v),
            _ => None,
        }
    }
}

/// How a node lays out its in-flow children.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Layout {
    /// CSS Flexbox.
    Flex(Flex),
    /// CSS Grid with explicit tracks.
    Grid(Grid),
    /// Children overlap, each filling the content box (like Godot's `PanelContainer`).
    Stack,
}

impl Default for Layout {
    fn default() -> Self {
        Layout::Flex(Flex::default())
    }
}

/// Flexbox settings.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
pub struct Flex {
    /// The main axis.
    pub direction: Direction,
    /// Wrap onto more lines.
    pub wrap: bool,
    /// Placement along the main axis.
    pub justify: Justify,
    /// Placement across it.
    pub align_items: Align,
    /// Placement of wrapped lines.
    pub align_content: Justify,
}

/// A flex main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Direction {
    /// Left to right.
    #[default]
    Row,
    /// Top to bottom.
    Column,
    /// Right to left.
    RowReverse,
    /// Bottom to top.
    ColumnReverse,
}

/// Distribution along an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Justify {
    /// Packed at the start.
    #[default]
    Start,
    /// Packed at the end.
    End,
    /// Centered.
    Center,
    /// Even gaps between, none at the ends.
    SpaceBetween,
    /// Half gaps at the ends.
    SpaceAround,
    /// Equal gaps everywhere.
    SpaceEvenly,
}

/// Alignment across an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Align {
    /// Filling the line.
    #[default]
    Stretch,
    /// At the start.
    Start,
    /// At the end.
    End,
    /// Centered.
    Center,
}

/// Grid settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
pub struct Grid {
    /// Column tracks (children fill row by row).
    pub columns: Vec<Track>,
    /// Row tracks (more rows are added at `Auto`).
    pub rows: Vec<Track>,
    /// Cell alignment across columns.
    pub justify_items: Align,
    /// Cell alignment across rows.
    pub align_items: Align,
}

/// A grid track's size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Track {
    /// Logical pixels.
    Px(f32),
    /// A share of the free space.
    Fr(f32),
    /// Percent of the grid's content box.
    Percent(f32),
    /// Fits its content.
    Auto,
}

/// Where a node goes: in its parent's flow, or pinned by anchors.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub enum Position {
    /// Laid out by the parent's layout.
    #[default]
    InFlow,
    /// Pinned inside the parent by anchors and offsets.
    Anchored(Anchored),
}

/// Godot-style anchors: each edge sits at `anchor × parent size + offset`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
pub struct Anchored {
    /// Anchor fractions of the parent (0..1).
    pub anchors: Edges,
    /// Pixel offsets from the anchors.
    pub offsets: Edges,
    /// Which side moves when the rect is narrower than the minimum width.
    pub grow_h: Grow,
    /// Which side moves when the rect is shorter than the minimum height.
    pub grow_v: Grow,
}

/// Which way a too-small anchored rect grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Grow {
    /// Toward the left or top.
    Begin,
    /// Toward the right or bottom.
    #[default]
    End,
    /// Both ways, about the center.
    Both,
}

/// A widget's box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiNode {
    /// Width (ignored when anchored).
    pub width: Length,
    /// Height (ignored when anchored).
    pub height: Length,
    /// Minimum width.
    pub min_width: Length,
    /// Minimum height.
    pub min_height: Length,
    /// Maximum width.
    pub max_width: Length,
    /// Maximum height.
    pub max_height: Length,
    /// Space outside the box.
    pub margin: Edges,
    /// Space inside the box, around the children.
    pub padding: Edges,
    /// Gaps between children `[column, row]`.
    pub gap: [f32; 2],
    /// How the children lay out.
    pub layout: Layout,
    /// In flow, or anchored.
    pub position: Position,
    /// Flex grow factor.
    #[reflect(range(min = 0.0, max = 1000.0))]
    pub grow: f32,
    /// Flex shrink factor.
    #[reflect(range(min = 0.0, max = 1000.0))]
    pub shrink: f32,
    /// Sibling order (then entity id).
    pub order: i32,
    /// Drawn and laid out.
    pub visible: bool,
    /// Clips the children to this box.
    pub clip: bool,
    /// Draw order among siblings (higher on top).
    pub z: i32,
    /// Whether the pointer stops here or passes to what is below.
    pub pointer: PointerFilter,
}

/// Whether a node takes the pointer (Godot's `mouse_filter`, for semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum PointerFilter {
    /// Interactive widgets and boxes with a background stop it; the rest pass it on.
    #[default]
    Auto,
    /// Always stops it (the click never reaches the world).
    Stop,
    /// Never stops it.
    Ignore,
}

impl Default for UiNode {
    fn default() -> Self {
        Self {
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Auto,
            min_height: Length::Auto,
            max_width: Length::Auto,
            max_height: Length::Auto,
            margin: Edges::default(),
            padding: Edges::default(),
            gap: [0.0; 2],
            layout: Layout::default(),
            position: Position::InFlow,
            grow: 0.0,
            shrink: 1.0,
            order: 0,
            visible: true,
            clip: false,
            z: 0,
            pointer: PointerFilter::Auto,
        }
    }
}

impl UiNode {
    /// Checks every field (finite values, ranges, min ≤ max, anchors in order).
    pub fn check(&self) -> Result<(), UiError> {
        self.width.check("width")?;
        self.height.check("height")?;
        self.min_width.check("min_width")?;
        self.min_height.check("min_height")?;
        self.max_width.check("max_width")?;
        self.max_height.check("max_height")?;
        min_le_max("width", self.min_width, self.max_width)?;
        min_le_max("height", self.min_height, self.max_height)?;
        check_edges("margin", &self.margin, f32::MIN, f32::MAX)?;
        check_edges("padding", &self.padding, 0.0, f32::MAX)?;
        in_range(NAME, "gap/0", self.gap[0], 0.0, f32::MAX)?;
        in_range(NAME, "gap/1", self.gap[1], 0.0, f32::MAX)?;
        in_range(NAME, "grow", self.grow, 0.0, 1000.0)?;
        in_range(NAME, "shrink", self.shrink, 0.0, 1000.0)?;
        check_layout(&self.layout)?;
        match &self.position {
            Position::InFlow => Ok(()),
            Position::Anchored(a) => self.check_anchored(a),
        }
    }

    fn check_anchored(&self, a: &Anchored) -> Result<(), UiError> {
        check_edges("position/anchors", &a.anchors, 0.0, 1.0)?;
        check_edges("position/offsets", &a.offsets, f32::MIN, f32::MAX)?;
        if a.anchors.left > a.anchors.right {
            return Err(UiError::field(
                NAME,
                "position/anchors",
                "left must not exceed right",
            ));
        }
        if a.anchors.top > a.anchors.bottom {
            return Err(UiError::field(
                NAME,
                "position/anchors",
                "top must not exceed bottom",
            ));
        }
        if self.width != Length::Auto || self.height != Length::Auto {
            return Err(UiError::field(
                NAME,
                "width",
                "an anchored node's size comes from its anchors and offsets; leave width and height Auto",
            ));
        }
        Ok(())
    }
}

fn min_le_max(axis: &str, min: Length, max: Length) -> Result<(), UiError> {
    match (min.px(), max.px()) {
        (Some(lo), Some(hi)) if lo > hi => Err(UiError::field(
            NAME,
            format!("min_{axis}"),
            format!("{lo} exceeds max_{axis} {hi}"),
        )),
        _ => Ok(()),
    }
}

fn check_edges(field: &str, edges: &Edges, min: f32, max: f32) -> Result<(), UiError> {
    finite(NAME, field, &edges.values())?;
    for (side, v) in ["left", "top", "right", "bottom"]
        .iter()
        .zip(edges.values())
    {
        in_range(NAME, &format!("{field}/{side}"), v, min, max)?;
    }
    Ok(())
}

fn check_layout(layout: &Layout) -> Result<(), UiError> {
    let Layout::Grid(grid) = layout else {
        return Ok(());
    };
    for (axis, tracks) in [("columns", &grid.columns), ("rows", &grid.rows)] {
        if tracks.len() > MAX_TRACKS {
            return Err(UiError::field(
                NAME,
                format!("layout/{axis}"),
                format!("{} tracks; at most {MAX_TRACKS}", tracks.len()),
            ));
        }
        for (i, t) in tracks.iter().enumerate() {
            let field = format!("layout/{axis}/{i}");
            match *t {
                Track::Px(v) | Track::Fr(v) => in_range(NAME, &field, v, 0.0, f32::MAX)?,
                Track::Percent(v) => in_range(NAME, &field, v, 0.0, 100.0)?,
                Track::Auto => {}
            }
        }
    }
    Ok(())
}
