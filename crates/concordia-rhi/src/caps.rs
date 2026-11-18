//! What a device can do, and whether that is enough for a milestone.

use serde::{Deserialize, Serialize};

/// Graphics API behind a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// Apple Metal.
    Metal,
    /// Khronos Vulkan.
    Vulkan,
}

/// A valid MSAA sample count (1, 2, 4 or 8); serializes as the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct SampleCount(u8);

impl SampleCount {
    /// One sample (no MSAA).
    pub const X1: Self = Self(1);
    /// Two samples.
    pub const X2: Self = Self(2);
    /// Four samples (M1's default).
    pub const X4: Self = Self(4);
    /// Eight samples.
    pub const X8: Self = Self(8);
    /// Every valid count, ascending.
    pub const ALL: [Self; 4] = [Self::X1, Self::X2, Self::X4, Self::X8];

    /// The count, if it is 1, 2, 4 or 8.
    pub fn new(count: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.0 == count)
    }

    /// The number of samples.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for SampleCount {
    type Error = String;

    fn try_from(count: u8) -> Result<Self, Self::Error> {
        Self::new(count).ok_or_else(|| format!("sample count must be 1, 2, 4 or 8, got {count}"))
    }
}

impl From<SampleCount> for u8 {
    fn from(count: SampleCount) -> u8 {
        count.0
    }
}

/// Device capabilities, as reported by a backend's probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceCaps {
    /// Graphics API.
    pub backend: Backend,
    /// Device name as the driver reports it.
    pub device_name: String,
    /// CPU and GPU share memory.
    pub unified_memory: bool,
    /// Supported MSAA counts for color textures, ascending.
    pub msaa_sample_counts: Vec<SampleCount>,
    /// Bindless resource tables (Metal: argument buffers tier 2).
    pub bindless: bool,
    /// Mesh and object/task shaders.
    pub mesh_shaders: bool,
    /// Hardware ray tracing.
    pub ray_tracing: bool,
    /// Largest single buffer in bytes.
    pub max_buffer_bytes: u64,
    /// Backend feature-set names (e.g. Metal GPU families `apple9`, `metal3`), sorted.
    pub feature_sets: Vec<String>,
}

/// A capability bar a milestone needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// M1 rendering: 4x MSAA and bindless resources.
    M1,
}

impl DeviceCaps {
    /// Human-readable reasons the device falls short of `requirement` (empty if it meets it).
    pub fn missing(&self, requirement: Requirement) -> Vec<String> {
        match requirement {
            Requirement::M1 => self.missing_for_m1(),
        }
    }

    fn missing_for_m1(&self) -> Vec<String> {
        let mut missing = Vec::new();
        if !self.msaa_sample_counts.contains(&SampleCount::X4) {
            missing.push("4x MSAA color textures".to_owned());
        }
        if !self.bindless {
            missing.push("bindless resources (Metal argument buffers tier 2)".to_owned());
        }
        missing
    }
}
