//! Native viewport input: mouse gestures over the viewport become editor-camera moves and gizmo
//! drags in Rust, so they never wait on the page. Godot-style, like the web UI: middle-drag
//! orbits, Shift+middle-drag pans, Alt+left-drag emulates the middle button, the wheel zooms, and
//! left-dragging a gizmo handle moves the entity. Other left clicks (selection) stay with the
//! page. Positions are viewport pixels, y down.

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Other,
}

/// A mouse event, in viewport pixels (y down).
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

/// A gizmo handle the page drew (viewport pixels).
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct Handle {
    pub axis: String,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
}

/// A step of a native gizmo drag (`viewport.gizmo`).
#[derive(Debug, Clone, PartialEq)]
pub enum GizmoStep {
    Begin {
        entity: String,
        axis: String,
        x: f64,
        y: f64,
    },
    Move {
        x: f64,
        y: f64,
    },
    End,
}

/// What a consumed event does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Nav(Nav),
    Gizmo(GizmoStep),
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    Orbit,
    Pan,
    Gizmo,
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    x: f64,
    y: f64,
    mode: Mode,
}

/// Tracks one drag at a time, and the selected entity's gizmo handles.
#[derive(Debug, Default)]
pub struct Navigator {
    drag: Option<Drag>,
    gizmo: Option<(String, Vec<Handle>)>,
}

impl Navigator {
    /// The gizmo handles the page drew for `entity` (empty when there is no gizmo).
    pub fn set_gizmo(&mut self, entity: &str, handles: Vec<Handle>) {
        self.gizmo = (!handles.is_empty()).then(|| (entity.to_owned(), handles));
    }

    /// Handles an event; `in_slot` says whether it is over the viewport. Returns what it does when
    /// the viewport consumes it, `None` when the page should get it.
    pub fn handle(&mut self, input: Input, in_slot: bool) -> Option<Action> {
        match input {
            Input::Down {
                button,
                x,
                y,
                alt,
                shift,
            } => self.press(button, x, y, alt, shift, in_slot),
            Input::Drag { x, y } => {
                let drag = self.drag.as_mut()?;
                let delta = [(x - drag.x) as f32, (y - drag.y) as f32];
                (drag.x, drag.y) = (x, y);
                Some(match drag.mode {
                    Mode::Orbit => Action::Nav(Nav {
                        orbit: Some(delta),
                        ..Nav::default()
                    }),
                    Mode::Pan => Action::Nav(Nav {
                        pan: Some(delta),
                        ..Nav::default()
                    }),
                    Mode::Gizmo => Action::Gizmo(GizmoStep::Move { x, y }),
                })
            }
            Input::Up => self.drag.take().map(|d| match d.mode {
                Mode::Gizmo => Action::Gizmo(GizmoStep::End),
                _ => Action::Nav(Nav::default()),
            }),
            Input::Scroll { dy } => {
                if !in_slot || dy == 0.0 || !dy.is_finite() {
                    return None;
                }
                // Scrolling the content up (dy > 0) is a wheel turn away: zoom in.
                Some(Action::Nav(Nav {
                    zoom: Some(if dy > 0.0 { -1.0 } else { 1.0 }),
                    ..Nav::default()
                }))
            }
        }
    }

    fn press(
        &mut self,
        button: Button,
        x: f64,
        y: f64,
        alt: bool,
        shift: bool,
        in_slot: bool,
    ) -> Option<Action> {
        if !in_slot {
            return None;
        }
        if button == Button::Middle || (button == Button::Left && alt) {
            let mode = if shift { Mode::Pan } else { Mode::Orbit };
            self.drag = Some(Drag { x, y, mode });
            return Some(Action::Nav(Nav::default()));
        }
        if button != Button::Left {
            return None;
        }
        let (entity, handles) = self.gizmo.as_ref()?;
        let hit = handles
            .iter()
            .find(|h| (h.x - x).hypot(h.y - y) <= h.radius)?;
        self.drag = Some(Drag {
            x,
            y,
            mode: Mode::Gizmo,
        });
        Some(Action::Gizmo(GizmoStep::Begin {
            entity: entity.clone(),
            axis: hit.axis.clone(),
            x,
            y,
        }))
    }
}
