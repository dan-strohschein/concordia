//! Caps from Vulkan probe records (pure; runs on every OS, M4 4.1v(a)).

use concordia_rhi::{Backend, Requirement, SampleCount, TimestampGranularity};
use concordia_rhi_vulkan::BINDLESS_CAPACITY;
use concordia_rhi_vulkan::record::{DeviceType, MeshRecord, ProbeRecord, caps_from_record, choose};

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
    assert_eq!(
        caps.feature_sets,
        [
            "VK_EXT_mesh_shader",
            "VK_KHR_ray_query",
            "VK_KHR_swapchain",
            "VK_KHR_swapchain_mutable_format",
            "vulkan1.4"
        ]
    );
    assert!(
        !caps.ray_tracing && !caps.metalfx_temporal,
        "not implemented yet (ray queries arrive in 4.8)"
    );
}

#[test]
fn vulkan_caps_from_record_read_mesh_shader_properties() {
    let caps = caps_from_record(&lavapipe());
    assert!(caps.mesh_shaders && caps.object_shaders, "{caps:?}");
    assert_eq!(
        (
            caps.max_mesh_output_vertices,
            caps.max_mesh_output_primitives
        ),
        (256, 256)
    );
    let mut mesh_only = lavapipe();
    mesh_only.mesh.as_mut().unwrap().task_shader = false;
    let caps = caps_from_record(&mesh_only);
    assert!(caps.mesh_shaders && !caps.object_shaders);
}

#[test]
fn records_without_mesh_properties_have_no_mesh_shaders() {
    let mut record = lavapipe();
    record.mesh = None;
    let caps = caps_from_record(&record);
    assert!(!caps.mesh_shaders && !caps.object_shaders);
    assert_eq!(
        (
            caps.max_mesh_output_vertices,
            caps.max_mesh_output_primitives
        ),
        (0, 0)
    );
    // A 4.3a record has no `mesh` field.
    let mut json: serde_json::Value = serde_json::to_value(lavapipe()).unwrap();
    json.as_object_mut().unwrap().remove("mesh");
    let old: ProbeRecord = serde_json::from_value(json).unwrap();
    assert_eq!(old.mesh, None);
    let mut no_extension = lavapipe();
    no_extension
        .extensions
        .retain(|e| e != "VK_EXT_mesh_shader");
    assert!(!caps_from_record(&no_extension).mesh_shaders);
    let mut no_feature = lavapipe();
    no_feature.mesh.as_mut().unwrap().mesh_shader = false;
    assert!(!caps_from_record(&no_feature).mesh_shaders);
}

#[test]
fn mesh_limits_below_the_portable_minimum_withhold_mesh_shaders() {
    let shrink: [fn(&mut MeshRecord); 6] = [
        |m| m.max_payload_bytes = 16_383,
        |m| m.max_mesh_invocations = 127,
        |m| m.max_task_invocations = 127,
        |m| m.max_task_groups[1] = 65_534,
        |m| m.max_mesh_groups_total = (1 << 22) - 1,
        |m| m.max_output_primitives = 0,
    ];
    for (i, change) in shrink.into_iter().enumerate() {
        let mut record = lavapipe();
        change(record.mesh.as_mut().unwrap());
        assert!(!caps_from_record(&record).mesh_shaders, "change {i}");
    }
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
        ["VK_EXT_mesh_shader", "VK_KHR_ray_query", "vulkan1.4"]
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

#[test]
fn slang_vertex_shaders_need_draw_parameters() {
    let mut record = lavapipe();
    record.features.shader_draw_parameters = false;
    assert_eq!(record.missing(), ["shaderDrawParameters"]);
}

/// Records list the instance's surface extensions (4.1v(b)); records made before they did still
/// read, with none.
#[test]
fn records_list_surface_extensions_and_older_ones_still_read() {
    let record = lavapipe();
    assert!(
        record
            .surface_extensions
            .contains(&"VK_EXT_headless_surface".to_owned()),
        "the fixture was recorded after 4.1v(b): {:?}",
        record.surface_extensions
    );
    // A 4.1v(a) record has no `surface_extensions` field.
    let mut json: serde_json::Value = serde_json::to_value(&record).unwrap();
    json.as_object_mut().unwrap().remove("surface_extensions");
    let old: ProbeRecord = serde_json::from_value(json).unwrap();
    assert!(old.surface_extensions.is_empty());
    assert_eq!(
        caps_from_record(&old),
        caps_from_record(&record),
        "caps don't depend on them"
    );
}

/// The renderer indexes bindless textures with per-pixel values (`NonUniformResourceIndex`), so
/// SPIR-V declares `SampledImageArrayNonUniformIndexing` (and storage images likewise): both are
/// required (4.1v(b), found on lavapipe under validation).
#[test]
fn bindless_textures_need_non_uniform_indexing() {
    let mut record = lavapipe();
    assert!(record.missing().is_empty(), "lavapipe has both");
    record
        .features
        .shader_sampled_image_array_non_uniform_indexing = false;
    record
        .features
        .shader_storage_image_array_non_uniform_indexing = false;
    assert_eq!(
        record.missing(),
        [
            "shaderSampledImageArrayNonUniformIndexing",
            "shaderStorageImageArrayNonUniformIndexing"
        ]
    );
}
