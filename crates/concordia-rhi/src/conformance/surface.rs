//! Surface conformance cases: acquire, present and misuse on a backend's test surface.

use std::time::Duration;

use crate::caps::TimestampGranularity;
use crate::command::{
    ClearValue, ColorAttachment, Command, CommandList, LoadOp, RenderPassDesc, StoreOp,
};
use crate::device::Device;
use crate::error::RhiError;
use crate::format::Format;
use crate::handle::{Surface, Texture};
use crate::surface::{Present, SurfaceTarget};

/// A backend's test surface: its target, and whatever keeps the native object alive.
pub struct TestSurface {
    /// What to pass to [`Device::create_surface`].
    pub target: SurfaceTarget,
    /// Keeps the native object (e.g. a `CAMetalLayer`) alive while the surface exists.
    pub keep: Box<dyn std::any::Any>,
}

/// Makes a fresh test surface target.
pub type SurfaceMaker<'a> = &'a dyn Fn() -> TestSurface;

const WAIT: Duration = Duration::from_secs(5);

fn create(device: &dyn Device, surface: &TestSurface, format: Format) -> Result<Surface, RhiError> {
    // SAFETY: `surface.keep` outlives every surface a case creates (each destroys its own first).
    unsafe { device.create_surface(surface.target, format) }
}

fn clear(texture: Texture) -> CommandList {
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

/// Acquiring, clearing and presenting repeatedly releases each image's handle.
pub fn acquire_clear_present_cycles_without_leaking_handles(
    device: &dyn Device,
    make: SurfaceMaker<'_>,
) {
    let native = make();
    let surface = create(device, &native, Format::Bgra8UnormSrgb).unwrap();
    for _ in 0..12 {
        let texture = device.acquire(surface, 64, 48).unwrap().expect("an image");
        device
            .present(surface, Present::new(&clear(texture)))
            .unwrap();
        assert!(
            device.texture_index(texture).is_err(),
            "the image's handle is released after present"
        );
    }
    device.wait_idle().unwrap();
    device.destroy_surface(surface);
    assert!(
        device.acquire(surface, 64, 48).is_err(),
        "a destroyed surface is stale"
    );
}

/// Presenting without an image, zero sizes and unpresentable formats are errors.
pub fn surface_misuse_is_reported(device: &dyn Device, make: SurfaceMaker<'_>) {
    let native = make();
    let surface = create(device, &native, Format::Bgra8UnormSrgb).unwrap();
    assert!(matches!(
        device.present(surface, Present::new(&CommandList::new("nothing acquired"))),
        Err(RhiError::Invalid { .. })
    ));
    assert!(matches!(
        device.acquire(surface, 0, 10),
        Err(RhiError::Invalid { .. })
    ));
    let image = device.acquire(surface, 16, 16).unwrap().expect("an image");
    assert!(
        matches!(
            device.acquire(surface, 16, 16),
            Err(RhiError::Invalid { .. })
        ),
        "one image at a time"
    );
    device
        .present(surface, Present::new(&clear(image)))
        .unwrap();
    device.destroy_surface(surface);
    let rgba = create(device, &native, Format::Rgba32Float);
    assert!(
        matches!(rgba, Err(RhiError::Unsupported(_))),
        "surfaces take BGRA8 (sRGB or not) or RGBA16F: {rgba:?}"
    );
}

/// Present signals the frame's timelines and reports when the image was presented.
pub fn present_signals_timelines_and_reports_presentation(
    device: &dyn Device,
    make: SurfaceMaker<'_>,
) {
    let native = make();
    let surface = create(device, &native, Format::Bgra8UnormSrgb).unwrap();
    let timeline = device.create_timeline(0).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let texture = device.acquire(surface, 32, 32).unwrap().expect("an image");
    let present = Present {
        commands: &clear(texture),
        signal: &[(timeline, 7)],
        on_presented: Some(Box::new(move |at| tx.send(at).unwrap())),
        on_gpu_time: None,
    };
    device.present(surface, present).unwrap();
    device.wait_timeline(timeline, 7, WAIT).unwrap();
    assert_eq!(
        device.timeline_value(timeline).unwrap(),
        7,
        "the frame's signal fires"
    );
    let at = rx.recv_timeout(WAIT).expect("the presented callback runs");
    if let Some(time) = at {
        let age = std::time::SystemTime::now()
            .duration_since(time)
            .unwrap_or_default();
        assert!(
            age < WAIT,
            "presentation time is recent wall-clock time: {age:?}"
        );
    }
    device.destroy_surface(surface);
}

/// Present reports its commands' GPU time once (when the device measures it).
pub fn present_reports_the_gpu_time_of_its_commands(device: &dyn Device, make: SurfaceMaker<'_>) {
    let native = make();
    let surface = create(device, &native, Format::Bgra8UnormSrgb).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let texture = device.acquire(surface, 64, 64).unwrap().expect("an image");
    let commands = clear(texture);
    let mut present = Present::new(&commands);
    present.on_gpu_time = Some(Box::new(move |ms| tx.send(ms).unwrap()));
    device.present(surface, present).unwrap();
    device.wait_idle().unwrap();
    let timing = rx.recv_timeout(WAIT).expect("GPU time reported");
    assert!(rx.try_recv().is_err(), "reported once");
    let measures = device.caps().timestamp_granularity != TimestampGranularity::None;
    match timing.total_ms {
        Some(ms) => assert!((0.0..1000.0).contains(&ms), "{ms} ms"),
        None => assert!(!measures, "a device with timestamps measures present"),
    }
    device.destroy_surface(surface);
}

/// The surface follows the requested size from frame to frame.
pub fn acquire_follows_the_requested_size(device: &dyn Device, make: SurfaceMaker<'_>) {
    let native = make();
    let surface = create(device, &native, Format::Bgra8UnormSrgb).unwrap();
    for (w, h) in [(64, 48), (128, 96), (32, 32), (128, 96)] {
        let texture = device.acquire(surface, w, h).unwrap().expect("an image");
        device
            .present(surface, Present::new(&clear(texture)))
            .unwrap();
    }
    device.wait_idle().unwrap();
    device.destroy_surface(surface);
}

/// Destroying a surface with an image acquired releases the image.
pub fn destroying_a_surface_releases_its_acquired_image(
    device: &dyn Device,
    make: SurfaceMaker<'_>,
) {
    let native = make();
    let surface = create(device, &native, Format::Bgra8UnormSrgb).unwrap();
    let texture = device.acquire(surface, 16, 16).unwrap().expect("an image");
    device.destroy_surface(surface);
    assert!(
        device.texture_index(texture).is_err(),
        "the acquired image is released"
    );
    device.wait_idle().unwrap();
}
