//! Caps from Vulkan probe records (pure; runs on every OS, M4 4.1v(a)).

use concordia_rhi::{Backend, Requirement, SampleCount, TimestampGranularity};
use concordia_rhi_vulkan::BINDLESS_CAPACITY;
use concordia_rhi_vulkan::record::{DeviceType, ProbeRecord, caps_from_record, choose};

fn lavapipe() -> ProbeRecord {
    serde_json::from_str(include_str!("fixtures/probes/lavapipe.probe.json"))
        .expect("fixture parses")
}

#[test]
fn lavapipe_meets_m1_with_draw_boundary_timestamps() {
    let caps = caps_from_record(&lavapipe());
    assert_eq!(caps.backend, Backend::Vulkan);
    assert!(
        caps.bindless && caps.msaa_sample_counts.contains(&SampleCount::X4),
        "{caps:?}"
    );
    assert_eq!(caps.missing(Requirement::M1), Vec::<String>::new());
    assert_eq!(
        caps.timestamp_granularity,
        TimestampGranularity::DrawBoundary
    );
    assert_eq!(
        (caps.max_bindless_textures, caps.max_bindless_buffers),
        (BINDLESS_CAPACITY, BINDLESS_CAPACITY)
    );
    assert_eq!(caps.feature_sets, ["vulkan1.3"]);
    assert!(
        !caps.mesh_shaders && !caps.ray_tracing && !caps.metalfx_temporal,
        "not implemented in 4.1v(a)"
    );
}

#[test]
fn missing_features_are_named_and_withhold_bindless() {
    let mut record = lavapipe();
    record.features.timeline_semaphore = false;
    record.features.descriptor_binding_partially_bound = false;
    assert_eq!(
        record.missing(),
        ["timelineSemaphore", "descriptorBindingPartiallyBound"]
    );
    let caps = caps_from_record(&record);
    assert!(!caps.bindless);
    assert!(caps.missing(Requirement::M1)[0].contains("descriptor indexing"));
}

#[test]
fn optional_anisotropy_is_not_required() {
    let mut record = lavapipe();
    record.features.sampler_anisotropy = false;
    assert!(record.missing().is_empty());
}

#[test]
fn old_apis_and_queueless_devices_are_refused() {
    let mut record = lavapipe();
    record.api_version = [1, 2, 198];
    record.graphics_compute_queue = false;
    assert_eq!(
        record.missing(),
        [
            "Vulkan 1.3 (the device reports 1.2)",
            "a graphics and compute queue"
        ]
    );
}

#[test]
fn timestamps_follow_the_queues_valid_bits() {
    let mut record = lavapipe();
    record.queue_timestamp_valid_bits = 0;
    assert_eq!(
        caps_from_record(&record).timestamp_granularity,
        TimestampGranularity::None
    );
}

#[test]
fn few_bindless_slots_withhold_bindless() {
    let mut record = lavapipe();
    record.max_per_stage_update_after_bind_storage_images = 4096;
    let caps = caps_from_record(&record);
    assert_eq!(caps.max_bindless_textures, 4096);
    assert!(!caps.bindless);
}

#[test]
fn extensions_join_the_feature_sets_sorted() {
    let mut record = lavapipe();
    record.extensions = vec!["VK_KHR_ray_query".into(), "VK_EXT_mesh_shader".into()];
    assert_eq!(
        caps_from_record(&record).feature_sets,
        ["VK_EXT_mesh_shader", "VK_KHR_ray_query", "vulkan1.3"]
    );
}

#[test]
fn choose_prefers_usable_discrete_gpus() {
    let cpu = lavapipe();
    let mut gpu = lavapipe();
    gpu.device_type = DeviceType::Discrete;
    gpu.device_name = "discrete".into();
    let mut broken = gpu.clone();
    broken.features.dynamic_rendering = false;
    assert_eq!(choose(&[cpu.clone(), gpu.clone()]), Ok(1));
    assert_eq!(
        choose(&[broken.clone(), cpu.clone()]),
        Ok(1),
        "unusable devices are skipped"
    );
    assert_eq!(
        choose(&[cpu.clone(), cpu.clone()]),
        Ok(0),
        "ties keep enumeration order"
    );
    assert_eq!(choose(&[]), Err("no Vulkan physical devices".into()));
    let reasons = choose(&[broken]).unwrap_err();
    assert!(
        reasons.contains("discrete lacks dynamicRendering"),
        "{reasons}"
    );
}

#[test]
fn records_round_trip_through_json() {
    let record = lavapipe();
    let json = serde_json::to_string(&record).unwrap();
    assert_eq!(serde_json::from_str::<ProbeRecord>(&json).unwrap(), record);
}

/// Prints this machine's record, to re-record fixtures (`--run-ignored only --no-capture`).
#[test]
#[ignore = "prints the local device's probe record for fixtures"]
fn print_this_machines_probe_record() {
    match concordia_rhi_vulkan::probe_record() {
        Ok(record) => println!("{}", serde_json::to_string_pretty(&record).unwrap()),
        Err(e) => println!("no Vulkan device: {e}"),
    }
}
