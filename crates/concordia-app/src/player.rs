//! Windowed play (M2 phase 2.5b.2b, ADR-0078): turns a window's events into Engine API calls, so
//! `concordia run` plays the scene through the same API agents use.

use std::collections::BTreeSet;

use concordia_api::ApiError;
use concordia_input::{InputEvent, KeyCode, MouseButton};
use concordia_platform::WindowEvent;
use serde_json::{Value, json};

/// One Engine API call: a method and its params.
pub type ApiCall<'a> = dyn FnMut(&str, Value) -> Result<Value, ApiError> + 'a;

/// Plays the scene in a window: starts the simulation, sends input (cursor moves also as world
/// rays from the drawing camera) and presents the play world from its game camera.
#[derive(Debug)]
pub struct Player {
    draws: bool,
    size: (u32, u32),
    scale: f64,
    keys: BTreeSet<KeyCode>,
    buttons: BTreeSet<MouseButton>,
    warned: BTreeSet<&'static str>,
}

impl Player {
    /// A player; `draws` says whether a viewport surface is attached to present into.
    pub fn new(draws: bool) -> Self {
        Self {
            draws,
            size: (1, 1),
            scale: 1.0,
            keys: BTreeSet::new(),
            buttons: BTreeSet::new(),
            warned: BTreeSet::new(),
        }
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
                self.present(api);
            }
            WindowEvent::Resized { width, height } => {
                self.resize(*width, *height);
                self.present(api);
            }
            WindowEvent::ScaleChanged { scale_factor } => self.rescale(*scale_factor),
            WindowEvent::Focused(false) => self.release_all(api)?,
            WindowEvent::Focused(true) => {}
            WindowEvent::CloseRequested => {
                api("sim.stop", json!({}))?;
            }
            WindowEvent::Input(input) => self.input(input, api)?,
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
        let mut events = vec![serde_json::to_value(input).unwrap_or(Value::Null)];
        if let InputEvent::PointerMove { x, y } = input
            && let Some(ray) = self.ray(*x, *y, api)
        {
            events.push(ray);
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
