//! What a Vulkan probe saw, and the capabilities that follow from it (pure, so it builds and is
//! tested on every OS, including from fixtures of devices we don't have).

use concordia_rhi::{Backend, RhiCaps, SampleCount, TimestampGranularity};
use serde::{Deserialize, Serialize};

/// Bindless table capacity this backend allocates (textures and buffers each), as on Metal.
pub const BINDLESS_CAPACITY: u32 = 16_384;

/// `VkPhysicalDeviceType`, as the probe reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    /// A discrete GPU.
    Discrete,
    /// A GPU in the same package as the CPU.
    Integrated,
    /// A virtualized GPU.
    Virtual,
    /// Software rendering on the CPU (lavapipe, SwiftShader).
    Cpu,
    /// Anything else.
    Other,
}

impl DeviceType {
    /// Selection preference, best first: discrete, integrated, virtual, CPU, other.
    pub fn rank(self) -> u8 {
        match self {
            Self::Discrete => 0,
            Self::Integrated => 1,
            Self::Virtual => 2,
            Self::Cpu => 3,
            Self::Other => 4,
        }
    }
}

/// The Vulkan 1.3 features the backend needs (ADR-0088), each `true` if present.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Features {
    /// `dynamicRendering`.
    pub dynamic_rendering: bool,
    /// `synchronization2`.
    pub synchronization2: bool,
    /// `timelineSemaphore`.
    pub timeline_semaphore: bool,
    /// `bufferDeviceAddress`.
    pub buffer_device_address: bool,
    /// `descriptorBindingPartiallyBound`.
    pub descriptor_binding_partially_bound: bool,
    /// `descriptorBindingSampledImageUpdateAfterBind`.
    pub descriptor_binding_sampled_image_update_after_bind: bool,
    /// `descriptorBindingStorageImageUpdateAfterBind`.
    pub descriptor_binding_storage_image_update_after_bind: bool,
    /// `descriptorBindingUpdateUnusedWhilePending`.
    pub descriptor_binding_update_unused_while_pending: bool,
    /// `shaderStorageImageReadWithoutFormat`.
    pub shader_storage_image_read_without_format: bool,
    /// `shaderStorageImageWriteWithoutFormat`.
    pub shader_storage_image_write_without_format: bool,
    /// `shaderInt64` (buffer pointers).
    pub shader_int64: bool,
    /// `samplerAnisotropy` (optional: without it `LinearRepeat` is not anisotropic).
    pub sampler_anisotropy: bool,
}

impl Features {
    /// The required features this set lacks, by Vulkan name (`samplerAnisotropy` is optional).
    pub fn missing(&self) -> Vec<&'static str> {
        [
            (self.dynamic_rendering, "dynamicRendering"),
            (self.synchronization2, "synchronization2"),
            (self.timeline_semaphore, "timelineSemaphore"),
            (self.buffer_device_address, "bufferDeviceAddress"),
            (
                self.descriptor_binding_partially_bound,
                "descriptorBindingPartiallyBound",
            ),
            (
                self.descriptor_binding_sampled_image_update_after_bind,
                "descriptorBindingSampledImageUpdateAfterBind",
            ),
            (
                self.descriptor_binding_storage_image_update_after_bind,
                "descriptorBindingStorageImageUpdateAfterBind",
            ),
            (
                self.descriptor_binding_update_unused_while_pending,
                "descriptorBindingUpdateUnusedWhilePending",
            ),
            (
                self.shader_storage_image_read_without_format,
                "shaderStorageImageReadWithoutFormat",
            ),
            (
                self.shader_storage_image_write_without_format,
                "shaderStorageImageWriteWithoutFormat",
            ),
            (self.shader_int64, "shaderInt64"),
        ]
        .into_iter()
        .filter_map(|(present, name)| (!present).then_some(name))
        .collect()
    }
}

/// The raw facts a Vulkan probe reads from a physical device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeRecord {
    /// `deviceName`.
    pub device_name: String,
    /// `deviceType`.
    pub device_type: DeviceType,
    /// `apiVersion` as `major.minor.patch`.
    pub api_version: [u32; 3],
    /// Every memory heap is device-local and some host-visible type is device-local.
    pub unified_memory: bool,
    /// The required (and optional) features.
    pub features: Features,
    /// Color MSAA counts both color and depth framebuffers support, ascending.
    pub msaa_sample_counts: Vec<SampleCount>,
    /// `maxBufferSize` (maintenance4).
    pub max_buffer_bytes: u64,
    /// `maxDescriptorSetUpdateAfterBindSampledImages`.
    pub max_update_after_bind_sampled_images: u32,
    /// `maxDescriptorSetUpdateAfterBindStorageImages`.
    pub max_update_after_bind_storage_images: u32,
    /// `maxPerStageDescriptorUpdateAfterBindSampledImages`.
    pub max_per_stage_update_after_bind_sampled_images: u32,
    /// `maxPerStageDescriptorUpdateAfterBindStorageImages`.
    pub max_per_stage_update_after_bind_storage_images: u32,
    /// Some queue family supports graphics and compute.
    pub graphics_compute_queue: bool,
    /// `timestampValidBits` of the chosen graphics queue (0: no timestamps).
    pub queue_timestamp_valid_bits: u32,
    /// `timestampPeriod` in nanoseconds per tick.
    pub timestamp_period_ns: f32,
    /// Device extensions of interest present (`VK_EXT_mesh_shader`, `VK_KHR_ray_query`,
    /// `VK_KHR_portability_subset`), sorted.
    pub extensions: Vec<String>,
}

impl ProbeRecord {
    /// Why this device can't run the backend (empty if it can).
    pub fn missing(&self) -> Vec<String> {
        let mut missing = Vec::new();
        if (self.api_version[0], self.api_version[1]) < (1, 3) {
            let [major, minor, _] = self.api_version;
            missing.push(format!("Vulkan 1.3 (the device reports {major}.{minor})"));
        }
        if !self.graphics_compute_queue {
            missing.push("a graphics and compute queue".to_owned());
        }
        missing.extend(self.features.missing().into_iter().map(str::to_owned));
        missing
    }

    /// Textures the bindless table can hold on this device.
    fn bindless_textures(&self) -> u32 {
        [
            self.max_update_after_bind_sampled_images,
            self.max_update_after_bind_storage_images,
            self.max_per_stage_update_after_bind_sampled_images,
            self.max_per_stage_update_after_bind_storage_images,
            BINDLESS_CAPACITY,
        ]
        .into_iter()
        .min()
        .unwrap_or(0)
    }
}

/// The capabilities a device with `record` has on this backend.
pub fn caps_from_record(record: &ProbeRecord) -> RhiCaps {
    let usable = record.missing().is_empty();
    let [major, minor, _] = record.api_version;
    let mut feature_sets = vec![format!("vulkan{major}.{minor}")];
    feature_sets.extend(record.extensions.iter().cloned());
    feature_sets.sort();
    RhiCaps {
        backend: Backend::Vulkan,
        device_name: record.device_name.clone(),
        unified_memory: record.unified_memory,
        msaa_sample_counts: record.msaa_sample_counts.clone(),
        bindless: usable && record.bindless_textures() >= BINDLESS_CAPACITY,
        // Not implemented in 4.1v(a): mesh pipelines arrive in 4.3b, ray queries in 4.8.
        mesh_shaders: false,
        object_shaders: false,
        ray_tracing: false,
        hw_ray_tracing: false,
        metalfx_temporal: false,
        timestamp_granularity: match record.queue_timestamp_valid_bits {
            0 => TimestampGranularity::None,
            _ => TimestampGranularity::DrawBoundary,
        },
        max_mesh_output_vertices: 0,
        max_mesh_output_primitives: 0,
        max_buffer_bytes: record.max_buffer_bytes,
        max_bindless_textures: record.bindless_textures(),
        max_bindless_buffers: BINDLESS_CAPACITY,
        feature_sets,
    }
}

/// The best usable device among `records` (by [`DeviceType::rank`], then order), or every
/// device's reasons for being unusable.
pub fn choose(records: &[ProbeRecord]) -> Result<usize, String> {
    let usable = records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.missing().is_empty())
        .min_by_key(|(i, r)| (r.device_type.rank(), *i));
    match usable {
        Some((index, _)) => Ok(index),
        None if records.is_empty() => Err("no Vulkan physical devices".into()),
        None => Err(records
            .iter()
            .map(|r| format!("{} lacks {}", r.device_name, r.missing().join(", ")))
            .collect::<Vec<_>>()
            .join("; ")),
    }
}
