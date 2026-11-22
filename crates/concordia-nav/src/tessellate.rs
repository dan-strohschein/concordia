//! Collider shapes as triangles, from a fixed 16-step table (no trigonometry, so builds are
//! byte-identical wherever the arithmetic is IEEE).

use concordia_core::math::Vec3;
use concordia_physics::ColliderShape;

/// Steps around a circle.
pub const SEGMENTS: usize = 16;

/// cos and sin of 2πk/16 for k in 0..4 (the rest follow by symmetry).
const QUARTER: [(f32, f32); 4] = [
    (1.0, 0.0),
    (0.923_879_5, 0.382_683_43),
    (
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    ),
    (0.382_683_43, 0.923_879_5),
];

/// cos and sin of 2πk/16, from a fixed table (no trigonometry).
pub fn unit(k: usize) -> (f32, f32) {
    let (c, s) = QUARTER[k % 4];
    match (k / 4) % 4 {
        0 => (c, s),
        1 => (-s, c),
        2 => (-c, -s),
        _ => (s, -c),
    }
}

/// A shape's triangles in local space, facing outward.
pub(crate) fn triangles(shape: &ColliderShape) -> Vec<[Vec3; 3]> {
    let raw = match *shape {
        ColliderShape::Box { half_extents } => cuboid(half_extents),
        ColliderShape::Cylinder {
            half_height,
            radius,
        } => lathe(&[
            (0.0, half_height),
            (radius, half_height),
            (radius, -half_height),
            (0.0, -half_height),
        ]),
        ColliderShape::Sphere { radius } => lathe(&arc(radius, 0.0, 0..=8)),
        ColliderShape::Capsule {
            half_height,
            radius,
        } => {
            let mut profile = arc(radius, half_height, 0..=4);
            profile.extend(arc(radius, -half_height, 4..=8));
            lathe(&profile)
        }
    };
    raw.into_iter().filter_map(outward).collect()
}

/// Profile points (radius, y) from the top down: polar angle πi/8 for each i.
fn arc(radius: f32, center_y: f32, steps: std::ops::RangeInclusive<usize>) -> Vec<(f32, f32)> {
    steps
        .map(|i| {
            let (cos, sin) = unit(i);
            (radius * sin, center_y + radius * cos)
        })
        .collect()
}

/// A surface of revolution about y from a (radius, y) profile.
fn lathe(profile: &[(f32, f32)]) -> Vec<[Vec3; 3]> {
    let at = |(r, y): (f32, f32), k: usize| {
        let (cos, sin) = unit(k);
        Vec3::new(r * cos, y, r * sin)
    };
    let mut out = Vec::new();
    for pair in profile.windows(2) {
        for k in 0..SEGMENTS {
            let (a0, a1) = (at(pair[0], k), at(pair[0], k + 1));
            let (b0, b1) = (at(pair[1], k), at(pair[1], k + 1));
            out.push([a0, a1, b1]);
            out.push([a0, b1, b0]);
        }
    }
    out
}

fn cuboid(h: Vec3) -> Vec<[Vec3; 3]> {
    let c = |x: f32, y: f32, z: f32| Vec3::new(x * h.x, y * h.y, z * h.z);
    let faces = [
        [
            c(-1.0, 1.0, -1.0),
            c(1.0, 1.0, -1.0),
            c(1.0, 1.0, 1.0),
            c(-1.0, 1.0, 1.0),
        ],
        [
            c(-1.0, -1.0, -1.0),
            c(1.0, -1.0, -1.0),
            c(1.0, -1.0, 1.0),
            c(-1.0, -1.0, 1.0),
        ],
        [
            c(1.0, -1.0, -1.0),
            c(1.0, 1.0, -1.0),
            c(1.0, 1.0, 1.0),
            c(1.0, -1.0, 1.0),
        ],
        [
            c(-1.0, -1.0, -1.0),
            c(-1.0, 1.0, -1.0),
            c(-1.0, 1.0, 1.0),
            c(-1.0, -1.0, 1.0),
        ],
        [
            c(-1.0, -1.0, 1.0),
            c(1.0, -1.0, 1.0),
            c(1.0, 1.0, 1.0),
            c(-1.0, 1.0, 1.0),
        ],
        [
            c(-1.0, -1.0, -1.0),
            c(1.0, -1.0, -1.0),
            c(1.0, 1.0, -1.0),
            c(-1.0, 1.0, -1.0),
        ],
    ];
    faces
        .iter()
        .flat_map(|[a, b, c, d]| [[*a, *b, *c], [*a, *c, *d]])
        .collect()
}

/// The triangle facing away from the (convex) shape's centre, or `None` if degenerate.
fn outward([a, b, c]: [Vec3; 3]) -> Option<[Vec3; 3]> {
    let normal = (b - a).cross(c - a);
    let toward = normal.dot(a + b + c);
    match toward {
        t if t > 0.0 => Some([a, b, c]),
        t if t < 0.0 => Some([a, c, b]),
        _ => None,
    }
}
