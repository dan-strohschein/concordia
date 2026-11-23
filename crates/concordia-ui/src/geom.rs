//! UI-space geometry: logical pixels, origin top left, y down.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// A rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// A rect from its corner and size.
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// The right edge.
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    /// The bottom edge.
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// The overlap of two rects (empty rects have zero size).
    pub fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        Rect::new(x, y, (r - x).max(0.0), (b - y).max(0.0))
    }

    /// Shrunk by `edges` on each side (never below zero size).
    pub fn inset(&self, edges: &Edges) -> Rect {
        Rect::new(
            self.x + edges.left,
            self.y + edges.top,
            (self.w - edges.left - edges.right).max(0.0),
            (self.h - edges.top - edges.bottom).max(0.0),
        )
    }

    /// Whether `[x, y]` lies inside (left and top edges inclusive).
    pub fn contains(&self, point: [f32; 2]) -> bool {
        point[0] >= self.x
            && point[0] < self.right()
            && point[1] >= self.y
            && point[1] < self.bottom()
    }
}

/// A value per side.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
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

impl Edges {
    /// The same value on every side.
    pub const fn all(v: f32) -> Self {
        Self::new(v, v, v, v)
    }

    /// Left, top, right, bottom.
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// The four values in order.
    pub fn values(&self) -> [f32; 4] {
        [self.left, self.top, self.right, self.bottom]
    }
}
