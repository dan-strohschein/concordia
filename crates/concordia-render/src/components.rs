//! Render components (reflected; they appear in `.scene.ron` and the inspector).

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// How the camera projects (reverse-Z for both).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Projection {
    /// Perspective with a vertical field of view in radians.
    Perspective {
        /// Vertical field of view (radians).
        fov_y: f32,
        /// Near plane distance (> 0).
        near: f32,
        /// Far plane distance (> near).
        far: f32,
    },
    /// Orthographic; `height` world units fill the image vertically.
    Orthographic {
        /// Visible height in world units.
        height: f32,
        /// Near plane distance (≥ 0).
        near: f32,
        /// Far plane distance (> near).
        far: f32,
    },
}

impl Default for Projection {
    fn default() -> Self {
        Self::Perspective {
            fov_y: 60f32.to_radians(),
            near: 0.1,
            far: 1000.0,
        }
    }
}

/// Multisample anti-aliasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Msaa {
    /// One sample per pixel.
    Off,
    /// Four samples per pixel.
    #[default]
    X4,
}

/// Maps HDR scene color to the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Tonemap {
    /// AgX: filmic, hue-stable highlight roll-off (Godot 4.4+ offers the same curve).
    #[default]
    AgX,
    /// Khronos PBR Neutral: keeps base colors faithful, compresses only highlights.
    KhronosPbrNeutral,
    /// ACES fitted curve (Narkowicz): punchy contrast.
    AcesFitted,
    /// Clamp to [0, 1] (exact colors for tests and unlit UI-like scenes).
    Linear,
}

impl Tonemap {
    /// The mode index `tonemap.slang` switches on.
    pub(crate) fn shader_mode(self) -> u32 {
        match self {
            Self::AgX => 0,
            Self::KhronosPbrNeutral => 1,
            Self::AcesFitted => 2,
            Self::Linear => 3,
        }
    }
}

/// A view into the world (the entity's `Transform` places it; it looks down its -Z).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct Camera {
    /// Projection.
    pub projection: Projection,
    /// Linear clear color (RGBA).
    pub clear: [f32; 4],
    /// Anti-aliasing.
    pub msaa: Msaa,
    /// Exposure as EV100 (physical camera; 15 suits direct sunlight).
    #[reflect(range(min = -10.0, max = 30.0))]
    pub exposure_ev100: f32,
    /// Tone mapping.
    pub tonemap: Tonemap,
    /// The highest priority camera is the default view.
    pub priority: i32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            projection: Projection::default(),
            clear: [0.0, 0.0, 0.0, 1.0],
            msaa: Msaa::default(),
            exposure_ev100: 15.0,
            tonemap: Tonemap::default(),
            priority: 0,
        }
    }
}

impl Camera {
    /// Multiplier from scene luminance to display: 1 / (1.2 · 2^EV100) (Lagarde & de Rousiers 2014).
    pub fn exposure(&self) -> f32 {
        1.0 / (1.2 * 2f32.powf(self.exposure_ev100))
    }
}

/// Sun-like light shining along the entity's -Z.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct DirectionalLight {
    /// Linear color.
    pub color: [f32; 3],
    /// Illuminance in lux (direct sunlight ≈ 100 000).
    #[reflect(range(min = 0.0, max = 200000.0))]
    pub illuminance_lux: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            illuminance_lux: 100_000.0,
        }
    }
}

/// Omnidirectional light at the entity's position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct PointLight {
    /// Linear color.
    pub color: [f32; 3],
    /// Luminous power in lumens (a 60 W bulb ≈ 800).
    #[reflect(range(min = 0.0, max = 1000000.0))]
    pub intensity_lumens: f32,
    /// Distance where the light fades to zero (also its clustering bound).
    #[reflect(range(min = 0.01, max = 1000.0))]
    pub range: f32,
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            intensity_lumens: 800.0,
            range: 10.0,
        }
    }
}

/// Cone light along the entity's -Z.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct SpotLight {
    /// Linear color.
    pub color: [f32; 3],
    /// Luminous power in lumens (as if unshielded; the cone does not concentrate it).
    #[reflect(range(min = 0.0, max = 1000000.0))]
    pub intensity_lumens: f32,
    /// Distance where the light fades to zero.
    #[reflect(range(min = 0.01, max = 1000.0))]
    pub range: f32,
    /// Half-angle (radians) of full intensity.
    #[reflect(range(min = 0.0, max = 1.57))]
    pub inner_angle: f32,
    /// Half-angle (radians) where the light reaches zero (≥ inner).
    #[reflect(range(min = 0.0, max = 1.57))]
    pub outer_angle: f32,
}

impl Default for SpotLight {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            intensity_lumens: 800.0,
            range: 10.0,
            inner_angle: 0.3,
            outer_angle: 0.5,
        }
    }
}
