//! Windowing (winit) and file watching, behind engine-owned types.
//!
//! On macOS the window event loop must own the main thread; simulation and rendering run elsewhere.

mod error;
mod keys;
mod watch;
mod window;

pub use error::PlatformError;
pub use keys::{key_from_winit, mouse_button_from_winit};
pub use watch::{ChangeKind, FileChanged, FileWatcher};
pub use window::{Flow, RunOptions, WindowApp, WindowDesc, WindowEvent, run_window};
