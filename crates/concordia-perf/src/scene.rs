//! `tests/perf/scenes/<name>.perf.ron`: what to measure: a project's scene, a size, frame counts
//! and a camera path.

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::PerfError;

/// One camera pose on the path.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct CameraKey {
    /// Camera position.
    pub eye: [f32; 3],
    /// Point looked at.
    pub target: [f32; 3],
}

/// A scene to measure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PerfScene {
    /// The project folder, relative to this file.
    pub project: String,
    /// The scene, relative to the project.
    pub scene: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Frames drawn before measuring (shader compiles, caches, clocks).
    pub warmup_frames: u32,
    /// Frames measured.
    pub frames: u32,
    /// Vertical field of view (radians); the scene camera's exposure, MSAA and tonemap apply.
    pub fov_y: f32,
    /// Poses the measured frames move through, evenly and linearly (warm-up holds the first).
    pub path: Vec<CameraKey>,
}

impl PerfScene {
    /// Reads and validates a perf scene's text.
    pub fn read(text: &str) -> Result<Self, PerfError> {
        let scene: Self =
            concordia_scene::read_document(text).map_err(|e| PerfError::Invalid(e.to_string()))?;
        scene.validate()?;
        Ok(scene)
    }

    /// At least one frame and one key, a drawable size and a field of view in (0, π).
    pub fn validate(&self) -> Result<(), PerfError> {
        let fail = |m: String| Err(PerfError::Invalid(m));
        if self.frames == 0 {
            return fail("frames must be at least 1".into());
        }
        if self.width == 0 || self.height == 0 {
            return fail(format!("size {}×{} is empty", self.width, self.height));
        }
        if !(self.fov_y > 0.0 && self.fov_y < std::f32::consts::PI) {
            return fail(format!("fov_y {} is not in (0, π) radians", self.fov_y));
        }
        if self.path.is_empty() {
            return fail("path needs at least one camera key".into());
        }
        Ok(())
    }

    /// The pose for measured frame `i` of `frames`.
    pub fn pose(&self, i: u32, frames: u32) -> CameraKey {
        let last = self.path.len() - 1;
        let t = match frames {
            0 | 1 => 0.0,
            n => i.min(n - 1) as f32 / (n - 1) as f32,
        };
        let along = t * last as f32;
        let k = (along.floor() as usize).min(last.saturating_sub(1));
        let (a, b) = (self.path[k], self.path[(k + 1).min(last)]);
        let f = along - k as f32;
        let lerp = |x: [f32; 3], y: [f32; 3]| std::array::from_fn(|j| x[j] + (y[j] - x[j]) * f);
        CameraKey {
            eye: lerp(a.eye, b.eye),
            target: lerp(a.target, b.target),
        }
    }
}
