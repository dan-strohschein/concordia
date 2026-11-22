//! Editor overlays drawn by the engine in the viewport's own frame (ADR-0047): selection bounds
//! and the Move, Rotate and Scale gizmos, as screen-space triangles over the tonemapped image. The
//! gizmo geometry here is the one source for drawing and hit-testing.

use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Vec3};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::{RenderWorld, View};

/// A gizmo axis.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum GizmoAxis {
    /// World +X.
    x,
    /// World +Y.
    y,
    /// World +Z.
    z,
}

impl GizmoAxis {
    /// The world direction.
    pub fn vector(self) -> Vec3 {
        match self {
            Self::x => Vec3::X,
            Self::y => Vec3::Y,
            Self::z => Vec3::Z,
        }
    }

    /// The arrow's color (linear), Godot-like: red, green, blue; yellow when hovered.
    fn color(self, hovered: bool) -> [f32; 3] {
        linear(match (self, hovered) {
            (_, true) => [1.0, 0.85, 0.3],
            (Self::x, false) => [0.878, 0.314, 0.314],
            (Self::y, false) => [0.376, 0.753, 0.376],
            (Self::z, false) => [0.314, 0.502, 0.878],
        })
    }
}

/// What a gizmo does: arrows move, rings rotate, box-tipped arrows scale.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum GizmoMode {
    /// Translate along a world axis.
    #[default]
    r#move,
    /// Rotate about a world axis.
    rotate,
    /// Scale along a local axis.
    scale,
}

/// The transform gizmo on an entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GizmoOverlay {
    /// The entity it moves.
    pub entity: EntityId,
    /// Move (default), rotate or scale.
    #[serde(default)]
    pub mode: GizmoMode,
    /// The handle under the pointer, drawn highlighted.
    #[serde(default)]
    pub hovered: Option<GizmoAxis>,
}

/// What the engine draws over the viewport.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Overlays {
    /// Selected entities (outlined).
    #[serde(default)]
    pub selection: Vec<EntityId>,
    /// The Move gizmo, if shown.
    #[serde(default)]
    pub gizmo: Option<GizmoOverlay>,
    /// The ground grid.
    #[serde(default)]
    pub grid: bool,
    /// Every posed skeleton's bones, drawn on top (play world only: the edit world has no poses).
    #[serde(default)]
    pub skeleton: bool,
    /// Image pixels per UI point (the display's scale), so overlays keep their on-screen size.
    #[serde(default)]
    #[reflect(range(min = 0.5, max = 4.0))]
    pub scale: Option<f32>,
    /// How the native viewport draws the scene (lit, or a debug view).
    #[serde(default)]
    pub view_mode: crate::ViewMode,
}

/// One gizmo arrow on screen (pixels, y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GizmoHandle {
    /// Its axis.
    pub axis: GizmoAxis,
    /// The entity's position.
    pub origin: [f32; 2],
    /// The arrow's tip.
    pub end: [f32; 2],
}

/// A rotate ring on screen: a closed loop of points (pixels, y down).
#[derive(Debug, Clone, PartialEq)]
pub struct GizmoRing {
    /// The axis it rotates about.
    pub axis: GizmoAxis,
    /// Points around the loop (the last joins the first).
    pub points: Vec<[f32; 2]>,
}

/// Arrow length (and ring radius) on screen, in pixels (constant under zoom).
pub(crate) const ARROW: f32 = 60.0;
/// Segments per ring.
const RING_SEGMENTS: usize = 64;
/// Radius of an arrow's tip, in pixels.
const TIP: f32 = 7.0;
/// How close to an arrow (pixels) a press counts as on it.
const REACH: f32 = 8.0;

/// Projects world points into a `width`×`height` image (pixels, y down).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Projector {
    clip_from_world: Mat4,
    size: [f32; 2],
}

impl Projector {
    pub(crate) fn new(clip_from_world: Mat4, width: f32, height: f32) -> Self {
        Self {
            clip_from_world,
            size: [width, height],
        }
    }

    fn of(view: &View, width: f32, height: f32) -> Option<Self> {
        if !(width > 0.0 && height > 0.0) {
            return None;
        }
        let m = view.projection.matrix(width / height).ok()? * view.world_matrix().ok()?.inverse();
        Some(Self::new(m, width, height))
    }

    fn project(&self, point: Vec3) -> Option<[f32; 2]> {
        let clip = self.clip_from_world * point.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if !(0.0..=1.0).contains(&ndc.z) {
            return None;
        }
        Some([
            (ndc.x + 1.0) / 2.0 * self.size[0],
            (1.0 - ndc.y) / 2.0 * self.size[1],
        ])
    }
}

/// The gizmo's arrows for an entity at `at`; an axis pointing at the eye gets none.
pub fn gizmo_handles(view: &View, at: Vec3, width: f32, height: f32) -> Vec<GizmoHandle> {
    Projector::of(view, width, height).map_or_else(Vec::new, |p| handles(&p, at, 1.0))
}

fn handles(projector: &Projector, at: Vec3, scale: f32) -> Vec<GizmoHandle> {
    let Some(origin) = projector.project(at) else {
        return Vec::new();
    };
    [GizmoAxis::x, GizmoAxis::y, GizmoAxis::z]
        .into_iter()
        .filter_map(|axis| {
            let tip = projector.project(at + axis.vector())?;
            let (dx, dy) = (tip[0] - origin[0], tip[1] - origin[1]);
            let n = dx.hypot(dy);
            (n >= 1e-3).then(|| GizmoHandle {
                axis,
                origin,
                end: [
                    origin[0] + dx / n * ARROW * scale,
                    origin[1] + dy / n * ARROW * scale,
                ],
            })
        })
        .collect()
}

/// The gizmo's rings for an entity at `at`; a ring seen edge-on is a line through the entity.
pub fn gizmo_rings(view: &View, at: Vec3, width: f32, height: f32) -> Vec<GizmoRing> {
    Projector::of(view, width, height).map_or_else(Vec::new, |p| rings(&p, at, 1.0))
}

/// Each ring: a unit circle about the axis, projected, then scaled about the entity's screen
/// position so its longest radius is the arrow length.
fn rings(projector: &Projector, at: Vec3, scale: f32) -> Vec<GizmoRing> {
    let Some(origin) = projector.project(at) else {
        return Vec::new();
    };
    [GizmoAxis::x, GizmoAxis::y, GizmoAxis::z]
        .into_iter()
        .filter_map(|axis| {
            let (u, v) = axis.vector().any_orthonormal_pair();
            let projected: Vec<[f32; 2]> = (0..RING_SEGMENTS)
                .filter_map(|i| {
                    let t = i as f32 / RING_SEGMENTS as f32 * std::f32::consts::TAU;
                    projector.project(at + u * t.cos() + v * t.sin())
                })
                .map(|p| [p[0] - origin[0], p[1] - origin[1]])
                .collect();
            let reach = projected
                .iter()
                .map(|d| d[0].hypot(d[1]))
                .fold(0.0, f32::max);
            (projected.len() == RING_SEGMENTS && reach >= 1e-3).then(|| {
                let k = ARROW * scale / reach;
                let points = projected
                    .iter()
                    .map(|d| [origin[0] + d[0] * k, origin[1] + d[1] * k])
                    .collect();
                GizmoRing { axis, points }
            })
        })
        .collect()
}

/// The ring under the pointer at (x, y), nearest first.
pub fn ring_hit(rings: &[GizmoRing], x: f32, y: f32) -> Option<GizmoAxis> {
    rings
        .iter()
        .map(|r| {
            let n = r.points.len();
            let d = (0..n)
                .map(|i| distance_to_segment([x, y], r.points[i], r.points[(i + 1) % n]))
                .fold(f32::INFINITY, f32::min);
            (r.axis, d)
        })
        .filter(|(_, d)| *d <= REACH)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(axis, _)| axis)
}

/// The handle of a `mode` gizmo at `at` under the pointer: rings rotate, arrows move and scale.
pub fn gizmo_pick(
    mode: GizmoMode,
    view: &View,
    at: Vec3,
    [x, y]: [f32; 2],
    width: f32,
    height: f32,
) -> Option<GizmoAxis> {
    match mode {
        GizmoMode::rotate => ring_hit(&gizmo_rings(view, at, width, height), x, y),
        GizmoMode::r#move | GizmoMode::scale => {
            gizmo_hit(&gizmo_handles(view, at, width, height), x, y)
        }
    }
}

/// The arrow under the pointer at (x, y), nearest first.
pub fn gizmo_hit(handles: &[GizmoHandle], x: f32, y: f32) -> Option<GizmoAxis> {
    handles
        .iter()
        .map(|h| (h.axis, distance_to_segment([x, y], h.origin, h.end)))
        .filter(|(_, d)| *d <= REACH.max(TIP))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(axis, _)| axis)
}

fn distance_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (ab, ap) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 > 0.0 {
        ((ap[0] * ab[0] + ap[1] * ab[1]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - (a[0] + ab[0] * t)).hypot(p[1] - (a[1] + ab[1] * t))
}

/// Overlay triangles (x, y in pixels, then linear r, g, b, a per vertex) for one frame; `boxes`
/// holds each draw's world matrix and object-space bounds (posed when skinned), in draw order.
pub(crate) fn triangles(
    overlays: &Overlays,
    world: &RenderWorld,
    projector: &Projector,
    boxes: &[(Mat4, (Vec3, Vec3))],
) -> Vec<f32> {
    let mut out = Vec::new();
    let scale = overlays
        .scale
        .filter(|s| s.is_finite())
        .unwrap_or(1.0)
        .clamp(0.5, 4.0);
    let selection_color = linear([1.0, 0.6, 0.1]);
    for (draw, (at, (lo, hi))) in world.draws.iter().zip(boxes) {
        if overlays.selection.contains(&draw.entity) {
            box_edges(
                &mut out,
                projector,
                at,
                *lo,
                *hi,
                2.0 * scale,
                selection_color,
            );
        }
    }
    if overlays.skeleton {
        for skin in &world.skins {
            skeleton_lines(&mut out, projector, skin, scale);
        }
    }
    if let Some(gizmo) = &overlays.gizmo
        && let Some(at) = position_of(world, gizmo.entity)
    {
        gizmo_triangles(&mut out, projector, gizmo, at, scale);
    }
    out
}

/// A posed skeleton: a line from each joint to its parent and a dot on every joint.
fn skeleton_lines(out: &mut Vec<f32>, projector: &Projector, skin: &crate::SkinView, scale: f32) {
    let color = linear([0.35, 0.85, 1.0]);
    let at = |j: usize| {
        let m = skin.world * skin.joints[j];
        projector.project(m.transform_point3(Vec3::ZERO))
    };
    for (j, joint) in skin.skeleton.joints().iter().enumerate() {
        let Some(p) = at(j) else { continue };
        if let Some(q) = joint.parent.and_then(at) {
            line(out, q, p, 2.0 * scale, color);
        }
        disc(out, p, 3.0 * scale, color);
    }
}

/// The gizmo's shapes: arrows with round tips (move) or square tips (scale), or rings (rotate).
fn gizmo_triangles(
    out: &mut Vec<f32>,
    projector: &Projector,
    gizmo: &GizmoOverlay,
    at: Vec3,
    scale: f32,
) {
    let color = |axis: GizmoAxis| axis.color(gizmo.hovered == Some(axis));
    if gizmo.mode == GizmoMode::rotate {
        for ring in rings(projector, at, scale) {
            let n = ring.points.len();
            for i in 0..n {
                let (a, b) = (ring.points[i], ring.points[(i + 1) % n]);
                line(out, a, b, 3.0 * scale, color(ring.axis));
            }
        }
        return;
    }
    for h in handles(projector, at, scale) {
        line(out, h.origin, h.end, 4.0 * scale, color(h.axis));
        match gizmo.mode {
            GizmoMode::scale => square(out, h.end, TIP * scale, color(h.axis)),
            _ => disc(out, h.end, TIP * scale, color(h.axis)),
        }
    }
}

/// Where an entity is in the world (from its draw, camera or light).
fn position_of(world: &RenderWorld, entity: EntityId) -> Option<Vec3> {
    world
        .draws
        .iter()
        .find(|d| d.entity == entity)
        .map(|d| d.world.transform_point3(Vec3::ZERO))
        .or_else(|| {
            world
                .cameras
                .iter()
                .find(|c| c.entity == entity)
                .map(|c| c.world.transform_point3(Vec3::ZERO))
        })
        .or_else(|| {
            world
                .punctual
                .iter()
                .find(|l| l.entity == entity)
                .map(|l| l.position)
        })
}

fn box_edges(
    out: &mut Vec<f32>,
    p: &Projector,
    world: &Mat4,
    lo: Vec3,
    hi: Vec3,
    width: f32,
    color: [f32; 3],
) {
    let corner = |i: usize| {
        let pick = |bit: usize, a: f32, b: f32| if i & bit == 0 { a } else { b };
        world.transform_point3(Vec3::new(
            pick(1, lo.x, hi.x),
            pick(2, lo.y, hi.y),
            pick(4, lo.z, hi.z),
        ))
    };
    const EDGES: [(usize, usize); 12] = [
        (0, 1),
        (2, 3),
        (4, 5),
        (6, 7),
        (0, 2),
        (1, 3),
        (4, 6),
        (5, 7),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    for (a, b) in EDGES {
        if let (Some(a), Some(b)) = (p.project(corner(a)), p.project(corner(b))) {
            line(out, a, b, width, color);
        }
    }
}

fn vertex(out: &mut Vec<f32>, at: [f32; 2], color: [f32; 3]) {
    out.extend_from_slice(&[at[0], at[1], color[0], color[1], color[2], 1.0]);
}

/// A `width`-pixel line as two triangles.
fn line(out: &mut Vec<f32>, a: [f32; 2], b: [f32; 2], width: f32, color: [f32; 3]) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let n = dx.hypot(dy).max(1e-6);
    let (nx, ny) = (-dy / n * width / 2.0, dx / n * width / 2.0);
    let q = [
        [a[0] + nx, a[1] + ny],
        [b[0] + nx, b[1] + ny],
        [b[0] - nx, b[1] - ny],
        [a[0] - nx, a[1] - ny],
    ];
    for i in [0, 1, 2, 0, 2, 3] {
        vertex(out, q[i], color);
    }
}

/// A filled disc as a 16-sided fan.
fn disc(out: &mut Vec<f32>, at: [f32; 2], radius: f32, color: [f32; 3]) {
    let point = |i: usize| {
        let t = i as f32 / 16.0 * std::f32::consts::TAU;
        [at[0] + radius * t.cos(), at[1] + radius * t.sin()]
    };
    for i in 0..16 {
        vertex(out, at, color);
        vertex(out, point(i), color);
        vertex(out, point(i + 1), color);
    }
}

/// A filled square of half-size `half`, axis-aligned on screen.
fn square(out: &mut Vec<f32>, at: [f32; 2], half: f32, color: [f32; 3]) {
    let c = [
        [at[0] - half, at[1] - half],
        [at[0] + half, at[1] - half],
        [at[0] + half, at[1] + half],
        [at[0] - half, at[1] + half],
    ];
    for i in [0, 1, 2, 0, 2, 3] {
        vertex(out, c[i], color);
    }
}

/// Floats per overlay vertex.
pub(crate) const VERTEX_FLOATS: usize = 6;

/// An sRGB color as linear (overlays draw into sRGB targets, which encode on write).
fn linear(srgb: [f32; 3]) -> [f32; 3] {
    srgb.map(|c| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    })
}
