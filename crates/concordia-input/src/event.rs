//! Input events: device events and direct action values.

use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::device::{KeyCode, MouseButton};
use crate::text::TextInput;
use crate::value::ActionValue;

/// One input event, from the OS or injected through `input.inject`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum InputEvent {
    /// A key changed.
    Key {
        /// The key.
        key: KeyCode,
        /// Down (true) or up.
        pressed: bool,
    },
    /// A mouse button changed.
    MouseButton {
        /// The button.
        button: MouseButton,
        /// Down (true) or up.
        pressed: bool,
    },
    /// The pointer moved to a position in logical pixels (origin top-left of the viewport).
    PointerMove {
        /// X in logical pixels.
        x: f32,
        /// Y in logical pixels.
        y: f32,
    },
    /// The pointer as a world-space ray, from the camera the pointer is over.
    PointerRay {
        /// Start point.
        origin: Vec3,
        /// Direction (any non-zero length).
        direction: Vec3,
    },
    /// Relative mouse motion.
    MouseMotion {
        /// Horizontal motion.
        dx: f32,
        /// Vertical motion.
        dy: f32,
    },
    /// Scroll wheel or trackpad scroll.
    Scroll {
        /// Horizontal scroll.
        dx: f32,
        /// Vertical scroll.
        dy: f32,
    },
    /// The viewport's size in logical pixels (window open, resize, scale change); it holds
    /// until the next.
    Viewport {
        /// Width in logical pixels.
        width: f32,
        /// Height in logical pixels.
        height: f32,
    },
    /// Typed text or an editing command (text fields).
    Text(TextInput),
    /// Set an action's value directly, bypassing bindings (it holds until set again).
    Action {
        /// Action name.
        action: String,
        /// New value (must match the action's kind).
        value: ActionValue,
    },
}
