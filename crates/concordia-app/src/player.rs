//! Windowed play (M2 phase 2.5b.2b, ADR-0078): turns a window's events into Engine API calls, so
//! `concordia run` plays the scene through the same API agents use.

use std::collections::BTreeSet;

use concordia_api::{ApiError, ApiErrorKind};
use concordia_input::{EditKey, InputEvent, KeyCode, MAX_INSERT_BYTES, MouseButton, TextInput};
use concordia_platform::{CursorIcon, WindowEvent};

use crate::clipboard::{Clipboard, OsClipboard};
use serde_json::{Value, json};

/// One Engine API call: a method and its params.
pub type ApiCall<'a> = dyn FnMut(&str, Value) -> Result<Value, ApiError> + 'a;

/// Plays the scene in a window: starts the simulation, sends input (cursor moves also as world
/// rays from the drawing camera) and presents the play world from its game camera.
pub struct Player {
    draws: bool,
    size: (u32, u32),
    scale: f64,
    keys: BTreeSet<KeyCode>,
    buttons: BTreeSet<MouseButton>,
    warned: BTreeSet<&'static str>,
    cursor: CursorIcon,
    has_ui: bool,
    clipboard: Box<dyn Clipboard>,
    copied: u64,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("draws", &self.draws)
            .field("size", &self.size)
            .field("scale", &self.scale)
            .field("cursor", &self.cursor)
            .finish_non_exhaustive()
    }
}

impl Player {
    /// A player; `draws` says whether a viewport surface is attached to present into. It uses
    /// the OS clipboard.
    pub fn new(draws: bool) -> Self {
        Self::with_clipboard(draws, Box::new(OsClipboard::default()))
    }

    /// A player with its own clipboard (tests, or no OS clipboard).
    pub fn with_clipboard(draws: bool, clipboard: Box<dyn Clipboard>) -> Self {
        Self {
            draws,
            size: (1, 1),
            scale: 1.0,
            keys: BTreeSet::new(),
            buttons: BTreeSet::new(),
            warned: BTreeSet::new(),
            cursor: CursorIcon::Arrow,
            has_ui: true,
            clipboard,
            copied: 0,
        }
    }

    /// The cursor the widget under the pointer asks for.
    pub fn cursor(&self) -> CursorIcon {
        self.cursor
    }

    /// Handles one window event through `api`. Drawing and ray failures are warned about once
    /// and play goes on; other failures are returned.
    pub fn event(&mut self, event: &WindowEvent, api: &mut ApiCall<'_>) -> Result<(), ApiError> {
        match event {
            WindowEvent::Opened {
                width,
                height,
                scale_factor,
                ..
            } => {
                self.resize(*width, *height);
                self.rescale(*scale_factor);
                api("sim.play", json!({}))?;
                self.viewport(api)?;
                self.present(api);
            }
            WindowEvent::Resized { width, height } => {
                self.resize(*width, *height);
                self.viewport(api)?;
                self.present(api);
            }
            WindowEvent::ScaleChanged { scale_factor } => {
                self.rescale(*scale_factor);
                self.viewport(api)?;
            }
            WindowEvent::Focused(false) => self.release_all(api)?,
            WindowEvent::Focused(true) => {}
            WindowEvent::CloseRequested => {
                api("sim.stop", json!({}))?;
            }
            WindowEvent::Input(input) => {
                self.input(input, api)?;
                self.sync_clipboard(api);
            }
        }
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.size = (width.max(1), height.max(1));
    }

    fn rescale(&mut self, scale: f64) {
        if scale.is_finite() && scale > 0.0 {
            self.scale = scale;
        }
    }

    /// The window in logical pixels (the unit of cursor positions).
    fn logical(&self) -> (u32, u32) {
        let at = |physical: u32| ((f64::from(physical) / self.scale).round() as u32).max(1);
        (at(self.size.0), at(self.size.1))
    }

    /// Tells the simulation (the UI) the window's size in logical pixels.
    fn viewport(&self, api: &mut ApiCall<'_>) -> Result<(), ApiError> {
        let at = |physical: u32| (f64::from(physical) / self.scale) as f32;
        let (width, height) = (at(self.size.0), at(self.size.1));
        inject(
            api,
            vec![json!({"Viewport": {"width": width, "height": height}})],
        )
    }

    /// Asks the UI which cursor the widget at (x, y) wants (projects without `[ui]` are asked
    /// once).
    fn hover(&mut self, x: f32, y: f32, api: &mut ApiCall<'_>) {
        if !self.has_ui {
            return;
        }
        match api("ui.hit_test", json!({"x": x, "y": y, "world": "play"})) {
            Ok(hit) => {
                self.cursor = match hit["cursor"].as_str() {
                    Some("Pointer") => CursorIcon::Pointer,
                    Some("Text") => CursorIcon::Text,
                    _ => CursorIcon::Arrow,
                }
            }
            Err(e) if e.kind == ApiErrorKind::MethodNotFound => {
                self.has_ui = false;
                self.cursor = CursorIcon::Arrow;
            }
            Err(e) => self.warn_once("cursor", &e),
        }
    }

    /// A paste becomes the OS clipboard's text as typed text (the engine's own paste when the
    /// OS has none).
    fn paste(&mut self, input: &InputEvent) -> InputEvent {
        let InputEvent::Text(TextInput::Edit {
            key: EditKey::Paste,
            ..
        }) = input
        else {
            return input.clone();
        };
        let typed = self.clipboard.get().map(|text| {
            let kept: String = text
                .chars()
                .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                .collect();
            crate::clipboard::cut(&kept, MAX_INSERT_BYTES).to_owned()
        });
        match typed.filter(|t| !t.is_empty()) {
            Some(text) => InputEvent::Text(TextInput::Insert(text)),
            None => input.clone(),
        }
    }

    /// Writes the UI's latest copy to the OS clipboard (once per `ui.clipboard` generation).
    fn sync_clipboard(&mut self, api: &mut ApiCall<'_>) {
        if !self.has_ui {
            return;
        }
        match api("ui.clipboard", json!({})) {
            Ok(clip) => {
                let generation = clip["generation"].as_u64().unwrap_or(0);
                if generation > self.copied {
                    self.copied = generation;
                    if let Some(text) = clip["text"].as_str() {
                        self.clipboard.set(text);
                    }
                }
            }
            Err(e) if e.kind == ApiErrorKind::MethodNotFound => self.has_ui = false,
            Err(e) => self.warn_once("clipboard", &e),
        }
    }

    /// Presents the play world from its game camera (later ticks redraw it by themselves).
    fn present(&mut self, api: &mut ApiCall<'_>) {
        if !self.draws {
            return;
        }
        let (width, height) = self.size;
        let params =
            json!({"width": width, "height": height, "world": "play", "scale": self.scale});
        if let Err(e) = api("viewport.present", params) {
            self.warn_once("present", &e);
        }
    }

    /// Sends one input event (plus its ray, for a cursor move) for the next tick.
    fn input(&mut self, input: &InputEvent, api: &mut ApiCall<'_>) -> Result<(), ApiError> {
        self.track(input);
        let input = &self.paste(input);
        let mut events = vec![serde_json::to_value(input).unwrap_or(Value::Null)];
        if let InputEvent::PointerMove { x, y } = input {
            if let Some(ray) = self.ray(*x, *y, api) {
                events.push(ray);
            }
            self.hover(*x, *y, api);
        }
        inject(api, events)
    }

    /// Remembers held keys and buttons, so losing focus can release them.
    fn track(&mut self, input: &InputEvent) {
        match *input {
            InputEvent::Key { key, pressed } => toggle(&mut self.keys, key, pressed),
            InputEvent::MouseButton { button, pressed } => {
                toggle(&mut self.buttons, button, pressed)
            }
            _ => {}
        }
    }

    /// The pointer's world ray from the drawing camera, as a `PointerRay` event.
    fn ray(&mut self, x: f32, y: f32, api: &mut ApiCall<'_>) -> Option<Value> {
        let (width, height) = self.logical();
        let (x, y) = (x.clamp(0.0, width as f32), y.clamp(0.0, height as f32));
        let params = json!({"x": x, "y": y, "width": width, "height": height, "world": "play"});
        match api("viewport.pointer_ray", params) {
            Ok(ray) => Some(
                json!({"PointerRay": {"origin": ray["origin"], "direction": ray["direction"]}}),
            ),
            Err(e) => {
                self.warn_once("ray", &e);
                None
            }
        }
    }

    /// Releases every held key and button (the window lost focus; it won't see their key-ups).
    fn release_all(&mut self, api: &mut ApiCall<'_>) -> Result<(), ApiError> {
        let keys = std::mem::take(&mut self.keys)
            .into_iter()
            .map(|key| InputEvent::Key {
                key,
                pressed: false,
            });
        let buttons =
            std::mem::take(&mut self.buttons)
                .into_iter()
                .map(|button| InputEvent::MouseButton {
                    button,
                    pressed: false,
                });
        let events: Vec<Value> = keys
            .chain(buttons)
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect();
        match events.is_empty() {
            true => Ok(()),
            false => inject(api, events),
        }
    }

    fn warn_once(&mut self, what: &'static str, error: &ApiError) {
        if self.warned.insert(what) {
            tracing::warn!(target: "concordia::player", what, error = %error.message, "windowed play goes on without it");
        }
    }
}

/// `input.inject` of `events` for the next tick.
fn inject(api: &mut ApiCall<'_>, events: Vec<Value>) -> Result<(), ApiError> {
    let events: Vec<Value> = events.into_iter().map(|e| json!({"event": e})).collect();
    api(
        "input.inject",
        json!({"events": events, "actor": "human:player"}),
    )
    .map(|_| ())
}

fn toggle<T: Ord>(set: &mut BTreeSet<T>, item: T, on: bool) {
    match on {
        true => set.insert(item),
        false => set.remove(&item),
    };
}
