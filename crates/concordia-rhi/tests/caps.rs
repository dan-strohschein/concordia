//! Device capabilities and the M1 requirement check (M1 phase 1.2a).

use concordia_rhi::{Backend, DeviceCaps, Requirement, SampleCount};

fn metal_caps() -> DeviceCaps {
    DeviceCaps {
        backend: Backend::Metal,
        device_name: "Test GPU".into(),
        unified_memory: true,
        msaa_sample_counts: vec![SampleCount::X1, SampleCount::X4],
        bindless: true,
        mesh_shaders: true,
        ray_tracing: true,
        max_buffer_bytes: 1 << 30,
        max_bindless_textures: 16_384,
        max_bindless_buffers: 16_384,
        feature_sets: vec!["apple9".into(), "metal3".into()],
    }
}

#[test]
fn complete_device_meets_m1_requirements() {
    assert_eq!(metal_caps().missing(Requirement::M1), Vec::<String>::new());
}

#[test]
fn missing_msaa4_and_bindless_are_both_reported() {
    let mut caps = metal_caps();
    caps.msaa_sample_counts = vec![SampleCount::X1];
    caps.bindless = false;
    let missing = caps.missing(Requirement::M1);
    assert_eq!(missing.len(), 2, "{missing:?}");
    assert!(missing.iter().any(|m| m.contains("4x MSAA")), "{missing:?}");
    assert!(
        missing.iter().any(|m| m.contains("bindless")),
        "{missing:?}"
    );
}

#[test]
fn caps_serialize_with_lowercase_backend_and_numeric_sample_counts() {
    let json = serde_json::to_value(metal_caps()).unwrap();
    assert_eq!(json["backend"], "metal");
    assert_eq!(json["msaa_sample_counts"], serde_json::json!([1, 4]));
    let back: DeviceCaps = serde_json::from_value(json).unwrap();
    assert_eq!(back, metal_caps());
}

#[test]
fn sample_count_rejects_non_power_of_two_counts() {
    assert_eq!(SampleCount::new(4), Some(SampleCount::X4));
    assert_eq!(SampleCount::new(3), None);
    assert_eq!(SampleCount::new(0), None);
    assert!(serde_json::from_value::<SampleCount>(serde_json::json!(3)).is_err());
}
