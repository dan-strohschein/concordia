//! Shadow settings of a directional light (a component field; the renderer fits the cascades).

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Percentage-closer filter size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum PcfKernel {
    /// 3×3 texels (tent-filtered).
    #[default]
    X3,
    /// 5×5 texels (softer).
    X5,
}

impl PcfKernel {
    /// Kernel radius in texels.
    pub fn radius(self) -> u32 {
        match self {
            Self::X3 => 1,
            Self::X5 => 2,
        }
    }
}

/// Shadow settings of a directional light.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct CascadeSettings {
    /// Cascades (1..=4).
    #[reflect(range(min = 1.0, max = 4.0))]
    pub cascades: u8,
    /// Shadows end this far from the camera (view distance).
    #[reflect(range(min = 0.1, max = 10000.0))]
    pub max_distance: f32,
    /// Blend between logarithmic (1) and uniform (0) splits.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub split_lambda: f32,
    /// Shadow map size per cascade (texels per side).
    #[reflect(range(min = 64.0, max = 8192.0))]
    pub resolution: u32,
    /// Constant depth bias in world units.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub depth_bias: f32,
    /// Receiver offset along the normal, in shadow texels.
    #[reflect(range(min = 0.0, max = 10.0))]
    pub normal_bias: f32,
    /// Filter.
    pub pcf: PcfKernel,
    /// Snap cascades to the texel grid so shadows do not shimmer while the camera moves.
    pub stabilize: bool,
}

impl Default for CascadeSettings {
    fn default() -> Self {
        Self {
            cascades: 3,
            max_distance: 60.0,
            split_lambda: 0.75,
            resolution: 2048,
            depth_bias: 0.02,
            normal_bias: 1.5,
            pcf: PcfKernel::X3,
            stabilize: true,
        }
    }
}

impl CascadeSettings {
    /// Why these settings are invalid, if they are.
    pub fn validate(&self) -> Result<(), String> {
        match () {
            _ if !(1..=4).contains(&self.cascades) => Err("cascades must be 1..=4".into()),
            _ if !(64..=8192).contains(&self.resolution) => {
                Err("resolution must be 64..=8192".into())
            }
            _ if !(0.0..=1.0).contains(&self.split_lambda) => {
                Err("split_lambda must be in [0, 1]".into())
            }
            _ if !(self.max_distance > 0.0 && self.max_distance.is_finite()) => {
                Err("max_distance must be positive".into())
            }
            _ if !(self.depth_bias >= 0.0 && self.normal_bias >= 0.0) => {
                Err("biases must be non-negative".into())
            }
            _ => Ok(()),
        }
    }
}
