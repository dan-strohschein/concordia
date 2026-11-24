//! Photometric units shared by the renderer and the path tracer.

/// Luminous intensity (cd) of `lumens` radiated uniformly (spots are not concentrated by their cone).
pub fn candela(lumens: f32) -> f32 {
    lumens / (4.0 * std::f32::consts::PI)
}

/// Inverse-square falloff windowed smoothly to zero at `range`: (1 − (d²/r²)²)² / d², the
/// renderer's `distance_attenuation` (d² floored at 1e-4).
pub fn distance_window(d2: f32, range: f32) -> f32 {
    let r = d2 / (range * range);
    let w = (1.0 - r * r).clamp(0.0, 1.0);
    w * w / d2.max(1e-4)
}

/// A spot light's cone: attenuation saturate(cos θ · scale + offset)², full inside the inner
/// half-angle and zero outside the outer one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpotCone {
    /// 1 / (cos inner − cos outer).
    pub scale: f32,
    /// −cos outer · scale.
    pub offset: f32,
}

impl SpotCone {
    /// The cone between half-angles `inner` and `outer` (radians).
    pub fn new(inner: f32, outer: f32) -> Self {
        let (ci, co) = (inner.cos(), outer.cos());
        let scale = 1.0 / (ci - co).max(1e-4);
        Self {
            scale,
            offset: -co * scale,
        }
    }

    /// The attenuation at `cos_theta` from the spot's axis.
    pub fn attenuation(&self, cos_theta: f32) -> f32 {
        let t = (cos_theta * self.scale + self.offset).clamp(0.0, 1.0);
        t * t
    }
}
