//! The native viewport (macOS, ADR-0043 as amended): an `NSView` with a `CAMetalLayer` ordered
//! below the transparent webview and placed at the page's viewport slot. Main thread only.

use std::cell::RefCell;
use std::ptr::NonNull;

use concordia_rhi::SurfaceTarget;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow, NSWindowOrderingMode};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_quartz_core::CAMetalLayer;
use tauri::WebviewWindow;

/// The slot in CSS px from the page's top-left, the device pixel ratio and the page's viewport height.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct SlotRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub viewport_height: f64,
}

impl SlotRect {
    /// Finite, non-negative sizes and a positive scale.
    pub fn validate(&self) -> Result<(), String> {
        let finite = [
            self.x,
            self.y,
            self.width,
            self.height,
            self.scale,
            self.viewport_height,
        ]
        .iter()
        .all(|v| v.is_finite());
        match finite
            && self.width >= 0.0
            && self.height >= 0.0
            && self.scale > 0.0
            && self.viewport_height > 0.0
        {
            true => Ok(()),
            false => Err(format!("invalid slot rect {self:?}")),
        }
    }
}

struct Native {
    window: Retained<NSWindow>,
    view: Retained<NSView>,
    layer: Retained<CAMetalLayer>,
}

thread_local! {
    static NATIVE: RefCell<Option<Native>> = const { RefCell::new(None) };
}

/// Adds the Metal view below the webview; returns the layer for `concordia_render::attach_surface`.
pub fn install(window: &WebviewWindow) -> Result<SurfaceTarget, String> {
    let mtm = MainThreadMarker::new()
        .ok_or("the native viewport must be installed on the main thread")?;
    let ns_window = window.ns_window().map_err(|e| e.to_string())?;
    // SAFETY: Tauri returns the window's live NSWindow; retaining keeps it alive while we hold it.
    let window: Retained<NSWindow> =
        unsafe { Retained::retain(ns_window.cast()) }.ok_or("null NSWindow")?;
    let layer = CAMetalLayer::new();
    let view = NSView::initWithFrame(
        mtm.alloc(),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0)),
    );
    view.setWantsLayer(true);
    view.setLayer(Some(&layer));
    let content = window.contentView().ok_or("window has no content view")?;
    content.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, None);
    let target = SurfaceTarget::MetalLayer(NonNull::from(&*layer).cast());
    NATIVE.with(|n| {
        *n.borrow_mut() = Some(Native {
            window,
            view,
            layer,
        })
    });
    Ok(target)
}

/// The webview: the content view's subview that is not ours.
fn webview(n: &Native) -> Option<Retained<NSView>> {
    let subviews = n.window.contentView()?.subviews();
    (0..subviews.len())
        .map(|i| subviews.objectAtIndex(i))
        .find(|v| !std::ptr::eq(&**v, &*n.view))
}

/// The slot in the content view's coordinates. The webview can extend under the title bar while
/// the page starts below it, so the slot is measured from the page's top inside the webview.
fn content_rect(n: &Native, rect: SlotRect) -> Option<NSRect> {
    let content = n.window.contentView()?;
    let web = webview(n)?;
    let height = web.bounds().size.height;
    let top = (height - rect.viewport_height).max(0.0) + rect.y;
    let y = if web.isFlipped() {
        top
    } else {
        height - top - rect.height
    };
    Some(content.convertRect_fromView(
        NSRect::new(
            NSPoint::new(rect.x, y),
            NSSize::new(rect.width, rect.height),
        ),
        Some(&web),
    ))
}

/// The window number (for `screencapture -l` in local visual checks).
pub fn window_number() -> Option<isize> {
    NATIVE.with(|n| n.borrow().as_ref().map(|n| n.window.windowNumber()))
}

/// Moves and sizes the Metal view to the slot.
pub fn place(rect: SlotRect) {
    NATIVE.with(|n| {
        let n = n.borrow();
        let Some(n) = n.as_ref() else { return };
        if let Some(frame) = content_rect(n, rect) {
            n.view.setFrame(frame);
            n.layer.setContentsScale(rect.scale);
        }
    });
}
