//! `UiRoot`: the top of a UI tree, and how its base canvas maps onto the viewport (Godot's
//! `canvas_items` stretch mode and aspect settings).

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::{UiError, in_range};

const NAME: &str = "UiRoot";
/// The largest canvas side.
pub const MAX_CANVAS: f32 = 16_384.0;

/// Whether the canvas scales with the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Stretch {
    /// UI space is the viewport, at scale 1.
    #[default]
    Disabled,
    /// The canvas scales to the viewport by `aspect`.
    Scale,
}

/// How a scaled canvas keeps its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Aspect {
    /// Stretched to fill, non-uniformly.
    Ignore,
    /// Uniform, with bars.
    #[default]
    Keep,
    /// Uniform; taller viewports show more height.
    KeepWidth,
    /// Uniform; wider viewports show more width.
    KeepHeight,
    /// Uniform; every viewport shows more of the canvas, no bars.
    Expand,
}

/// Where a root draws.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub enum RootSpace {
    /// Over the screen.
    #[default]
    Screen,
    /// At its entity's position in the world (2.7b.3).
    World {
        /// World-space offset from the entity.
        offset: [f32; 3],
        /// Screen pixels per UI pixel.
        pixel_size: f32,
    },
}

/// The top of a UI tree: its children are the entity's `Parent` children.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "UI", default)]
#[serde(default, deny_unknown_fields)]
pub struct UiRoot {
    /// The base canvas size in logical pixels.
    pub canvas: [f32; 2],
    /// Whether the canvas scales.
    pub stretch: Stretch,
    /// How a scaled canvas keeps its shape.
    pub aspect: Aspect,
    /// Screen or world.
    pub space: RootSpace,
    /// Draw order among roots (higher on top).
    pub z: i32,
}

impl Default for UiRoot {
    fn default() -> Self {
        Self {
            canvas: [1280.0, 720.0],
            stretch: Stretch::Disabled,
            aspect: Aspect::Keep,
            space: RootSpace::Screen,
            z: 0,
        }
    }
}

impl UiRoot {
    /// Checks the canvas and the world space's pixel size.
    pub fn check(&self) -> Result<(), UiError> {
        for v in self.canvas {
            in_range(NAME, "canvas", v, f32::MIN_POSITIVE, MAX_CANVAS)?;
        }
        match self.space {
            RootSpace::Screen => Ok(()),
            RootSpace::World { offset, pixel_size } => {
                crate::error::finite(NAME, "space/offset", &offset)?;
                in_range(
                    NAME,
                    "space/pixel_size",
                    pixel_size,
                    f32::MIN_POSITIVE,
                    1.0e6,
                )
            }
        }
    }
}

/// How UI space maps to the viewport: `viewport = ui × scale + offset`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct CanvasMap {
    /// Viewport pixels per UI pixel, per axis.
    pub scale: [f32; 2],
    /// Where UI (0, 0) lands, in viewport logical pixels.
    pub offset: [f32; 2],
    /// The UI space's size.
    pub size: [f32; 2],
}

impl CanvasMap {
    /// Maps a UI point to the viewport.
    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        [
            p[0] * self.scale[0] + self.offset[0],
            p[1] * self.scale[1] + self.offset[1],
        ]
    }

    /// Maps a viewport point back to UI space.
    pub fn unapply(&self, p: [f32; 2]) -> [f32; 2] {
        [
            (p[0] - self.offset[0]) / self.scale[0],
            (p[1] - self.offset[1]) / self.scale[1],
        ]
    }
}

/// How `root`'s canvas maps onto a `viewport` (logical pixels).
pub fn canvas_map(root: &UiRoot, viewport: [f32; 2]) -> CanvasMap {
    let [bw, bh] = root.canvas;
    let [vw, vh] = viewport;
    if root.stretch == Stretch::Disabled || bw <= 0.0 || bh <= 0.0 {
        return CanvasMap {
            scale: [1.0, 1.0],
            offset: [0.0, 0.0],
            size: viewport,
        };
    }
    let (sx, sy) = (vw / bw, vh / bh);
    let s = sx.min(sy);
    let size = match root.aspect {
        Aspect::Ignore => {
            return CanvasMap {
                scale: [sx, sy],
                offset: [0.0, 0.0],
                size: root.canvas,
            };
        }
        Aspect::Keep => root.canvas,
        Aspect::KeepWidth if sx <= sy => [bw, vh / s],
        Aspect::KeepHeight if sy <= sx => [vw / s, bh],
        Aspect::KeepWidth | Aspect::KeepHeight => root.canvas,
        Aspect::Expand => [vw / s, vh / s],
    };
    CanvasMap {
        scale: [s, s],
        offset: [(vw - size[0] * s) / 2.0, (vh - size[1] * s) / 2.0],
        size,
    }
}
