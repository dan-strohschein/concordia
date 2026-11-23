//! Godot's `Control` anchor rules: each edge sits at `anchor × parent + offset`, and a rect
//! smaller than its minimum grows by its grow direction. Plus the 16 layout presets.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::geom::{Edges, Rect};
use crate::node::{Anchored, Grow};

/// Godot's 16 layout presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Preset {
    /// Top left corner.
    TopLeft,
    /// Top right corner.
    TopRight,
    /// Bottom left corner.
    BottomLeft,
    /// Bottom right corner.
    BottomRight,
    /// Middle of the left edge.
    CenterLeft,
    /// Middle of the top edge.
    CenterTop,
    /// Middle of the right edge.
    CenterRight,
    /// Middle of the bottom edge.
    CenterBottom,
    /// The center.
    Center,
    /// The left edge, full height.
    LeftWide,
    /// The top edge, full width.
    TopWide,
    /// The right edge, full height.
    RightWide,
    /// The bottom edge, full width.
    BottomWide,
    /// A vertical band through the center.
    VCenterWide,
    /// A horizontal band through the center.
    HCenterWide,
    /// The whole parent.
    FullRect,
}

/// Every preset, in Godot's order.
pub const PRESETS: [Preset; 16] = [
    Preset::TopLeft,
    Preset::TopRight,
    Preset::BottomLeft,
    Preset::BottomRight,
    Preset::CenterLeft,
    Preset::CenterTop,
    Preset::CenterRight,
    Preset::CenterBottom,
    Preset::Center,
    Preset::LeftWide,
    Preset::TopWide,
    Preset::RightWide,
    Preset::BottomWide,
    Preset::VCenterWide,
    Preset::HCenterWide,
    Preset::FullRect,
];

/// Where an anchor sits on one axis.
#[derive(Clone, Copy)]
enum Side {
    Begin,
    Middle,
    End,
    Wide,
}

impl Preset {
    fn sides(self) -> (Side, Side) {
        use Side::*;
        match self {
            Preset::TopLeft => (Begin, Begin),
            Preset::TopRight => (End, Begin),
            Preset::BottomLeft => (Begin, End),
            Preset::BottomRight => (End, End),
            Preset::CenterLeft => (Begin, Middle),
            Preset::CenterTop => (Middle, Begin),
            Preset::CenterRight => (End, Middle),
            Preset::CenterBottom => (Middle, End),
            Preset::Center => (Middle, Middle),
            Preset::LeftWide => (Begin, Wide),
            Preset::TopWide => (Wide, Begin),
            Preset::RightWide => (End, Wide),
            Preset::BottomWide => (Wide, End),
            Preset::VCenterWide => (Middle, Wide),
            Preset::HCenterWide => (Wide, Middle),
            Preset::FullRect => (Wide, Wide),
        }
    }
}

/// One axis of a preset: (anchor begin, anchor end, offset begin, offset end, grow).
fn axis(side: Side, size: f32, margin: f32) -> (f32, f32, f32, f32, Grow) {
    match side {
        Side::Begin => (0.0, 0.0, margin, margin + size, Grow::End),
        Side::End => (1.0, 1.0, -margin - size, -margin, Grow::Begin),
        Side::Middle => (0.5, 0.5, -size / 2.0, size / 2.0, Grow::Both),
        Side::Wide => (0.0, 1.0, margin, -margin, Grow::End),
    }
}

impl Anchored {
    /// A preset's anchors and offsets for a `size` box `margin` pixels from the parent's edges,
    /// with Godot's grow directions (right/bottom presets grow toward the start, centered ones
    /// both ways).
    pub fn preset(preset: Preset, size: [f32; 2], margin: f32) -> Anchored {
        let (h, v) = preset.sides();
        let (al, ar, ol, or, grow_h) = axis(h, size[0], margin);
        let (at, ab, ot, ob, grow_v) = axis(v, size[1], margin);
        Anchored {
            anchors: Edges::new(al, at, ar, ab),
            offsets: Edges::new(ol, ot, or, ob),
            grow_h,
            grow_v,
        }
    }
}

/// The rect `anchored` places inside `parent`, at least `min` in size.
pub fn anchored_rect(parent: Rect, anchored: &Anchored, min: [f32; 2]) -> Rect {
    let (a, o) = (&anchored.anchors, &anchored.offsets);
    let (x, w) = span(
        parent.x + a.left * parent.w + o.left,
        parent.x + a.right * parent.w + o.right,
        min[0],
        anchored.grow_h,
    );
    let (y, h) = span(
        parent.y + a.top * parent.h + o.top,
        parent.y + a.bottom * parent.h + o.bottom,
        min[1],
        anchored.grow_v,
    );
    Rect::new(x, y, w, h)
}

/// One axis's start and size: at least `min`, growing as `grow` says.
fn span(begin: f32, end: f32, min: f32, grow: Grow) -> (f32, f32) {
    let size = end - begin;
    let min = min.max(0.0);
    if size >= min {
        return (begin, size);
    }
    match grow {
        Grow::End => (begin, min),
        Grow::Begin => (end - min, min),
        Grow::Both => ((begin + end) / 2.0 - min / 2.0, min),
    }
}
