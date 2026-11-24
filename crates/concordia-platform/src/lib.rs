//! Windowing (winit) and file watching, behind engine-owned types.
//!
//! On macOS the window event loop must own the main thread; simulation and rendering run elsewhere.

mod cursor;
mod error;
mod keys;
mod scroll;
mod typing;
mod watch;
mod window;

pub use cursor::{CursorIcon, cursor_to_winit};
pub use error::PlatformError;
pub use keys::{key_from_winit, mouse_button_from_winit};
pub use scroll::{SCROLL_LINE_PX, scroll_from_winit};
pub use typing::{Modifiers, text_from_key};
pub use watch::{ChangeKind, FileChanged, FileWatcher};
pub use window::{
    Flow, NativeView, RunOptions, STOP_POLL, WindowApp, WindowDesc, WindowEvent, run_window,
};
