//! The native viewport on Linux (M4 4.1v(c)): an X11 child window of the editor's window at the
//! page's viewport slot, which the engine presents into through Vulkan (`SurfaceTarget::Xcb`).
//! The editor opens its own XCB connection for it, so the surface never shares GTK's. The child
//! selects no input, so mouse events reach the page underneath. Main thread only.

use std::cell::RefCell;
use std::ptr::NonNull;

use concordia_rhi::SurfaceTarget;
use gtk::prelude::WidgetExt;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::WebviewWindow;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt, CreateWindowAux, WindowClass};
use x11rb::xcb_ffi::XCBConnection;

use super::{SlotRect, child_rect};

struct Native {
    connection: XCBConnection,
    parent: u32,
    child: u32,
}

thread_local! {
    static NATIVE: RefCell<Option<Native>> = const { RefCell::new(None) };
}

fn x11(e: impl std::fmt::Display) -> String {
    format!("the native viewport's X11 window: {e}")
}

/// The editor window's X11 id, or why there is none (a Wayland session).
fn parent_window(window: &WebviewWindow) -> Result<u32, String> {
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    match handle.as_raw() {
        RawWindowHandle::Xlib(h) => u32::try_from(h.window).map_err(x11),
        RawWindowHandle::Xcb(h) => Ok(h.window.get()),
        _ => Err(
            "the native viewport needs X11; under Wayland run the editor with GDK_BACKEND=x11"
                .into(),
        ),
    }
}

/// Adds the viewport's child window to `window`; returns it for `concordia_render::attach_surface`.
pub fn install(window: &WebviewWindow) -> Result<SurfaceTarget, String> {
    let parent = parent_window(window)?;
    let (connection, _) = XCBConnection::connect(None).map_err(x11)?;
    let child = connection.generate_id().map_err(x11)?;
    // No event mask and no background: input falls through to the page, and X never clears it.
    let aux = CreateWindowAux::new().background_pixmap(x11rb::NONE);
    connection
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            child,
            parent,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &aux,
        )
        .map_err(x11)?
        .check()
        .map_err(x11)?;
    connection.map_window(child).map_err(x11)?;
    connection.flush().map_err(x11)?;
    let raw =
        NonNull::new(connection.get_raw_xcb_connection()).ok_or_else(|| x11("no connection"))?;
    NATIVE.with(|n| {
        *n.borrow_mut() = Some(Native {
            connection,
            parent,
            child,
        })
    });
    Ok(SurfaceTarget::Xcb {
        connection: raw,
        window: child,
    })
}

/// Moves and sizes the child window to the slot.
pub fn place(rect: SlotRect) {
    NATIVE.with(|n| {
        let n = n.borrow();
        let Some(n) = n.as_ref() else { return };
        if let Err(e) = configure(n, &rect) {
            eprintln!("concordia-editor: {e}");
        }
    });
}

fn configure(n: &Native, rect: &SlotRect) -> Result<(), String> {
    let parent = n
        .connection
        .get_geometry(n.parent)
        .map_err(x11)?
        .reply()
        .map_err(x11)?;
    let at = child_rect(rect, u32::from(parent.height))?;
    let aux = ConfigureWindowAux::new()
        .x(at.x)
        .y(at.y)
        .width(at.width)
        .height(at.height);
    n.connection.configure_window(n.child, &aux).map_err(x11)?;
    n.connection.flush().map_err(x11)
}

/// Runs `detach` before the editor window's X11 window is destroyed (GTK's `unrealize` runs
/// before its default handler destroys it), then destroys the child window.
pub fn before_window_goes(
    window: &WebviewWindow,
    detach: impl Fn() + 'static,
) -> Result<(), String> {
    let gtk = window.gtk_window().map_err(|e| e.to_string())?;
    gtk.connect_unrealize(move |_| {
        detach();
        destroy();
    });
    Ok(())
}

/// Destroys the child window and closes the connection (after the surface is detached).
fn destroy() {
    if let Some(n) = NATIVE.with(|n| n.borrow_mut().take()) {
        let _ = n.connection.destroy_window(n.child);
        let _ = n.connection.flush();
    }
}
