//! Scrolling reaches the engine in lines, from a mouse wheel or a trackpad (M3 phase 3.0b).

use concordia_input::InputEvent;
use concordia_platform::{SCROLL_LINE_PX, scroll_from_winit};
use winit::dpi::PhysicalPosition;
use winit::event::MouseScrollDelta;

/// A wheel's lines pass through unchanged: (1.5, −3) lines stay (1.5, −3).
#[test]
fn line_scroll_passes_through() {
    let event = scroll_from_winit(MouseScrollDelta::LineDelta(1.5, -3.0), 2.0);
    assert_eq!(event, InputEvent::Scroll { dx: 1.5, dy: -3.0 });
}

/// A trackpad's pixels become lines: at scale 2, (−20, 80) physical px are (−10, 40) logical px,
/// which at 20 logical px per line is (−0.5, 2) lines.
#[test]
fn pixel_scroll_arrives_in_lines() {
    assert_eq!(SCROLL_LINE_PX, 20.0);
    let delta = MouseScrollDelta::PixelDelta(PhysicalPosition::new(-20.0, 80.0));
    assert_eq!(
        scroll_from_winit(delta, 2.0),
        InputEvent::Scroll { dx: -0.5, dy: 2.0 }
    );
    // At scale 1, 30 px is 1.5 lines.
    let delta = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 30.0));
    assert_eq!(
        scroll_from_winit(delta, 1.0),
        InputEvent::Scroll { dx: 0.0, dy: 1.5 }
    );
}
