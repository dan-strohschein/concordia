//! Presenting to a window surface (`CAMetalLayer`), M1 phase 1.9c: the shared surface conformance
//! cases (M4 4.1v(b)) plus Metal's own. Uses a standalone layer, so no window is needed. Skips
//! without a GPU unless CONCORDIA_REQUIRE_GPU=1.
#![cfg(target_os = "macos")]

use std::ptr::NonNull;

use concordia_rhi::{
    ClearValue, ColorAttachment, Command, CommandList, Device, Format, LoadOp, Present,
    RenderPassDesc, RhiError, StoreOp, SurfaceTarget,
};
use concordia_testkit::gpu_required;
use objc2::rc::Retained;
use objc2_quartz_core::CAMetalLayer;

fn device() -> Option<Box<dyn Device>> {
    match concordia_rhi_metal::new_headless() {
        Ok(d) => Some(d),
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            None
        }
        Err(e) => panic!("{e}"),
    }
}

fn layer() -> Retained<CAMetalLayer> {
    CAMetalLayer::new()
}

fn target(layer: &CAMetalLayer) -> SurfaceTarget {
    SurfaceTarget::MetalLayer(NonNull::from(layer).cast())
}

fn test_surface() -> concordia_rhi::conformance::TestSurface {
    let layer = layer();
    concordia_rhi::conformance::TestSurface {
        target: target(&layer),
        keep: Box::new(layer),
    }
}

concordia_rhi::surface_conformance_tests!(device, test_surface);

fn clear(texture: concordia_rhi::Texture) -> CommandList {
    let mut list = CommandList::new("clear surface");
    list.push(Command::BeginRenderPass(RenderPassDesc {
        colors: vec![ColorAttachment {
            texture,
            resolve: None,
            load: LoadOp::Clear(ClearValue::Color([0.2, 0.4, 0.6, 1.0])),
            store: StoreOp::Store,
        }],
        depth: None,
    }))
    .push(Command::EndRenderPass);
    list
}

/// The engine renders on a long-lived thread: per-frame Metal objects must not pile up in a
/// never-drained autorelease pool (they retain every resource the frame used).
#[test]
fn frame_resources_are_freed_on_a_long_lived_thread() {
    let Some(device) = device() else { return };
    let layer = layer();
    // SAFETY: the layer outlives the surface (destroyed below).
    let surface = unsafe { device.create_surface(target(&layer), Format::Bgra8UnormSrgb) }.unwrap();
    let side = std::num::NonZeroU32::new(2048).unwrap();
    let desc = concordia_rhi::TextureDesc {
        width: side,
        height: side,
        mip_levels: std::num::NonZeroU32::MIN,
        samples: concordia_rhi::SampleCount::X1,
        format: Format::Rgba16Float,
        usage: concordia_rhi::TextureUsage::RENDER_TARGET,
        label: "frame target".into(),
    };
    let frame = |device: &dyn Device| {
        let scratch = device.create_texture(&desc).unwrap();
        let mut list = clear(scratch);
        if let Some(image) = device.acquire(surface, 256, 256).unwrap() {
            list.commands.extend(clear(image).commands);
            device.present(surface, Present::new(&list)).unwrap();
        }
        device.wait_idle().unwrap();
        device.destroy_texture(scratch);
    };
    frame(device.as_ref());
    let before = device
        .allocated_bytes()
        .expect("Metal reports allocated bytes");
    for _ in 0..40 {
        frame(device.as_ref());
    }
    let grown = device.allocated_bytes().unwrap().saturating_sub(before);
    assert!(
        grown < 128 << 20,
        "40 frames of a 32 MB target grew allocations by {} MB",
        grown >> 20
    );
    device.destroy_surface(surface);
}

#[test]
fn window_surfaces_keep_a_short_drawable_queue_for_low_latency() {
    let Some(device) = device() else { return };
    let layer = layer();
    // SAFETY: the layer outlives the surface (destroyed below).
    let surface = unsafe { device.create_surface(target(&layer), Format::Bgra8UnormSrgb) }.unwrap();
    assert_eq!(layer.maximumDrawableCount(), 2);
    device.destroy_surface(surface);
}

/// Metal presents only into `CAMetalLayer`s; other platforms' targets are `Unsupported`.
#[test]
fn metal_refuses_other_platforms_surface_targets() {
    let Some(device) = device() else { return };
    let a = NonNull::<std::ffi::c_void>::dangling();
    for target in [
        SurfaceTarget::Xlib {
            display: a,
            window: 1,
        },
        SurfaceTarget::Xcb {
            connection: a,
            window: 1,
        },
        SurfaceTarget::Wayland {
            display: a,
            surface: a,
        },
        SurfaceTarget::Headless,
    ] {
        // SAFETY: refused before anything behind the pointers is touched.
        let made = unsafe { device.create_surface(target, Format::Bgra8UnormSrgb) };
        assert!(
            matches!(made, Err(RhiError::Unsupported(_))),
            "{target:?}: {made:?}"
        );
    }
}
