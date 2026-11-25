//! The backend-agnostic conformance suite. Each backend instantiates it with
//! [`conformance_tests!`](crate::conformance_tests) in its `tests/conformance.rs`.

mod cases;
mod surface;
mod util;

pub use cases::*;
pub use surface::*;

use std::path::PathBuf;

use crate::pipeline::ShaderSource;

/// Directory of the conformance shaders (`*.slang`; `import bindless;` from `concordia-shaders`).
pub fn shader_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/conformance"))
}

/// A pipeline stage, for shader lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    /// Compute kernel.
    Compute,
    /// Vertex shader.
    Vertex,
    /// Fragment shader.
    Fragment,
}

/// Compiles conformance shaders for the device under test (backends compile `shader_dir()/<name>.slang`).
pub trait ShaderProvider {
    /// The `entry` of conformance shader `name`, and its compute thread group size.
    fn shader(
        &self,
        name: &str,
        entry: &str,
        stage: ShaderStage,
    ) -> (ShaderSource, Option<[u32; 3]>);
}

/// What each case gets besides the device.
pub struct Harness<'a> {
    /// Golden comparisons.
    pub golden: &'a dyn GoldenCheck,
    /// Shader compilation.
    pub shaders: &'a dyn ShaderProvider,
}

/// Compares a rendered RGBA8 image with a golden (backends pass a `concordia-testkit` implementation).
pub trait GoldenCheck {
    /// Panics if `rgba` (tightly packed `width`×`height`) does not match golden `name` within its
    /// tolerance (`tests/golden/rhi/tolerances.ron`).
    fn check(&self, name: &str, width: u32, height: u32, rgba: &[u8]);
}

/// One `#[test]` per conformance case. `$make` returns `Option<Box<dyn Device>>` (`None` skips);
/// `$golden` is a `GoldenCheck` and `$shaders` a `ShaderProvider`. An optional
/// `finish = |device: &dyn Device| ...` runs after each case (e.g. to fail on validation errors).
#[macro_export]
macro_rules! conformance_tests {
    ($make:expr, $golden:expr, $shaders:expr) => {
        $crate::conformance_tests!($make, $golden, $shaders, finish = |_: &dyn $crate::Device| {});
    };
    ($make:expr, $golden:expr, $shaders:expr, finish = $finish:expr) => {
        $crate::conformance_tests!(@cases $make, $golden, $shaders, $finish;
            device_reports_caps_and_limits,
            buffer_upload_and_readback_round_trips,
            compute_dispatch_writes_expected_values,
            compute_reads_bindless_buffer_by_index,
            triangle_renders_golden,
            clear_color_readback_is_exact,
            depth_test_orders_overlapping_triangles,
            msaa4x_resolve_produces_antialiased_edge,
            barrier_between_compute_write_and_fragment_read_is_respected,
            transient_textures_aliased_in_same_heap_do_not_corrupt,
            timeline_fence_signals_monotonically,
            readback_of_frame_n_does_not_block_frame_n_plus_1_submission,
            pipeline_creation_error_is_reported_not_panicked,
            invalid_descriptors_and_stale_handles_are_errors,
            static_samplers_filter_and_address_as_named,
            additive_blend_sums_overlapping_draws,
            submit_timed_reports_the_gpu_time_once,
            timer_scopes_report_each_scope_once_in_order,
            clip_space_y_is_up_and_back_faces_cull
        );
    };
    (@cases $make:expr, $golden:expr, $shaders:expr, $finish:expr; $($name:ident),+) => {$(
        #[test]
        fn $name() {
            let Some(device) = ($make)() else { return };
            let harness = $crate::conformance::Harness { golden: &$golden, shaders: &$shaders };
            $crate::conformance::$name(&*device, &harness);
            ($finish)(&*device);
        }
    )+};
}

/// One `#[test]` per surface conformance case. `$make` returns `Option<Box<dyn Device>>` (`None`
/// skips) and `$surface` a [`TestSurface`]; `finish` runs after each case as above.
#[macro_export]
macro_rules! surface_conformance_tests {
    ($make:expr, $surface:expr) => {
        $crate::surface_conformance_tests!($make, $surface, finish = |_: &dyn $crate::Device| {});
    };
    ($make:expr, $surface:expr, finish = $finish:expr) => {
        $crate::surface_conformance_tests!(@cases $make, $surface, $finish;
            acquire_clear_present_cycles_without_leaking_handles,
            surface_misuse_is_reported,
            present_signals_timelines_and_reports_presentation,
            present_reports_the_gpu_time_of_its_commands,
            acquire_follows_the_requested_size,
            destroying_a_surface_releases_its_acquired_image,
            reports_arrive_without_another_device_call
        );
    };
    (@cases $make:expr, $surface:expr, $finish:expr; $($name:ident),+) => {$(
        #[test]
        fn $name() {
            let Some(device) = ($make)() else { return };
            $crate::conformance::$name(&*device, &$surface);
            ($finish)(&*device);
        }
    )+};
}
