//! Reads `DeviceCaps` from the system default `MTLDevice`.

use concordia_rhi::{Backend, DeviceCaps, RhiError, SampleCount};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLArgumentBuffersTier, MTLCreateSystemDefaultDevice, MTLDevice, MTLGPUFamily};

type Device = ProtocolObject<dyn MTLDevice>;

/// Named Metal GPU families reported in `feature_sets`.
const FAMILIES: [(&str, MTLGPUFamily); 9] = [
    ("apple6", MTLGPUFamily::Apple6),
    ("apple7", MTLGPUFamily::Apple7),
    ("apple8", MTLGPUFamily::Apple8),
    ("apple9", MTLGPUFamily::Apple9),
    ("apple10", MTLGPUFamily::Apple10),
    ("common3", MTLGPUFamily::Common3),
    ("mac2", MTLGPUFamily::Mac2),
    ("metal3", MTLGPUFamily::Metal3),
    ("metal4", MTLGPUFamily::Metal4),
];

pub(crate) fn system_default() -> Result<DeviceCaps, RhiError> {
    let device = MTLCreateSystemDefaultDevice().ok_or_else(|| RhiError::NoDevice {
        reason: "MTLCreateSystemDefaultDevice returned nil (no Metal device in this session)"
            .into(),
    })?;
    Ok(caps(&device))
}

fn caps(device: &Retained<Device>) -> DeviceCaps {
    let feature_sets = feature_sets(device);
    DeviceCaps {
        backend: Backend::Metal,
        device_name: device.name().to_string(),
        unified_memory: device.hasUnifiedMemory(),
        msaa_sample_counts: sample_counts(device),
        bindless: device.argumentBuffersSupport() == MTLArgumentBuffersTier::Tier2,
        mesh_shaders: feature_sets.iter().any(|f| f == "metal3"),
        ray_tracing: device.supportsRaytracing(),
        max_buffer_bytes: device.maxBufferLength() as u64,
        feature_sets,
    }
}

fn sample_counts(device: &Device) -> Vec<SampleCount> {
    SampleCount::ALL
        .into_iter()
        .filter(|c| device.supportsTextureSampleCount(usize::from(c.get())))
        .collect()
}

fn feature_sets(device: &Device) -> Vec<String> {
    let mut names: Vec<String> = FAMILIES
        .iter()
        .filter(|(_, family)| device.supportsFamily(*family))
        .map(|(name, _)| (*name).to_owned())
        .collect();
    names.sort();
    names
}
