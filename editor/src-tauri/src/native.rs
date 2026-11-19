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

/// The maximum refresh rate of the window's display, in Hz.
pub fn display_hz() -> Option<f64> {
    NATIVE.with(|n| {
        let n = n.borrow();
        let hz = n.as_ref()?.window.screen()?.maximumFramesPerSecond();
        (hz > 0).then_some(hz as f64)
    })
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

thread_local! {
    static NAVIGATOR: RefCell<crate::input::Navigator> = RefCell::new(crate::input::Navigator::default());
    static MONITOR: RefCell<Option<Retained<objc2::runtime::AnyObject>>> = const { RefCell::new(None) };
}

/// Sends viewport navigation gestures to `on_nav` (with the event's uptime timestamp, seconds)
/// before the page sees them; other events pass through. Main thread, after [`install`].
pub fn install_input(on_nav: impl Fn(crate::input::Nav, f64) + 'static) {
    use objc2_app_kit::{NSEvent, NSEventMask};
    let mask = NSEventMask::LeftMouseDown
        | NSEventMask::LeftMouseDragged
        | NSEventMask::LeftMouseUp
        | NSEventMask::OtherMouseDown
        | NSEventMask::OtherMouseDragged
        | NSEventMask::OtherMouseUp
        | NSEventMask::ScrollWheel;
    let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a live event for the duration of the handler.
        let e = unsafe { event.as_ref() };
        let Some((input, in_slot)) = translate(e) else {
            return event.as_ptr();
        };
        match NAVIGATOR.with(|n| n.borrow_mut().handle(input, in_slot)) {
            Some(nav) => {
                if nav.is_move() {
                    on_nav(nav, e.timestamp());
                }
                std::ptr::null_mut()
            }
            None => event.as_ptr(),
        }
    });
    // SAFETY: the block returns the event it was given or null, as AppKit requires.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
    MONITOR.with(|m| *m.borrow_mut() = monitor);
}

/// The event as navigator input, and whether it is over the viewport (our window's only).
fn translate(e: &objc2_app_kit::NSEvent) -> Option<(crate::input::Input, bool)> {
    use crate::input::{Button, Input};
    use objc2_app_kit::{NSEventModifierFlags, NSEventType};
    NATIVE.with(|n| {
        let n = n.borrow();
        let n = n.as_ref()?;
        if e.windowNumber() != n.window.windowNumber() {
            return None;
        }
        let at = e.locationInWindow();
        let local = n.view.convertPoint_fromView(at, None);
        let bounds = n.view.bounds();
        let in_slot = local.x >= 0.0
            && local.y >= 0.0
            && local.x < bounds.size.width
            && local.y < bounds.size.height;
        let flags = e.modifierFlags();
        let down = |button| Input::Down {
            button,
            x: at.x,
            y: at.y,
            alt: flags.contains(NSEventModifierFlags::Option),
            shift: flags.contains(NSEventModifierFlags::Shift),
        };
        let input = match e.r#type() {
            NSEventType::LeftMouseDown => down(Button::Left),
            NSEventType::OtherMouseDown if e.buttonNumber() == 2 => down(Button::Middle),
            NSEventType::OtherMouseDown => down(Button::Other),
            NSEventType::LeftMouseDragged | NSEventType::OtherMouseDragged => {
                Input::Drag { x: at.x, y: at.y }
            }
            NSEventType::LeftMouseUp | NSEventType::OtherMouseUp => Input::Up,
            NSEventType::ScrollWheel => Input::Scroll {
                dy: e.scrollingDeltaY(),
            },
            _ => return None,
        };
        Some((input, in_slot))
    })
}

/// One step of a synthetic Alt+left-drag across the viewport (benchmarks).
#[derive(Debug, Clone, Copy)]
pub enum DragStep {
    Down,
    /// Points right of the viewport's centre.
    Move(f64),
    Up(f64),
}

/// Posts a synthetic drag step into the app's event queue, as if from the mouse. Main thread.
pub fn post_drag(step: DragStep) {
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    NATIVE.with(|n| {
        let n = n.borrow();
        let Some(n) = n.as_ref() else { return };
        let bounds = n.view.bounds();
        let centre = NSPoint::new(bounds.size.width / 2.0, bounds.size.height / 2.0);
        let base = n.view.convertPoint_toView(centre, None);
        let (kind, dx) = match step {
            DragStep::Down => (NSEventType::LeftMouseDown, 0.0),
            DragStep::Move(dx) => (NSEventType::LeftMouseDragged, dx),
            DragStep::Up(dx) => (NSEventType::LeftMouseUp, dx),
        };
        let uptime = objc2_foundation::NSProcessInfo::processInfo().systemUptime();
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            NSPoint::new(base.x + dx, base.y),
            NSEventModifierFlags::Option,
            uptime,
            n.window.windowNumber(),
            None,
            0,
            1,
            1.0,
        );
        if let Some(event) = event {
            NSApplication::sharedApplication(mtm).postEvent_atStart(&event, false);
        }
    });
}

/// Converts an AppKit event timestamp (seconds of uptime) to Unix milliseconds.
pub fn uptime_to_unix_ms(uptime: f64) -> f64 {
    let age = (objc2_quartz_core::CACurrentMediaTime() - uptime).max(0.0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    now.as_secs_f64() * 1000.0 - age * 1000.0
}
