//! One window on winit's event loop, reporting engine-owned events.

use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use concordia_input::{InputEvent, TextInput};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseScrollDelta};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::cursor::{CursorIcon, cursor_to_winit};
use crate::error::PlatformError;
use crate::keys::{key_from_winit, mouse_button_from_winit};
use crate::typing::{Modifiers, insert, text_from_key};

/// How to open a window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct WindowDesc {
    /// Title bar text.
    pub title: String,
    /// Width in logical pixels.
    pub width: NonZeroU32,
    /// Height in logical pixels.
    pub height: NonZeroU32,
    /// Whether the user can resize it.
    pub resizable: bool,
    /// Borderless fullscreen on the current display.
    #[serde(default)]
    pub fullscreen: bool,
}

impl Default for WindowDesc {
    fn default() -> Self {
        Self {
            title: "Concordia".into(),
            width: NonZeroU32::new(1280).expect("non-zero literal"),
            height: NonZeroU32::new(720).expect("non-zero literal"),
            resizable: true,
            fullscreen: false,
        }
    }
}

/// The window's native content view (on macOS an `NSView`), for attaching a GPU surface. Valid
/// while the window is open; use it on the main thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeView(pub std::ptr::NonNull<std::ffi::c_void>);

/// Something that happened to the window (engine-owned, not winit's).
#[derive(Debug, Clone, PartialEq)]
pub enum WindowEvent {
    /// The window exists; size in physical pixels.
    Opened {
        /// Width in physical pixels.
        width: u32,
        /// Height in physical pixels.
        height: u32,
        /// Physical pixels per logical pixel.
        scale_factor: f64,
        /// The content view, when the platform exposes one.
        view: Option<NativeView>,
    },
    /// New size in physical pixels.
    Resized {
        /// Width in physical pixels.
        width: u32,
        /// Height in physical pixels.
        height: u32,
    },
    /// The DPI scale changed (e.g. moved to another display).
    ScaleChanged {
        /// Physical pixels per logical pixel.
        scale_factor: f64,
    },
    /// Keyboard focus gained (true) or lost.
    Focused(bool),
    /// The user asked to close the window.
    CloseRequested,
    /// Keyboard, mouse or scroll input (pointer positions in logical pixels).
    Input(InputEvent),
}

/// What the app wants after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep running.
    Continue,
    /// Close the window and return from [`run_window`].
    Exit,
}

/// Receives window events.
pub trait WindowApp {
    /// Handles one event.
    fn event(&mut self, event: WindowEvent) -> Flow;

    /// The cursor to show (asked after every event).
    fn cursor(&self) -> CursorIcon {
        CursorIcon::Arrow
    }
}

/// Options for [`run_window`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Close automatically after this long (smoke tests).
    pub exit_after: Option<Duration>,
}

/// Opens the window and runs the event loop until the app exits; must be called on the main thread.
pub fn run_window(
    desc: &WindowDesc,
    options: RunOptions,
    app: &mut dyn WindowApp,
) -> Result<(), PlatformError> {
    let event_loop = EventLoop::new().map_err(|e| PlatformError::EventLoop(e.to_string()))?;
    let mut runner = Runner {
        desc: desc.clone(),
        app,
        window: None,
        cursor: CursorIcon::Arrow,
        modifiers: Modifiers::default(),
        deadline: options.exit_after.map(|d| Instant::now() + d),
        error: None,
    };
    event_loop
        .run_app(&mut runner)
        .map_err(|e| PlatformError::EventLoop(e.to_string()))?;
    runner.error.map_or(Ok(()), Err)
}

struct Runner<'a> {
    desc: WindowDesc,
    app: &'a mut dyn WindowApp,
    window: Option<Window>,
    cursor: CursorIcon,
    modifiers: Modifiers,
    deadline: Option<Instant>,
    error: Option<PlatformError>,
}

impl Runner<'_> {
    fn send(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        if self.app.event(event) == Flow::Exit {
            event_loop.exit();
        }
        let cursor = self.app.cursor();
        if cursor != self.cursor
            && let Some(window) = &self.window
        {
            window.set_cursor(cursor_to_winit(cursor));
            self.cursor = cursor;
        }
    }

    fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<WindowEvent, PlatformError> {
        let attributes = Window::default_attributes()
            .with_title(self.desc.title.clone())
            .with_inner_size(LogicalSize::new(
                self.desc.width.get(),
                self.desc.height.get(),
            ))
            .with_resizable(self.desc.resizable)
            .with_fullscreen(
                self.desc
                    .fullscreen
                    .then_some(winit::window::Fullscreen::Borderless(None)),
            );
        let window = event_loop
            .create_window(attributes)
            .map_err(|e| PlatformError::Window(e.to_string()))?;
        let size = window.inner_size();
        let opened = WindowEvent::Opened {
            width: size.width,
            height: size.height,
            scale_factor: window.scale_factor(),
            view: native_view(&window),
        };
        self.window = Some(window);
        Ok(opened)
    }

    fn scale(&self) -> f64 {
        self.window.as_ref().map_or(1.0, Window::scale_factor)
    }
}

impl ApplicationHandler for Runner<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        match self.open(event_loop) {
            Ok(opened) => self.send(event_loop, opened),
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _: WindowId,
        event: winit::event::WindowEvent,
    ) {
        if let winit::event::WindowEvent::ModifiersChanged(m) = &event {
            let state = m.state();
            self.modifiers = Modifiers {
                shift: state.shift_key(),
                command: state.control_key() || state.super_key(),
            };
        }
        if let Some(event) = convert(&event, self.scale()) {
            self.send(event_loop, event);
        }
        if let Some(text) = typed(&event, self.modifiers) {
            self.send(event_loop, WindowEvent::Input(InputEvent::Text(text)));
        }
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            let motion = InputEvent::MouseMotion {
                dx: dx as f32,
                dy: dy as f32,
            };
            self.send(event_loop, WindowEvent::Input(motion));
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        match self.deadline {
            Some(deadline) if Instant::now() >= deadline => event_loop.exit(),
            Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

fn native_view(window: &Window) -> Option<NativeView> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::AppKit(handle) => Some(NativeView(handle.ns_view)),
        _ => None,
    }
}

fn convert(event: &winit::event::WindowEvent, scale: f64) -> Option<WindowEvent> {
    use winit::event::WindowEvent as W;
    match event {
        W::CloseRequested => Some(WindowEvent::CloseRequested),
        W::Focused(focused) => Some(WindowEvent::Focused(*focused)),
        W::Resized(size) => Some(WindowEvent::Resized {
            width: size.width,
            height: size.height,
        }),
        W::ScaleFactorChanged { scale_factor, .. } => Some(WindowEvent::ScaleChanged {
            scale_factor: *scale_factor,
        }),
        _ => input(event, scale).map(WindowEvent::Input),
    }
}

/// A pressed key's typed text (repeats included, so held editing keys repeat).
fn typed(event: &winit::event::WindowEvent, mods: Modifiers) -> Option<TextInput> {
    let winit::event::WindowEvent::KeyboardInput { event, .. } = event else {
        return None;
    };
    if event.state != ElementState::Pressed {
        return None;
    }
    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    let text = event.text.as_deref();
    match key_from_winit(code) {
        Some(key) => text_from_key(key, text, mods),
        None if !mods.command => insert(text?),
        None => None,
    }
}

fn input(event: &winit::event::WindowEvent, scale: f64) -> Option<InputEvent> {
    use winit::event::WindowEvent as W;
    match event {
        W::KeyboardInput { event, .. } if !event.repeat => match event.physical_key {
            PhysicalKey::Code(code) => Some(InputEvent::Key {
                key: key_from_winit(code)?,
                pressed: event.state == ElementState::Pressed,
            }),
            PhysicalKey::Unidentified(_) => None,
        },
        W::MouseInput { state, button, .. } => Some(InputEvent::MouseButton {
            button: mouse_button_from_winit(*button)?,
            pressed: *state == ElementState::Pressed,
        }),
        W::CursorMoved { position, .. } => {
            let logical = position.to_logical::<f32>(scale);
            Some(InputEvent::PointerMove {
                x: logical.x,
                y: logical.y,
            })
        }
        W::MouseWheel { delta, .. } => Some(match delta {
            MouseScrollDelta::LineDelta(dx, dy) => InputEvent::Scroll { dx: *dx, dy: *dy },
            MouseScrollDelta::PixelDelta(p) => {
                let logical = p.to_logical::<f32>(scale);
                InputEvent::Scroll {
                    dx: logical.x,
                    dy: logical.y,
                }
            }
        }),
        _ => None,
    }
}
