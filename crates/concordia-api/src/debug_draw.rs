//! Debug layers: world-space lines and triangles a module draws for a named layer (e.g. the
//! navmesh), which the renderer projects over a view like the other overlays.

use concordia_core::math::Vec3;

/// A layer's geometry for one world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DebugGeometry {
    /// Line segments.
    pub lines: Vec<DebugLine>,
    /// Filled triangles.
    pub triangles: Vec<DebugTriangle>,
}

/// A segment in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugLine {
    /// One end.
    pub from: Vec3,
    /// The other end.
    pub to: Vec3,
    /// Linear RGBA.
    pub color: [f32; 4],
}

/// A filled triangle in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugTriangle {
    /// Its corners.
    pub points: [Vec3; 3],
    /// Linear RGBA.
    pub color: [f32; 4],
}
