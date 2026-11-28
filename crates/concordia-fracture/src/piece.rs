//! What a piece needs for play: its centre of mass, collision hull and interior UVs.

use concordia_core::math::Vec3;
use concordia_physics::ColliderShape;

use crate::{FractureError, Piece, Slot};

impl Piece {
    /// The centre of mass of the piece, as a solid of uniform density.
    pub fn centre_of_mass(&self) -> Vec3 {
        let Some(&origin) = self.positions.first() else {
            return Vec3::ZERO;
        };
        let o = origin.map(f64::from);
        let (mut volume, mut moment) = (0.0f64, [0.0f64; 3]);
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| {
                let p = self.positions[i as usize];
                [0, 1, 2].map(|k| f64::from(p[k]) - o[k])
            });
            // Six times the signed volume of the tetrahedron (origin, a, b, c).
            let v = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]);
            volume += v;
            for k in 0..3 {
                moment[k] += v * (a[k] + b[k] + c[k]) / 4.0;
            }
        }
        if volume.abs() < f64::EPSILON {
            return Vec3::from_array(origin);
        }
        Vec3::from_array([0, 1, 2].map(|k| (o[k] + moment[k] / volume) as f32))
    }

    /// The piece's collider: the convex hull of its vertices, in piece space (relative to its
    /// centre of mass). Concave pieces get their hull; convex decomposition is later work.
    pub fn collider(&self) -> Result<ColliderShape, FractureError> {
        let centre = self.centre_of_mass();
        let mut points: Vec<[f32; 3]> = self.positions.clone();
        points.sort_by(|a, b| {
            a.iter()
                .zip(b)
                .map(|(x, y)| x.total_cmp(y))
                .find(|o| o.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        points.dedup();
        if points.len() < 4 {
            return Err(FractureError::Hull(format!(
                "{} distinct vertices; a hull needs 4",
                points.len()
            )));
        }
        Ok(ColliderShape::ConvexHull {
            points: points
                .into_iter()
                .map(|p| Vec3::from_array(p) - centre)
                .collect(),
        })
    }

    /// UVs for each triangle's corners: interior faces get a planar projection in world space,
    /// `metres_per_tile` metres to one UV unit, so both sides of a cut line up; surface faces get
    /// zeros (their own UVs come with the source mesh).
    pub fn interior_uvs(&self, metres_per_tile: f32) -> Result<Vec<[[f32; 2]; 3]>, FractureError> {
        if !(metres_per_tile.is_finite() && metres_per_tile > 0.0) {
            return Err(FractureError::InvalidParameter(format!(
                "`metres_per_tile` must be a positive number, not {metres_per_tile}"
            )));
        }
        Ok(self
            .triangles
            .iter()
            .zip(&self.materials)
            .map(|(t, slot)| match slot {
                Slot::Surface => [[0.0; 2]; 3],
                Slot::Interior => {
                    let [a, b, c] = t.map(|i| Vec3::from_array(self.positions[i as usize]));
                    let (u, v) = basis((b - a).cross(c - a));
                    [a, b, c].map(|p| [p.dot(u) / metres_per_tile, p.dot(v) / metres_per_tile])
                }
            })
            .collect())
    }
}

/// Two tangent axes for a face normal, the same for either side of the face: the normal is
/// flipped so its largest component is positive.
fn basis(normal: Vec3) -> (Vec3, Vec3) {
    let n = normal.normalize_or_zero();
    let a = n.abs();
    let largest = if a.x >= a.y && a.x >= a.z {
        n.x
    } else if a.y >= a.z {
        n.y
    } else {
        n.z
    };
    let n = if largest < 0.0 { -n } else { n };
    let up = if n.y.abs() < 0.99 { Vec3::Y } else { Vec3::X };
    let u = up.cross(n).normalize_or_zero();
    (u, n.cross(u))
}
