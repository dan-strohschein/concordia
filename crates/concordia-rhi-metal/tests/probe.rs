//! The Metal device probe (M1 phase 1.2a).

use concordia_rhi::{Backend, RhiError};
use concordia_testkit::gpu_required;

#[test]
fn probe_reports_metal_caps_or_explains_absence() {
    match concordia_rhi_metal::probe() {
        Ok(caps) => {
            assert_eq!(caps.backend, Backend::Metal);
            assert!(!caps.device_name.is_empty());
            assert!(caps.max_buffer_bytes > 0);
            assert!(
                caps.msaa_sample_counts
                    .contains(&concordia_rhi::SampleCount::X1)
            );
            assert!(caps.feature_sets.windows(2).all(|w| w[0] < w[1]), "sorted");
        }
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            assert!(!reason.is_empty());
        }
        Err(other) => panic!("unexpected probe error: {other}"),
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn probe_off_macos_reports_no_device() {
    assert!(matches!(
        concordia_rhi_metal::probe(),
        Err(RhiError::NoDevice { .. })
    ));
}
