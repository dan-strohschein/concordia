//! The closure budget (M2 2.3b, ADR-0068): each platform has a default, and a project may lower
//! it but never raise it.

use concordia_render::{ClosureBudget, effective_closure_budget, platform_closure_budget};
use concordia_rhi::{Backend, RhiCaps, SampleCount, TimestampGranularity};

fn caps(backend: Backend) -> RhiCaps {
    RhiCaps {
        backend,
        device_name: "test".into(),
        unified_memory: true,
        msaa_sample_counts: vec![SampleCount::X1, SampleCount::X4],
        bindless: true,
        mesh_shaders: false,
        object_shaders: false,
        ray_tracing: false,
        hw_ray_tracing: false,
        metalfx_temporal: false,
        depth_resolve_min: false,
        fragment_atomics: false,
        max_render_extent: 0,
        timestamp_granularity: TimestampGranularity::CommandBuffer,
        max_mesh_output_vertices: 0,
        max_mesh_output_primitives: 0,
        max_buffer_bytes: 1 << 30,
        max_bindless_textures: 1 << 16,
        max_bindless_buffers: 1 << 16,
        feature_sets: Vec::new(),
    }
}

fn budget(n: u8) -> ClosureBudget {
    ClosureBudget::new(n).unwrap()
}

#[test]
fn metal_evaluates_up_to_eight_closures_per_pixel() {
    assert_eq!(platform_closure_budget(&caps(Backend::Metal)), budget(8));
}

#[test]
fn a_project_budget_only_lowers_the_platforms() {
    let platform = budget(4);
    assert_eq!(effective_closure_budget(platform, None), budget(4));
    assert_eq!(
        effective_closure_budget(platform, Some(budget(2))),
        budget(2)
    );
    assert_eq!(
        effective_closure_budget(platform, Some(budget(4))),
        budget(4)
    );
    assert_eq!(
        effective_closure_budget(platform, Some(budget(8))),
        budget(4),
        "a project cannot raise the platform's budget"
    );
}
