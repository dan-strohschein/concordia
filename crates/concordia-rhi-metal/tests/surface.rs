//! Presenting to a window surface (`CAMetalLayer`), M1 phase 1.9c. Uses a standalone layer, so no
//! window is needed. Skips without a GPU unless CONCORDIA_REQUIRE_GPU=1.
#![cfg(target_os = "macos")]

use std::ptr::NonNull;

use concordia_rhi::{
    ClearValue, ColorAttachment, Command, CommandList, Device, Format, LoadOp, RenderPassDesc,
    RhiError, StoreOp, SurfaceTarget,
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
        device.present(surface, &clear(texture)).unwrap();
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
        device.present(surface, &CommandList::new("nothing acquired")),
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
