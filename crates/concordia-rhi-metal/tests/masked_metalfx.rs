//! The temporal scaler case on a Metal device with MetalFX masked (M4 4.6b): creating a scaler
//! and encoding a scale are refused, as on a device without MetalFX. Skips without a device
//! unless CONCORDIA_REQUIRE_GPU=1.

use concordia_rhi::conformance::{self, Harness};
use concordia_rhi::{CapsMask, RhiError};
use concordia_shaders::Target;
use concordia_shaders::conformance::SlangShaders;
use concordia_testkit::golden::RhiGoldens;
use concordia_testkit::gpu_required;

#[test]
fn a_masked_device_has_no_temporal_scaler() {
    let mask = CapsMask {
        metalfx_temporal: true,
        ..CapsMask::default()
    };
    let device = match concordia_rhi_metal::new_headless_masked(&mask) {
        Ok(device) => device,
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            return;
        }
        Err(other) => panic!("unexpected device error: {other}"),
    };
    assert!(!device.caps().metalfx_temporal);
    let (golden, shaders) = (
        RhiGoldens(concordia_rhi::Backend::Metal),
        SlangShaders::new(Target::Msl),
    );
    let harness = Harness {
        golden: &golden,
        shaders: &shaders,
    };
    conformance::temporal_scaler_upscales_a_frame(&*device, &harness);
}
