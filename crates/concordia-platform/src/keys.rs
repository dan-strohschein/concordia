//! winit → engine input vocabulary.

use concordia_input::{KeyCode, MouseButton};
use winit::event::MouseButton as WinitButton;
use winit::keyboard::KeyCode as K;

/// The engine key for a winit physical key (`None` for keys the engine does not name).
pub fn key_from_winit(key: K) -> Option<KeyCode> {
    letter(key)
        .or_else(|| digit(key))
        .or_else(|| function(key))
        .or_else(|| other(key))
}

/// The engine button for a winit mouse button.
pub fn mouse_button_from_winit(button: WinitButton) -> Option<MouseButton> {
    match button {
        WinitButton::Left => Some(MouseButton::Left),
        WinitButton::Right => Some(MouseButton::Right),
        WinitButton::Middle => Some(MouseButton::Middle),
        WinitButton::Back => Some(MouseButton::Back),
        WinitButton::Forward => Some(MouseButton::Forward),
        WinitButton::Other(_) => None,
    }
}

fn letter(key: K) -> Option<KeyCode> {
    use KeyCode as C;
    Some(match key {
        K::KeyA => C::A,
        K::KeyB => C::B,
        K::KeyC => C::C,
        K::KeyD => C::D,
        K::KeyE => C::E,
        K::KeyF => C::F,
        K::KeyG => C::G,
        K::KeyH => C::H,
        K::KeyI => C::I,
        K::KeyJ => C::J,
        K::KeyK => C::K,
        K::KeyL => C::L,
        K::KeyM => C::M,
        K::KeyN => C::N,
        K::KeyO => C::O,
        K::KeyP => C::P,
        K::KeyQ => C::Q,
        K::KeyR => C::R,
        K::KeyS => C::S,
        K::KeyT => C::T,
        K::KeyU => C::U,
        K::KeyV => C::V,
        K::KeyW => C::W,
        K::KeyX => C::X,
        K::KeyY => C::Y,
        K::KeyZ => C::Z,
        _ => return None,
    })
}

fn digit(key: K) -> Option<KeyCode> {
    use KeyCode as C;
    Some(match key {
        K::Digit0 => C::Digit0,
        K::Digit1 => C::Digit1,
        K::Digit2 => C::Digit2,
        K::Digit3 => C::Digit3,
        K::Digit4 => C::Digit4,
        K::Digit5 => C::Digit5,
        K::Digit6 => C::Digit6,
        K::Digit7 => C::Digit7,
        K::Digit8 => C::Digit8,
        K::Digit9 => C::Digit9,
        _ => return None,
    })
}

fn function(key: K) -> Option<KeyCode> {
    use KeyCode as C;
    Some(match key {
        K::F1 => C::F1,
        K::F2 => C::F2,
        K::F3 => C::F3,
        K::F4 => C::F4,
        K::F5 => C::F5,
        K::F6 => C::F6,
        K::F7 => C::F7,
        K::F8 => C::F8,
        K::F9 => C::F9,
        K::F10 => C::F10,
        K::F11 => C::F11,
        K::F12 => C::F12,
        _ => return None,
    })
}

fn other(key: K) -> Option<KeyCode> {
    use KeyCode as C;
    Some(match key {
        K::Space => C::Space,
        K::Enter => C::Enter,
        K::Escape => C::Escape,
        K::Tab => C::Tab,
        K::Backspace => C::Backspace,
        K::Delete => C::Delete,
        K::Minus => C::Minus,
        K::Equal => C::Equal,
        K::Comma => C::Comma,
        K::Period => C::Period,
        K::Slash => C::Slash,
        K::ArrowUp => C::ArrowUp,
        K::ArrowDown => C::ArrowDown,
        K::ArrowLeft => C::ArrowLeft,
        K::ArrowRight => C::ArrowRight,
        K::ShiftLeft => C::ShiftLeft,
        K::ShiftRight => C::ShiftRight,
        K::ControlLeft => C::ControlLeft,
        K::ControlRight => C::ControlRight,
        K::AltLeft => C::AltLeft,
        K::AltRight => C::AltRight,
        K::SuperLeft => C::SuperLeft,
        K::SuperRight => C::SuperRight,
        _ => return None,
    })
}
