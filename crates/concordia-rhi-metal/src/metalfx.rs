//! MetalFX's temporal scaler (M4 4.6b, ADR-0103): creating one from a `TemporalScalerDesc` and
//! encoding a `TemporalScale`. Every MetalFX `unsafe` in the engine is here.

use concordia_rhi::{RhiError, TemporalScale, TemporalScalerDesc};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandBuffer, MTLTexture};
use objc2_metal_fx::{MTLFXTemporalScaler, MTLFXTemporalScalerBase, MTLFXTemporalScalerDescriptor};

use crate::bindless::MtlTexture;
use crate::convert;
use crate::probe::Device as MtlDevice;

pub(crate) type MtlScaler = ProtocolObject<dyn MTLFXTemporalScaler>;

/// A live scaler and what it was made for.
#[derive(Debug)]
pub(crate) struct ScalerEntry {
    pub(crate) raw: Retained<MtlScaler>,
    pub(crate) desc: TemporalScalerDesc,
}

/// Creates a scaler for `desc` (validated) on `device`, which supports MetalFX's temporal scaler.
pub(crate) fn create(
    device: &MtlDevice,
    desc: &TemporalScalerDesc,
) -> Result<Retained<MtlScaler>, RhiError> {
    let reactive = desc.reactive.map(convert::pixel_format).transpose()?;
    let (color, depth, motion, output) = (
        convert::pixel_format(desc.color)?,
        convert::pixel_format(desc.depth)?,
        convert::pixel_format(desc.motion)?,
        convert::pixel_format(desc.output_format)?,
    );
    // SAFETY: a fresh descriptor; the formats are Metal formats MetalFX accepts (checked by
    // `TemporalScalerDesc::validate`), the sizes are non-zero with the output at least the
    // input, and the device supports the temporal scaler (the caller checked `supportsDevice`).
    let scaler = unsafe {
        let d = MTLFXTemporalScalerDescriptor::new();
        d.setColorTextureFormat(color);
        d.setDepthTextureFormat(depth);
        d.setMotionTextureFormat(motion);
        d.setOutputTextureFormat(output);
        d.setInputWidth(desc.input[0].get() as usize);
        d.setInputHeight(desc.input[1].get() as usize);
        d.setOutputWidth(desc.output[0].get() as usize);
        d.setOutputHeight(desc.output[1].get() as usize);
        d.setAutoExposureEnabled(false);
        d.setInputContentPropertiesEnabled(false);
        d.setReactiveMaskTextureEnabled(reactive.is_some());
        if let Some(format) = reactive {
            d.setReactiveMaskTextureFormat(format);
        }
        d.newTemporalScalerWithDevice(device)
    };
    scaler.ok_or_else(|| {
        RhiError::Unsupported(format!(
            "MetalFX could not make a temporal scaler for {desc:?}"
        ))
    })
}

/// The textures of one scale, resolved and checked against the scaler.
pub(crate) struct Textures<'a> {
    pub(crate) color: &'a MtlTexture,
    pub(crate) depth: &'a MtlTexture,
    pub(crate) motion: &'a MtlTexture,
    pub(crate) reactive: Option<&'a MtlTexture>,
    pub(crate) output: &'a MtlTexture,
}

/// Each texture has the usage the scaler asks for (MetalFX reports it per texture).
pub(crate) fn check_usage(scaler: &MtlScaler, t: &Textures<'_>) -> Result<(), RhiError> {
    // SAFETY: plain property reads on a live scaler.
    let required = unsafe {
        [
            ("color", Some(t.color), scaler.colorTextureUsage()),
            ("depth", Some(t.depth), scaler.depthTextureUsage()),
            ("motion", Some(t.motion), scaler.motionTextureUsage()),
            ("reactive", t.reactive, scaler.reactiveTextureUsage()),
            ("output", Some(t.output), scaler.outputTextureUsage()),
        ]
    };
    for (name, texture, needs) in required {
        let Some(texture) = texture else { continue };
        let has = texture.usage();
        if !has.contains(needs) {
            return Err(RhiError::invalid(
                "temporal scale",
                format!("{name} has Metal usage {has:?}; the scaler needs {needs:?}"),
            ));
        }
    }
    Ok(())
}

/// Encodes `scale` into `cb` (no encoder open) with checked textures.
pub(crate) fn encode(
    scaler: &MtlScaler,
    cb: &ProtocolObject<dyn MTLCommandBuffer>,
    t: &Textures<'_>,
    scale: &TemporalScale,
) {
    // SAFETY: the scaler and every texture are live and match the scaler's sizes, formats and
    // usages (checked by `check::temporal_scale` and `check_usage`); the textures are
    // hazard-tracked, so MetalFX needs no fence; no other encoder is open on `cb`.
    unsafe {
        scaler.setColorTexture(Some(t.color));
        scaler.setDepthTexture(Some(t.depth));
        scaler.setMotionTexture(Some(t.motion));
        scaler.setReactiveMaskTexture(t.reactive);
        scaler.setOutputTexture(Some(t.output));
        scaler.setJitterOffsetX(scale.jitter[0]);
        scaler.setJitterOffsetY(scale.jitter[1]);
        scaler.setMotionVectorScaleX(scale.motion_scale[0]);
        scaler.setMotionVectorScaleY(scale.motion_scale[1]);
        scaler.setPreExposure(scale.pre_exposure);
        scaler.setReset(scale.reset);
        scaler.setDepthReversed(true);
        scaler.encodeToCommandBuffer(cb);
    }
}
