//! Native viewport input: mouse gestures over the viewport become editor-camera moves in Rust,
//! so navigation never waits on the page. Godot-style, like the web UI: middle-drag orbits,
//! Shift+middle-drag pans, Alt+left-drag emulates the middle button, the wheel zooms. Plain left
//! clicks (selection, gizmos) stay with the page.

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Other,
}

/// A mouse event in window coordinates (AppKit: y grows upward).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    Down {
        button: Button,
        x: f64,
        y: f64,
        alt: bool,
        shift: bool,
    },
    Drag {
        x: f64,
        y: f64,
    },
    Up,
    /// Scroll with the content-moves-up delta (`scrollingDeltaY`).
    Scroll {
        dy: f64,
    },
}

/// A camera move, in screen pixels with y down (as `viewport.navigate` takes it).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Nav {
    pub orbit: Option<[f32; 2]>,
    pub pan: Option<[f32; 2]>,
    pub zoom: Option<f32>,
}

impl Nav {
    /// Both moves as one.
    pub fn merge(self, other: Nav) -> Nav {
        let pair = |a: Option<[f32; 2]>, b: Option<[f32; 2]>| match (a, b) {
            (Some(a), Some(b)) => Some([a[0] + b[0], a[1] + b[1]]),
            (a, b) => a.or(b),
        };
        Nav {
            orbit: pair(self.orbit, other.orbit),
            pan: pair(self.pan, other.pan),
            zoom: match (self.zoom, other.zoom) {
                (Some(a), Some(b)) => Some(a + b),
                (a, b) => a.or(b),
            },
        }
    }

    /// Whether it moves the camera at all.
    pub fn is_move(&self) -> bool {
        self.orbit.is_some() || self.pan.is_some() || self.zoom.is_some()
    }
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    x: f64,
    y: f64,
    pan: bool,
}

/// Tracks one navigation drag at a time.
#[derive(Debug, Default)]
pub struct Navigator {
    drag: Option<Drag>,
}

impl Navigator {
    /// Handles an event; `in_slot` says whether it is over the viewport. Returns the camera move
    /// (empty for presses and releases) when the viewport consumes the event, `None` when the page
    /// should get it.
    pub fn handle(&mut self, input: Input, in_slot: bool) -> Option<Nav> {
        match input {
            Input::Down {
                button,
                x,
                y,
                alt,
                shift,
            } => {
                let navigates = button == Button::Middle || (button == Button::Left && alt);
                if !(in_slot && navigates) {
                    return None;
                }
                self.drag = Some(Drag { x, y, pan: shift });
                Some(Nav::default())
            }
            Input::Drag { x, y } => {
                let drag = self.drag.as_mut()?;
                let delta = [(x - drag.x) as f32, (drag.y - y) as f32];
                (drag.x, drag.y) = (x, y);
                Some(match drag.pan {
                    true => Nav {
                        pan: Some(delta),
                        ..Nav::default()
                    },
                    false => Nav {
                        orbit: Some(delta),
                        ..Nav::default()
                    },
                })
            }
            Input::Up => self.drag.take().map(|_| Nav::default()),
            Input::Scroll { dy } => {
                if !in_slot || dy == 0.0 || !dy.is_finite() {
                    return None;
                }
                // Scrolling the content up (dy > 0) is a wheel turn away: zoom in.
                Some(Nav {
                    zoom: Some(if dy > 0.0 { -1.0 } else { 1.0 }),
                    ..Nav::default()
                })
            }
        }
    }
}
