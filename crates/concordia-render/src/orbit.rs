//! The editor's orbit camera (Godot-style): middle-drag orbits, Shift+middle-drag pans, the
//! wheel zooms. Engine-side, so native viewport input and the web UI steer one camera.

use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::{Projection, View};

const FOV: f32 = 75.0 * std::f32::consts::PI / 180.0;
const PITCH_LIMIT: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
const ISO_PITCH: f32 = -35.264 * std::f32::consts::PI / 180.0;
/// Radians per pixel of orbit drag.
const ORBIT_SPEED: f32 = 0.005;

/// A named view direction (Godot's numpad views, plus isometric).
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Preset {
    /// Looking down -Z.
    front,
    /// Looking down -X.
    right,
    /// Looking down.
    top,
    /// Orthographic at true isometric angles.
    iso,
}

/// An orbit around `target`.
#[derive(Debug, Clone, PartialEq)]
pub struct OrbitCamera {
    /// The point orbited.
    pub target: Vec3,
    /// Rotation about +Y (radians).
    pub yaw: f32,
    /// Elevation; negative looks down (radians, clamped short of the poles).
    pub pitch: f32,
    /// Eye distance from the target.
    pub distance: f32,
    /// Orthographic instead of perspective.
    pub ortho: bool,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::new(0.0, 1.0, 0.0),
            yaw: 0.6,
            pitch: -0.45,
            distance: 14.0,
            ortho: false,
        }
    }
}

impl OrbitCamera {
    /// The engine view for this camera.
    pub fn view(&self) -> View {
        let dir = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            -self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        let far = (self.distance * 100.0).max(1000.0);
        let projection = if self.ortho {
            Projection::Orthographic {
                height: 2.0 * self.distance * (FOV / 2.0).tan(),
                near: 0.0,
                far: far + self.distance,
            }
        } else {
            Projection::Perspective {
                fov_y: FOV,
                near: (self.distance * 0.001).max(0.01),
                far,
            }
        };
        View {
            eye: self.target + dir * self.distance,
            target: self.target,
            projection,
        }
    }

    /// Orbits by a pointer drag of (dx, dy) pixels; non-finite input is ignored.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if dx.is_finite() && dy.is_finite() {
            self.yaw -= dx * ORBIT_SPEED;
            self.pitch = (self.pitch - dy * ORBIT_SPEED).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        }
    }

    /// Pans by a drag of (dx, dy) pixels in a viewport `height` pixels tall (> 0).
    pub fn pan(&mut self, dx: f32, dy: f32, height: f32) {
        if !(dx.is_finite() && dy.is_finite() && height > 0.0 && height.is_finite()) {
            return;
        }
        let view = self.view();
        let forward = (view.target - view.eye).normalize_or_zero();
        let up_hint = if forward.y.abs() > 0.999 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let right = forward.cross(up_hint).normalize_or_zero();
        let up = right.cross(forward);
        let units = match view.projection {
            Projection::Orthographic { height: h, .. } => h / height,
            Projection::Perspective { fov_y, .. } => {
                2.0 * self.distance * (fov_y / 2.0).tan() / height
            }
        };
        self.target += right * (-dx * units) + up * (dy * units);
    }

    /// Positive steps zoom out, negative zoom in; non-finite input is ignored.
    pub fn zoom(&mut self, steps: f32) {
        if steps.is_finite() {
            self.distance = (self.distance * 1.1f32.powf(steps)).clamp(0.05, 1e5);
        }
    }

    /// Jumps to a named view.
    pub fn preset(&mut self, preset: Preset) {
        (self.yaw, self.pitch) = match preset {
            Preset::front => (0.0, 0.0),
            Preset::right => (std::f32::consts::FRAC_PI_2, 0.0),
            Preset::top => (0.0, -PITCH_LIMIT),
            Preset::iso => {
                self.ortho = true;
                (std::f32::consts::FRAC_PI_4, ISO_PITCH)
            }
        };
    }
}
