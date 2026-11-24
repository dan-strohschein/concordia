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
pub struct RhiCaps {
    /// Graphics API.
    pub backend: Backend,
    /// Device name as the driver reports it.
    pub device_name: String,
    /// CPU and GPU share memory.
    pub unified_memory: bool,
    /// Supported MSAA counts for color textures, ascending.
    pub msaa_sample_counts: Vec<SampleCount>,
    /// Bindless resource tables (Metal: argument buffers tier 2; Vulkan: descriptor indexing and
    /// buffer device addresses).
    pub bindless: bool,
    /// Mesh shaders.
    pub mesh_shaders: bool,
    /// Object (task/amplification) shaders.
    pub object_shaders: bool,
    /// The ray-tracing API (intersection queries; may run in software).
    pub ray_tracing: bool,
    /// Hardware-accelerated ray tracing.
    pub hw_ray_tracing: bool,
    /// A temporal upscaler (Metal: MetalFX's temporal scaler).
    pub metalfx_temporal: bool,
    /// The finest point at which GPU timestamps can be sampled.
    pub timestamp_granularity: TimestampGranularity,
    /// Most vertices one mesh-shader threadgroup may output (0 without mesh shaders).
    pub max_mesh_output_vertices: u32,
    /// Most primitives one mesh-shader threadgroup may output (0 without mesh shaders).
    pub max_mesh_output_primitives: u32,
    /// Largest single buffer in bytes.
    pub max_buffer_bytes: u64,
    /// Bindless table capacity for textures.
    pub max_bindless_textures: u32,
    /// Bindless table capacity for buffers.
    pub max_bindless_buffers: u32,
    /// Backend feature-set names (e.g. Metal GPU families `apple9`, `metal3`), sorted.
    pub feature_sets: Vec<String>,
}

/// Where a device can sample GPU timestamps, coarsest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimestampGranularity {
    /// No GPU timestamps.
    None,
    /// Only a whole submission's start and end.
    CommandBuffer,
    /// At encoder (stage) boundaries: enough to time each render-graph pass.
    StageBoundary,
    /// Between individual draws and dispatches.
    DrawBoundary,
}

impl TimestampGranularity {
    /// True if each render-graph pass can be timed on its own.
    pub fn times_passes(self) -> bool {
        self >= Self::StageBoundary
    }
}

/// Features to hide from a device, so one machine can test every fallback path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CapsMask {
    /// Hide mesh and object shaders.
    pub mesh_shaders: bool,
    /// Hide the ray-tracing API (and so hardware ray tracing).
    pub ray_tracing: bool,
    /// Hide hardware ray tracing only.
    pub hw_ray_tracing: bool,
    /// Hide the temporal upscaler.
    pub metalfx_temporal: bool,
    /// Hide per-pass timestamps (whole-submission timing stays).
    pub timestamps: bool,
}

/// A capability bar a milestone needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// M1 rendering: 4x MSAA and bindless resources.
    M1,
}

impl RhiCaps {
    /// These caps with `mask`'s features hidden.
    pub fn masked(&self, mask: &CapsMask) -> RhiCaps {
        let mut caps = self.clone();
        if mask.mesh_shaders {
            caps.mesh_shaders = false;
            caps.object_shaders = false;
            caps.max_mesh_output_vertices = 0;
            caps.max_mesh_output_primitives = 0;
        }
        if mask.ray_tracing {
            caps.ray_tracing = false;
        }
        if mask.ray_tracing || mask.hw_ray_tracing {
            caps.hw_ray_tracing = false;
        }
        if mask.metalfx_temporal {
            caps.metalfx_temporal = false;
        }
        if mask.timestamps {
            caps.timestamp_granularity = caps
                .timestamp_granularity
                .min(TimestampGranularity::CommandBuffer);
        }
        caps
    }

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
            missing.push(match self.backend {
                Backend::Metal => "bindless resources (Metal argument buffers tier 2)".to_owned(),
                Backend::Vulkan => "bindless resources (Vulkan descriptor indexing with 16384 \
                                    update-after-bind images, and buffer device addresses)"
                    .to_owned(),
            });
        }
        missing
    }
}
