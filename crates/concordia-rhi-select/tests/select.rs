//! Backend choice and opening (M4 4.1v(b), ADR-0088).

use concordia_rhi::{Backend, RhiError};
use concordia_rhi_select::{BACKEND_ENV, BackendChoice, SelectError, choose};

#[test]
fn choices_parse_by_lowercase_name_and_list_the_options() {
    for (name, choice) in [
        ("auto", BackendChoice::Auto),
        ("metal", BackendChoice::Metal),
        ("vulkan", BackendChoice::Vulkan),
    ] {
        assert_eq!(name.parse::<BackendChoice>().unwrap(), choice);
        assert_eq!(
            serde_json::to_string(&choice).unwrap(),
            format!("\"{name}\"")
        );
        assert_eq!(choice.to_string(), name);
    }
    assert_eq!(BackendChoice::default(), BackendChoice::Auto);
    let err = "Metal".parse::<BackendChoice>().unwrap_err().to_string();
    for want in ["Metal", "auto", "metal", "vulkan"] {
        assert!(err.contains(want), "{want}: {err}");
    }
}

#[test]
fn auto_is_metal_on_macos_and_vulkan_elsewhere() {
    assert_eq!(BackendChoice::Auto.resolve_for(true), Backend::Metal);
    assert_eq!(BackendChoice::Auto.resolve_for(false), Backend::Vulkan);
    for macos in [true, false] {
        assert_eq!(BackendChoice::Metal.resolve_for(macos), Backend::Metal);
        assert_eq!(BackendChoice::Vulkan.resolve_for(macos), Backend::Vulkan);
    }
    assert_eq!(
        BackendChoice::Auto.resolve(),
        BackendChoice::Auto.resolve_for(cfg!(target_os = "macos"))
    );
}

#[test]
fn the_env_beats_the_project_setting_which_beats_auto() {
    assert_eq!(BACKEND_ENV, "CONCORDIA_BACKEND");
    let auto = BackendChoice::Auto.resolve();
    assert_eq!(
        choose(Some("vulkan"), BackendChoice::Metal).unwrap(),
        Backend::Vulkan
    );
    assert_eq!(
        choose(Some("metal"), BackendChoice::Vulkan).unwrap(),
        Backend::Metal
    );
    assert_eq!(choose(Some("auto"), BackendChoice::Vulkan).unwrap(), auto);
    assert_eq!(
        choose(None, BackendChoice::Vulkan).unwrap(),
        Backend::Vulkan
    );
    assert_eq!(
        choose(Some(""), BackendChoice::Metal).unwrap(),
        Backend::Metal,
        "empty is unset"
    );
    assert_eq!(choose(None, BackendChoice::Auto).unwrap(), auto);
}

#[test]
fn an_unknown_env_backend_names_the_variable() {
    let err = choose(Some("dx12"), BackendChoice::Auto).unwrap_err();
    assert!(matches!(err, SelectError::Env { .. }), "{err}");
    let text = err.to_string();
    assert!(
        text.contains("CONCORDIA_BACKEND") && text.contains("dx12"),
        "{text}"
    );
}

/// Each backend opens its own device, or is `NoDevice` where it can't (Vulkan on a Mac without
/// a loader); required devices must open.
#[test]
fn each_backend_opens_its_own_device_or_is_no_device() {
    for backend in Backend::ALL {
        match concordia_rhi_select::new_headless(backend) {
            Ok(device) => assert_eq!(device.caps().backend, backend),
            Err(RhiError::NoDevice { reason }) => {
                assert!(
                    !concordia_testkit::device_required(backend),
                    "{backend}: {reason}"
                );
                assert!(!reason.is_empty());
            }
            Err(other) => panic!("{backend}: {other}"),
        }
    }
}

#[test]
fn probes_and_records_follow_the_backend() {
    for backend in Backend::ALL {
        match concordia_rhi_select::probe(backend) {
            Ok(caps) => {
                assert_eq!(caps.backend, backend);
                let record = concordia_rhi_select::probe_record(backend).unwrap();
                assert!(
                    record["device_name"]
                        .as_str()
                        .is_some_and(|n| !n.is_empty()),
                    "{record}"
                );
            }
            Err(RhiError::NoDevice { .. }) => {
                assert!(!concordia_testkit::device_required(backend), "{backend}");
                assert!(concordia_rhi_select::probe_record(backend).is_err());
            }
            Err(other) => panic!("{backend}: {other}"),
        }
    }
}
