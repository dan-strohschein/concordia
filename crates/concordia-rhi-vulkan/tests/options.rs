//! `CONCORDIA_VULKAN_VALIDATION` (pure; M4 4.1v(b)): off by default, `auto` when installed, `on`
//! required, `strict` required with any error failing the device's next call.

use concordia_rhi::RhiError;
use concordia_rhi_vulkan::{Options, VALIDATION_ENV, Validation};

#[test]
fn validation_env_values_map_to_options() {
    assert_eq!(VALIDATION_ENV, "CONCORDIA_VULKAN_VALIDATION");
    let options = |v| Options::from_env_value(v).unwrap();
    for unset in [None, Some(""), Some("off")] {
        let o = options(unset);
        assert_eq!(
            (o.validation, o.strict),
            (Validation::Off, false),
            "{unset:?}"
        );
    }
    let o = options(Some("auto"));
    assert_eq!((o.validation, o.strict), (Validation::IfAvailable, false));
    let o = options(Some("on"));
    assert_eq!((o.validation, o.strict), (Validation::Required, false));
    let o = options(Some("strict"));
    assert_eq!((o.validation, o.strict), (Validation::Required, true));
}

#[test]
fn unknown_validation_values_name_the_choices() {
    let err = Options::from_env_value(Some("loud")).unwrap_err();
    assert!(matches!(err, RhiError::Invalid { .. }), "{err}");
    let text = err.to_string();
    for want in [VALIDATION_ENV, "loud", "off", "auto", "on", "strict"] {
        assert!(text.contains(want), "{want}: {text}");
    }
}
