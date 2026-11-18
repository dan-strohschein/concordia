//! The backend-agnostic conformance suite. Each backend instantiates it with
//! [`conformance_tests!`](crate::conformance_tests) in its `tests/conformance.rs`.

mod cases;
mod shaders;
mod util;

pub use cases::*;

/// Compares a rendered RGBA8 image with a golden (backends pass a `concordia-testkit` implementation).
pub trait GoldenCheck {
    /// Panics if `rgba` (tightly packed `width`×`height`) does not match golden `name` within `mean_flip`.
    fn check(&self, name: &str, width: u32, height: u32, rgba: &[u8], mean_flip: f32);
}

/// One `#[test]` per conformance case. `$make` returns `Option<Box<dyn Device>>` (`None` skips);
/// `$golden` is a `GoldenCheck`.
#[macro_export]
macro_rules! conformance_tests {
    ($make:expr, $golden:expr) => {
        $crate::conformance_tests!(@cases $make, $golden;
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
            invalid_descriptors_and_stale_handles_are_errors
        );
    };
    (@cases $make:expr, $golden:expr; $($name:ident),+) => {$(
        #[test]
        fn $name() {
            let Some(device) = ($make)() else { return };
            $crate::conformance::$name(&*device, &$golden);
        }
    )+};
}
