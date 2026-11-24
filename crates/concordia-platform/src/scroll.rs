//! Mouse-wheel and trackpad scrolling, in lines.

use concordia_input::InputEvent;
use winit::event::MouseScrollDelta;

/// Logical pixels one scroll line stands for when a trackpad reports pixels.
pub const SCROLL_LINE_PX: f32 = 20.0;

/// The engine's `Scroll` event (in lines) for a winit scroll delta at a window scale factor.
pub fn scroll_from_winit(delta: MouseScrollDelta, scale: f64) -> InputEvent {
    match delta {
        MouseScrollDelta::LineDelta(dx, dy) => InputEvent::Scroll { dx, dy },
        MouseScrollDelta::PixelDelta(p) => {
            let logical = p.to_logical::<f32>(scale);
            InputEvent::Scroll {
                dx: logical.x / SCROLL_LINE_PX,
                dy: logical.y / SCROLL_LINE_PX,
            }
        }
    }
}
