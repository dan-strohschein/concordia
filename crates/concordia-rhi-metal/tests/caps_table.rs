//! Capabilities follow Apple's family table (M4 phase 4.1a, D10), checked from probe fixtures, so
//! it runs on every OS and covers devices we don't have (Apple10).

use concordia_rhi::TimestampGranularity;
use concordia_rhi_metal::record::{ProbeRecord, SamplingPoint, caps_from_record};

fn fixture(name: &str) -> ProbeRecord {
    let path = format!(
        "{}/tests/fixtures/probes/{name}.probe.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// One row of the hand-written table: what a family's caps must be.
struct Row {
    fixture: &'static str,
    mesh_shaders: bool,
    ray_tracing: bool,
    hw_ray_tracing: bool,
    metalfx_temporal: bool,
}

const TABLE: [Row; 4] = [
    Row {
        fixture: "apple7-m1",
        mesh_shaders: true,
        ray_tracing: true,
        hw_ray_tracing: false,
        metalfx_temporal: true,
    },
    Row {
        fixture: "apple8-m2",
        mesh_shaders: true,
        ray_tracing: true,
        hw_ray_tracing: false,
        metalfx_temporal: true,
    },
    Row {
        fixture: "apple9-m3-pro",
        mesh_shaders: true,
        ray_tracing: true,
        hw_ray_tracing: true,
        metalfx_temporal: true,
    },
    Row {
        fixture: "apple10-m5",
        mesh_shaders: true,
        ray_tracing: true,
        hw_ray_tracing: true,
        metalfx_temporal: true,
    },
];

#[test]
fn caps_report_matches_known_family_table() {
    for row in &TABLE {
        let caps = caps_from_record(&fixture(row.fixture));
        let name = row.fixture;
        assert_eq!(caps.mesh_shaders, row.mesh_shaders, "{name}: mesh shaders");
        assert_eq!(
            caps.object_shaders, row.mesh_shaders,
            "{name}: object shaders"
        );
        assert_eq!(caps.ray_tracing, row.ray_tracing, "{name}: ray tracing API");
        assert_eq!(
            caps.hw_ray_tracing, row.hw_ray_tracing,
            "{name}: hardware RT"
        );
        assert_eq!(
            caps.metalfx_temporal, row.metalfx_temporal,
            "{name}: MetalFX"
        );
        assert!(caps.bindless, "{name}: argument buffers tier 2");
        assert_eq!(
            caps.timestamp_granularity,
            TimestampGranularity::StageBoundary,
            "{name}: Apple GPUs sample at stage boundaries"
        );
        assert_eq!(
            (
                caps.max_mesh_output_vertices,
                caps.max_mesh_output_primitives
            ),
            (256, 512),
            "{name}: Metal's mesh output limits"
        );
        assert_eq!(caps.feature_sets, fixture(name).families, "{name}");
    }
}

#[test]
fn a_device_without_metal3_has_no_mesh_shaders_or_output_limits() {
    let mut record = fixture("apple7-m1");
    record
        .families
        .retain(|f| f != "metal3" && f != "metal4" && f != "apple7");
    let caps = caps_from_record(&record);
    assert!(!caps.mesh_shaders && !caps.object_shaders);
    assert_eq!(caps.max_mesh_output_vertices, 0);
    assert_eq!(caps.max_mesh_output_primitives, 0);
}

#[test]
fn timestamp_granularity_follows_counter_support() {
    let mut record = fixture("apple9-m3-pro");
    record.counter_sampling = vec![SamplingPoint::Stage, SamplingPoint::Draw];
    assert_eq!(
        caps_from_record(&record).timestamp_granularity,
        TimestampGranularity::DrawBoundary
    );
    record.counter_sampling.clear();
    assert_eq!(
        caps_from_record(&record).timestamp_granularity,
        TimestampGranularity::CommandBuffer
    );
    record.counter_sampling = vec![SamplingPoint::Stage];
    record.timestamp_counter_set = false;
    assert_eq!(
        caps_from_record(&record).timestamp_granularity,
        TimestampGranularity::CommandBuffer,
        "sampling points without the timestamp counter set give nothing to sample"
    );
}

/// Min depth resolves (M4 4.4b) follow Apple's feature-set table: every Apple family from Apple3
/// and Mac2 has the min and max depth resolve filters.
#[test]
fn depth_resolve_min_follows_the_family_table() {
    for row in &TABLE {
        assert!(
            caps_from_record(&fixture(row.fixture)).depth_resolve_min,
            "{}",
            row.fixture
        );
    }
    let mut record = fixture("apple9-m3-pro");
    record.families = vec!["common3".into()];
    assert!(!caps_from_record(&record).depth_resolve_min);
    record.families = vec!["mac2".into()];
    assert!(caps_from_record(&record).depth_resolve_min);
}

#[test]
fn bindless_needs_argument_buffers_tier_2() {
    let mut record = fixture("apple9-m3-pro");
    record.argument_buffers_tier = 1;
    assert!(!caps_from_record(&record).bindless);
}

/// The live device's caps are its record's caps, and MetalFX agrees with the family rule.
#[test]
fn live_probe_agrees_with_its_record() {
    let Ok(record) = concordia_rhi_metal::probe_record() else {
        assert!(!concordia_testkit::gpu_required(), "no Metal device");
        return;
    };
    let caps = concordia_rhi_metal::probe().expect("probe after record");
    assert_eq!(caps, caps_from_record(&record));
    if record.families.iter().any(|f| f == "apple7") {
        assert!(
            record.metalfx_temporal,
            "Apple7+ has MetalFX temporal scaling"
        );
    }
}
