//! The RHI conformance suite on Metal (M1 phase 1.2b). Skips without a device unless CONCORDIA_REQUIRE_GPU=1.

use concordia_rhi::{Device, RhiError};
use concordia_shaders::Target;
use concordia_shaders::conformance::SlangShaders;
use concordia_testkit::golden::RhiGoldens;
use concordia_testkit::gpu_required;

fn device() -> Option<Box<dyn Device>> {
    match concordia_rhi_metal::new_headless() {
        Ok(device) => Some(device),
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            None
        }
        Err(other) => panic!("unexpected device error: {other}"),
    }
}

concordia_rhi::conformance_tests!(device, RhiGoldens, SlangShaders::new(Target::Msl));

#[cfg(target_os = "macos")]
#[test]
fn device_lost_is_reported() {
    let Ok(device) = concordia_rhi_metal::MetalDevice::system_default() else {
        assert!(!gpu_required());
        return;
    };
    device.simulate_device_lost();
    let list = concordia_rhi::CommandList::new("after loss");
    let submit = concordia_rhi::Submit {
        queue: concordia_rhi::QueueKind::Graphics,
        commands: &list,
        wait: &[],
        signal: &[],
    };
    assert!(matches!(
        device.submit(submit),
        Err(RhiError::DeviceLost(_))
    ));
}
