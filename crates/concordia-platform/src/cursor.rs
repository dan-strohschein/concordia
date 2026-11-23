//! The pointer's look over the window, as the in-game UI asks for it.

/// A cursor shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorIcon {
    /// The arrow.
    #[default]
    Arrow,
    /// A pointing hand (buttons and other clickables).
    Pointer,
    /// A text beam.
    Text,
}

/// winit's cursor for `icon`.
pub fn cursor_to_winit(icon: CursorIcon) -> winit::window::CursorIcon {
    match icon {
        CursorIcon::Arrow => winit::window::CursorIcon::Default,
        CursorIcon::Pointer => winit::window::CursorIcon::Pointer,
        CursorIcon::Text => winit::window::CursorIcon::Text,
    }
}
