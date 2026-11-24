//! Photometric units shared by the renderer and the path tracer.

/// Luminous intensity (cd) of `lumens` radiated uniformly (spots are not concentrated by their cone).
pub fn candela(lumens: f32) -> f32 {
    lumens / (4.0 * std::f32::consts::PI)
}
