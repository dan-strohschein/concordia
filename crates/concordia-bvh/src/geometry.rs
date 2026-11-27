//! Rays, triangles and their intersection.

use concordia_core::math::Vec3;

/// A ray from `origin` along `dir`, considered on (`t_min`, `t_max`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    /// Start.
    pub origin: Vec3,
    /// Direction (need not be unit length; `t` is in its units).
    pub dir: Vec3,
    /// Nearest distance that counts.
    pub t_min: f32,
    /// Farthest distance that counts.
    pub t_max: f32,
}

/// A triangle by its corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle {
    /// Corners in winding order.
    pub v: [Vec3; 3],
}

/// Where a ray met a triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// Distance along the ray.
    pub t: f32,
    /// Barycentric weight of corner 1.
    pub u: f32,
    /// Barycentric weight of corner 2.
    pub w: f32,
    /// The triangle's index.
    pub prim: u32,
}

impl Triangle {
    /// The ray's hit on this triangle (either side), if inside (`t_min`, `t_max`).
    pub fn intersect(&self, ray: &Ray, prim: u32) -> Option<Hit> {
        // Möller & Trumbore (1997), both faces.
        let [a, b, c] = self.v;
        let (e1, e2) = (b - a, c - a);
        let p = ray.dir.cross(e2);
        let det = e1.dot(p);
        if det.abs() < f32::MIN_POSITIVE || !det.is_finite() {
            return None;
        }
        let inv = 1.0 / det;
        let s = ray.origin - a;
        let u = s.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = s.cross(e1);
        let w = ray.dir.dot(q) * inv;
        if w < 0.0 || u + w > 1.0 {
            return None;
        }
        let t = e2.dot(q) * inv;
        (t > ray.t_min && t < ray.t_max).then_some(Hit { t, u, w, prim })
    }

    /// The triangle's bounds (min, max).
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let [a, b, c] = self.v;
        (a.min(b).min(c), a.max(b).max(c))
    }

    /// The centroid.
    pub fn centroid(&self) -> Vec3 {
        (self.v[0] + self.v[1] + self.v[2]) / 3.0
    }
}
