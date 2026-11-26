//! What a Metal probe saw, and the capabilities that follow from it (pure, so it builds and is
//! tested on every OS, including from recorded fixtures of devices we don't have).

use concordia_rhi::{Backend, RhiCaps, SampleCount, TimestampGranularity};
use serde::{Deserialize, Serialize};

/// Where a Metal device can sample counters (`MTLCounterSamplingPoint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SamplingPoint {
    /// At encoder (stage) boundaries.
    Stage,
    /// Between draws.
    Draw,
    /// Between dispatches.
    Dispatch,
    /// Between tile dispatches.
    TileDispatch,
    /// Between blits.
    Blit,
}

/// The raw facts a Metal probe reads from a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeRecord {
    /// `MTLDevice.name`.
    pub device_name: String,
    /// `hasUnifiedMemory`.
    pub unified_memory: bool,
    /// Supported GPU families (`apple6`…`apple10`, `common3`, `mac2`, `metal3`, `metal4`), sorted.
    pub families: Vec<String>,
    /// Supported color MSAA counts, ascending.
    pub msaa_sample_counts: Vec<SampleCount>,
    /// `argumentBuffersSupport` (1 or 2).
    pub argument_buffers_tier: u8,
    /// `supportsRaytracing`.
    pub supports_raytracing: bool,
    /// The device has the common timestamp counter set.
    pub timestamp_counter_set: bool,
    /// Supported counter sampling points.
    pub counter_sampling: Vec<SamplingPoint>,
    /// `MTLFXTemporalScalerDescriptor.supportsDevice`.
    pub metalfx_temporal: bool,
    /// `maxBufferLength`.
    pub max_buffer_bytes: u64,
}

/// Metal's per-threadgroup mesh output limits (vertices, primitives).
pub const MESH_OUTPUT_LIMITS: (u32, u32) = (256, 512);

/// The capabilities a device with `record` has, by Apple's family table.
pub fn caps_from_record(record: &ProbeRecord) -> RhiCaps {
    let mesh = record.has("metal3");
    let (vertices, primitives) = if mesh { MESH_OUTPUT_LIMITS } else { (0, 0) };
    RhiCaps {
        backend: Backend::Metal,
        device_name: record.device_name.clone(),
        unified_memory: record.unified_memory,
        msaa_sample_counts: record.msaa_sample_counts.clone(),
        bindless: record.argument_buffers_tier >= 2,
        mesh_shaders: mesh,
        object_shaders: mesh,
        ray_tracing: record.supports_raytracing,
        hw_ray_tracing: record.supports_raytracing && record.has("apple9"),
        metalfx_temporal: record.metalfx_temporal,
        // Apple's feature-set table: min and max depth resolve filters on Apple3+ and Mac2.
        depth_resolve_min: record
            .families
            .iter()
            .any(|f| f == "mac2" || f.starts_with("apple")),
        // Device-memory atomics in every function type on Metal 2+; render targets up to 16384
        // pixels on Apple3+ and Mac2 (Apple's feature-set table).
        fragment_atomics: true,
        max_render_extent: match record
            .families
            .iter()
            .any(|f| f == "mac2" || f.starts_with("apple"))
        {
            true => 16_384,
            false => 8_192,
        },
        timestamp_granularity: timestamp_granularity(record),
        max_mesh_output_vertices: vertices,
        max_mesh_output_primitives: primitives,
        max_buffer_bytes: record.max_buffer_bytes,
        max_bindless_textures: crate::BINDLESS_CAPACITY,
        max_bindless_buffers: crate::BINDLESS_CAPACITY,
        feature_sets: record.families.clone(),
    }
}

impl ProbeRecord {
    /// True if the device supports GPU family `name`.
    pub fn has(&self, name: &str) -> bool {
        self.families.iter().any(|f| f == name)
    }
}

/// The finest timestamp sampling the record allows (command buffers can always be timed).
fn timestamp_granularity(record: &ProbeRecord) -> TimestampGranularity {
    let can = |point| record.timestamp_counter_set && record.counter_sampling.contains(&point);
    if can(SamplingPoint::Draw) {
        TimestampGranularity::DrawBoundary
    } else if can(SamplingPoint::Stage) {
        TimestampGranularity::StageBoundary
    } else {
        TimestampGranularity::CommandBuffer
    }
}
