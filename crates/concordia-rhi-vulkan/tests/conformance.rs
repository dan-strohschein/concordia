//! The RHI conformance suite on Vulkan (M4 4.1v(a)), under the Khronos validation layer: any
//! error-severity message fails the case. Skips without a device unless CONCORDIA_REQUIRE_VULKAN=1.

use std::sync::OnceLock;

use concordia_rhi::{Device, RhiError};
use concordia_rhi_vulkan::{Options, Validation, ValidationLog, new_headless_with};
use concordia_shaders::Target;
use concordia_shaders::conformance::SlangShaders;
use concordia_testkit::golden::RhiGoldens;
use concordia_testkit::vulkan_required;

fn log() -> &'static ValidationLog {
    static LOG: OnceLock<ValidationLog> = OnceLock::new();
    LOG.get_or_init(ValidationLog::new)
}

fn device() -> Option<Box<dyn Device>> {
    let required = vulkan_required();
    let options = Options {
        validation: if required {
            Validation::Required
        } else {
            Validation::IfAvailable
        },
        log: Some(log().clone()),
        ..Options::default()
    };
    match new_headless_with(&options) {
        Ok(device) => Some(device),
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !required,
                "CONCORDIA_REQUIRE_VULKAN=1 but no device: {reason}"
            );
            None
        }
        Err(other) => panic!("unexpected device error: {other}"),
    }
}

fn no_validation_errors(_: &dyn Device) {
    let errors = log().take();
    assert!(
        errors.is_empty(),
        "validation errors:\n{}",
        errors.join("\n")
    );
}

concordia_rhi::conformance_tests!(
    device,
    RhiGoldens(concordia_rhi::Backend::Vulkan),
    SlangShaders::new(Target::SpirV),
    finish = no_validation_errors
);
