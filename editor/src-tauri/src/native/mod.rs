//! The native viewport (ADR-0043 as amended; M4 4.1v(c)): a native view at the page's viewport
//! slot that the engine presents into. macOS puts a Metal layer below the transparent webview;
//! Linux puts an X11 child window over it and presents through Vulkan. The rules here are
//! platform-neutral and tested everywhere.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;

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

/// A child window's place in its parent, in device pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Where the slot lies in a parent window `parent_height` device pixels tall: the page may start
/// below the window's top (a header), so the slot is measured from the page's top. X11 windows
/// are never empty, so sizes are at least 1.
pub fn child_rect(rect: &SlotRect, parent_height: u32) -> Result<PixelRect, String> {
    rect.validate()?;
    let px = |v: f64| (v * rect.scale).round();
    let top = (f64::from(parent_height) - px(rect.viewport_height)).max(0.0);
    let size = |v: f64| px(v).clamp(1.0, f64::from(u16::MAX)) as u32;
    let at = |v: f64| v.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i32;
    Ok(PixelRect {
        x: at(px(rect.x)),
        y: at(top + px(rect.y)),
        width: size(rect.width),
        height: size(rect.height),
    })
}

/// True when GTK will open its windows on X11: `GDK_BACKEND`'s first choice when set (not `*`),
/// otherwise Wayland when `WAYLAND_DISPLAY` is set and X11 when `DISPLAY` is. Empty is unset.
pub fn x11_session(
    gdk_backend: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
) -> bool {
    let set = |v: Option<&str>| v.is_some_and(|v| !v.is_empty());
    let first = gdk_backend
        .and_then(|b| b.split(',').next())
        .map(str::trim)
        .filter(|b| !b.is_empty() && *b != "*");
    match first {
        Some(backend) => backend == "x11" && set(display),
        None => !set(wayland_display) && set(display),
    }
}

/// What the shell does natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSupport {
    /// The engine presents into a native view at the slot.
    pub viewport: bool,
    /// The shell's own input grabs the gizmo and drives the camera.
    pub input: bool,
}

impl NativeSupport {
    /// Nothing native: the page renders the viewport from captures.
    pub const NONE: Self = Self {
        viewport: false,
        input: false,
    };
}

/// The native features of `os` (`std::env::consts::OS`): all on macOS; on Linux the viewport only,
/// and only in an X11 session (embedding into Wayland is not built).
pub fn support(os: &str, x11: bool) -> NativeSupport {
    match os {
        "macos" => NativeSupport {
            viewport: true,
            input: true,
        },
        "linux" if x11 => NativeSupport {
            viewport: true,
            input: false,
        },
        _ => NativeSupport::NONE,
    }
}
