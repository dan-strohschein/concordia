//! Reads a `ProbeRecord` (and so `RhiCaps`) from the system default `MTLDevice`.

use concordia_rhi::{RhiCaps, RhiError, SampleCount};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLArgumentBuffersTier, MTLCommonCounterSetTimestamp, MTLCounterSamplingPoint, MTLCounterSet,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLGPUFamily,
};
use objc2_metal_fx::MTLFXTemporalScalerDescriptor;

use crate::record::{ProbeRecord, SamplingPoint, caps_from_record};

pub(crate) type Device = ProtocolObject<dyn MTLDevice>;

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

/// Counter sampling points, by the name the record uses.
const SAMPLING_POINTS: [(SamplingPoint, MTLCounterSamplingPoint); 5] = [
    (
        SamplingPoint::Stage,
        MTLCounterSamplingPoint::AtStageBoundary,
    ),
    (SamplingPoint::Draw, MTLCounterSamplingPoint::AtDrawBoundary),
    (
        SamplingPoint::Dispatch,
        MTLCounterSamplingPoint::AtDispatchBoundary,
    ),
    (
        SamplingPoint::TileDispatch,
        MTLCounterSamplingPoint::AtTileDispatchBoundary,
    ),
    (SamplingPoint::Blit, MTLCounterSamplingPoint::AtBlitBoundary),
];

pub(crate) fn system_default() -> Result<RhiCaps, RhiError> {
    Ok(caps(&default_device()?))
}

pub(crate) fn system_default_record() -> Result<ProbeRecord, RhiError> {
    Ok(record(&default_device()?))
}

/// The system default device.
pub(crate) fn default_device() -> Result<Retained<Device>, RhiError> {
    MTLCreateSystemDefaultDevice().ok_or_else(|| RhiError::NoDevice {
        reason: "MTLCreateSystemDefaultDevice returned nil (no Metal device in this session)"
            .into(),
    })
}

pub(crate) fn caps(device: &Retained<Device>) -> RhiCaps {
    caps_from_record(&record(device))
}

/// What `device` reports.
pub(crate) fn record(device: &Retained<Device>) -> ProbeRecord {
    ProbeRecord {
        device_name: device.name().to_string(),
        unified_memory: device.hasUnifiedMemory(),
        families: feature_sets(device),
        msaa_sample_counts: sample_counts(device),
        argument_buffers_tier: match device.argumentBuffersSupport() {
            MTLArgumentBuffersTier::Tier2 => 2,
            _ => 1,
        },
        supports_raytracing: device.supportsRaytracing(),
        timestamp_counter_set: timestamp_counter_set(device).is_some(),
        counter_sampling: SAMPLING_POINTS
            .iter()
            .filter(|(_, point)| device.supportsCounterSampling(*point))
            .map(|(name, _)| *name)
            .collect(),
        // SAFETY: `device` is a live MTLDevice; the class method only reads it.
        metalfx_temporal: unsafe { MTLFXTemporalScalerDescriptor::supportsDevice(device) },
        max_buffer_bytes: device.maxBufferLength() as u64,
    }
}

/// The device's common timestamp counter set, if it has one.
pub(crate) fn timestamp_counter_set(
    device: &Device,
) -> Option<Retained<ProtocolObject<dyn MTLCounterSet>>> {
    // SAFETY: an immutable extern static NSString provided by Metal.
    let wanted = unsafe { MTLCommonCounterSetTimestamp };
    device
        .counterSets()?
        .iter()
        .find(|set| set.name().isEqualToString(wanted))
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
