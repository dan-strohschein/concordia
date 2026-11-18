//! Device vocabulary: keys, mouse buttons and modifiers (engine-owned, not winit's).

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// A physical key, named by its US-layout position.
#[allow(missing_docs)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Minus,
    Equal,
    Comma,
    Period,
    Slash,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    SuperLeft,
    SuperRight,
}

impl KeyCode {
    /// The modifier this key acts as, if any.
    pub fn modifier(self) -> Option<Modifier> {
        match self {
            Self::ShiftLeft | Self::ShiftRight => Some(Modifier::Shift),
            Self::ControlLeft | Self::ControlRight => Some(Modifier::Control),
            Self::AltLeft | Self::AltRight => Some(Modifier::Alt),
            Self::SuperLeft | Self::SuperRight => Some(Modifier::Super),
            _ => None,
        }
    }
}

/// A mouse button.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
pub enum MouseButton {
    /// Primary button.
    Left,
    /// Secondary button.
    Right,
    /// Wheel button.
    Middle,
    /// Back side button.
    Back,
    /// Forward side button.
    Forward,
}

/// A modifier, satisfied by either side's key.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
pub enum Modifier {
    /// Either Shift.
    Shift,
    /// Either Control.
    Control,
    /// Either Alt / Option.
    Alt,
    /// Either Super / Command.
    Super,
}
