//! Device capabilities and the M1 requirement check (M1 phase 1.2a).

use concordia_rhi::{Backend, CapsMask, Requirement, RhiCaps, SampleCount, TimestampGranularity};

fn metal_caps() -> RhiCaps {
    RhiCaps {
        backend: Backend::Metal,
        device_name: "Test GPU".into(),
        unified_memory: true,
        msaa_sample_counts: vec![SampleCount::X1, SampleCount::X4],
        bindless: true,
        mesh_shaders: true,
        object_shaders: true,
        ray_tracing: true,
        hw_ray_tracing: true,
        metalfx_temporal: true,
        timestamp_granularity: TimestampGranularity::StageBoundary,
        max_mesh_output_vertices: 256,
        max_mesh_output_primitives: 512,
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
    let back: RhiCaps = serde_json::from_value(json).unwrap();
    assert_eq!(back, metal_caps());
}

#[test]
fn sample_count_rejects_non_power_of_two_counts() {
    assert_eq!(SampleCount::new(4), Some(SampleCount::X4));
    assert_eq!(SampleCount::new(3), None);
    assert_eq!(SampleCount::new(0), None);
    assert!(serde_json::from_value::<SampleCount>(serde_json::json!(3)).is_err());
}

#[test]
fn empty_mask_changes_nothing() {
    assert_eq!(metal_caps().masked(&CapsMask::default()), metal_caps());
}

#[test]
fn masking_mesh_shaders_hides_object_shaders_and_output_limits() {
    let mask = CapsMask {
        mesh_shaders: true,
        ..CapsMask::default()
    };
    let caps = metal_caps().masked(&mask);
    assert!(!caps.mesh_shaders && !caps.object_shaders);
    assert_eq!(
        (
            caps.max_mesh_output_vertices,
            caps.max_mesh_output_primitives
        ),
        (0, 0)
    );
    assert!(
        caps.ray_tracing && caps.hw_ray_tracing,
        "only mesh shaders are hidden"
    );
}

#[test]
fn masking_the_ray_tracing_api_hides_hardware_ray_tracing() {
    let mask = CapsMask {
        ray_tracing: true,
        ..CapsMask::default()
    };
    let caps = metal_caps().masked(&mask);
    assert!(!caps.ray_tracing && !caps.hw_ray_tracing);
    let hw_only = metal_caps().masked(&CapsMask {
        hw_ray_tracing: true,
        ..CapsMask::default()
    });
    assert!(hw_only.ray_tracing && !hw_only.hw_ray_tracing);
}

#[test]
fn masking_timestamps_keeps_whole_submission_timing_only() {
    let mask = CapsMask {
        timestamps: true,
        metalfx_temporal: true,
        ..CapsMask::default()
    };
    let caps = metal_caps().masked(&mask);
    assert_eq!(
        caps.timestamp_granularity,
        TimestampGranularity::CommandBuffer
    );
    assert!(!caps.timestamp_granularity.times_passes());
    assert!(!caps.metalfx_temporal);
    let mut none = metal_caps();
    none.timestamp_granularity = TimestampGranularity::None;
    assert_eq!(
        none.masked(&mask).timestamp_granularity,
        TimestampGranularity::None,
        "masking never adds a capability"
    );
}

#[test]
fn caps_mask_rejects_unknown_features() {
    let err = serde_json::from_value::<CapsMask>(serde_json::json!({"mesh_shader": true}));
    assert!(err.is_err());
}
