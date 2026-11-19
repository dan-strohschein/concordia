//! Presenting to a window surface (`CAMetalLayer`), M1 phase 1.9c. Uses a standalone layer, so no
//! window is needed. Skips without a GPU unless CONCORDIA_REQUIRE_GPU=1.
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

#[test]
fn acquire_clear_present_cycles_without_leaking_handles() {
    let Some(device) = device() else { return };
    let layer = layer();
    // SAFETY: the layer outlives the surface (destroyed below).
    let surface = unsafe { device.create_surface(target(&layer), Format::Bgra8UnormSrgb) }.unwrap();
    let mut last = None;
    for _ in 0..12 {
        let texture = device
            .acquire(surface, 64, 48)
            .unwrap()
            .expect("a drawable");
        device
            .present(surface, Present::new(&clear(texture)))
            .unwrap();
        assert!(
            device.texture_index(texture).is_err(),
            "the drawable's handle is released after present"
        );
        last = Some(texture);
    }
    device.wait_idle().unwrap();
    assert!(last.is_some());
    device.destroy_surface(surface);
    assert!(
        device.acquire(surface, 64, 48).is_err(),
        "a destroyed surface is stale"
    );
}

#[test]
fn surface_misuse_is_reported() {
    let Some(device) = device() else { return };
    let layer = layer();
    // SAFETY: as above.
    let surface = unsafe { device.create_surface(target(&layer), Format::Bgra8UnormSrgb) }.unwrap();
    assert!(matches!(
        device.present(surface, Present::new(&CommandList::new("nothing acquired"))),
        Err(RhiError::Invalid { .. })
    ));
    assert!(matches!(
        device.acquire(surface, 0, 10),
        Err(RhiError::Invalid { .. })
    ));
    // SAFETY: as above.
    let rgba = unsafe { device.create_surface(target(&layer), Format::Rgba32Float) };
    assert!(
        matches!(rgba, Err(RhiError::Unsupported(_))),
        "layers only take BGRA8 (sRGB or not) or RGBA16F"
    );
    device.destroy_surface(surface);
}

#[test]
fn present_signals_timelines_and_reports_presentation() {
    let Some(device) = device() else { return };
    let layer = layer();
    // SAFETY: the layer outlives the surface (destroyed below).
    let surface = unsafe { device.create_surface(target(&layer), Format::Bgra8UnormSrgb) }.unwrap();
    let timeline = device.create_timeline(0).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let texture = device
        .acquire(surface, 32, 32)
        .unwrap()
        .expect("a drawable");
    let present = Present {
        commands: &clear(texture),
        signal: &[(timeline, 7)],
        on_presented: Some(Box::new(move |at| tx.send(at).unwrap())),
    };
    device.present(surface, present).unwrap();
    device
        .wait_timeline(timeline, 7, std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        device.timeline_value(timeline).unwrap(),
        7,
        "the frame's timeline signal fires"
    );
    let at = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("the presented callback runs");
    if let Some(time) = at {
        let age = std::time::SystemTime::now()
            .duration_since(time)
            .unwrap_or_default();
        assert!(
            age < std::time::Duration::from_secs(5),
            "presentation time is recent wall-clock time: {age:?}"
        );
    }
    device.destroy_surface(surface);
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
