//! Integration, from DetourCrowd's `integrate` (zlib source), with the end clamp of 2.5b.1's
//! follower: a tick never carries an agent past its path's end.

use concordia_core::math::Vec3;

use crate::steer::flat;

/// One tick of integration: the new velocity and the candidate position.
pub(crate) fn integrate(
    position: Vec3,
    velocity: Vec3,
    chosen: Vec3,
    max_acceleration: f32,
    dt: f32,
    end: Option<Vec3>,
) -> (Vec3, Vec3) {
    if !(dt.is_finite() && dt > 0.0) {
        return (velocity, position);
    }
    let mut dv = chosen - velocity;
    let limit = max_acceleration * dt;
    let ds = dv.length();
    if ds > limit {
        dv *= limit / ds;
    }
    let velocity = velocity + dv;
    if velocity.length() <= 0.0001 {
        return (Vec3::ZERO, position);
    }
    let mut step = flat(velocity * dt);
    if let Some(end) = end {
        let left = flat(end - position).length();
        let len = step.length();
        if len > left {
            step *= left / len;
        }
    }
    (velocity, position + step)
}
