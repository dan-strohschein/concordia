//! The mesh conformance cases on a Metal device with mesh shaders masked (M4 4.3b): every mesh
//! pipeline is refused as `Unsupported`, the way MoltenVK refuses them. Skips without a device
//! unless CONCORDIA_REQUIRE_GPU=1.

use concordia_rhi::conformance::{self, Harness};
use concordia_rhi::{CapsMask, Device, RhiError};
use concordia_shaders::Target;
use concordia_shaders::conformance::SlangShaders;
use concordia_testkit::golden::RhiGoldens;
use concordia_testkit::gpu_required;

fn masked() -> Option<Box<dyn Device>> {
    let mask = CapsMask {
        mesh_shaders: true,
        ..CapsMask::default()
    };
    match concordia_rhi_metal::new_headless_masked(&mask) {
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

type Case = fn(&dyn Device, &Harness<'_>);

#[test]
fn a_masked_device_refuses_every_mesh_case() {
    let Some(device) = masked() else { return };
    assert!(!device.caps().mesh_shaders && !device.caps().object_shaders);
    let (golden, shaders) = (
        RhiGoldens(concordia_rhi::Backend::Metal),
        SlangShaders::new(Target::Msl),
    );
    let harness = Harness {
        golden: &golden,
        shaders: &shaders,
    };
    let cases: [Case; 5] = [
        conformance::mesh_stage_draws_without_an_object_stage,
        conformance::object_payload_reaches_every_mesh_group,
        conformance::mesh_output_counts_below_the_declared_maximum_draw_only_those,
        conformance::mesh_and_vertex_draws_share_one_depth_buffer,
        conformance::mesh_pipeline_misuse_is_reported,
    ];
    for case in cases {
        case(&*device, &harness);
    }
}
