//! Native viewport input (M1 phase 1.9d): mouse gestures over the viewport become camera moves
//! in Rust, matching the web UI's Godot-style navigation.

use concordia_editor::input::{Button, Input, Nav, Navigator};

fn down(button: Button, alt: bool, shift: bool) -> Input {
    Input::Down {
        button,
        x: 100.0,
        y: 100.0,
        alt,
        shift,
    }
}

#[test]
fn middle_drag_orbits_by_the_pointer_delta_with_screen_down_positive() {
    let mut n = Navigator::default();
    assert_eq!(
        n.handle(down(Button::Middle, false, false), true),
        Some(Nav::default())
    );
    // AppKit window y grows upward: moving the pointer down lowers y.
    let nav = n.handle(Input::Drag { x: 110.0, y: 95.0 }, true);
    assert_eq!(
        nav,
        Some(Nav {
            orbit: Some([10.0, 5.0]),
            ..Nav::default()
        })
    );
    assert_eq!(n.handle(Input::Up, true), Some(Nav::default()));
    assert_eq!(
        n.handle(Input::Drag { x: 120.0, y: 95.0 }, true),
        None,
        "no drag after release"
    );
}

#[test]
fn shift_pans_and_alt_left_emulates_the_middle_button() {
    let mut n = Navigator::default();
    n.handle(down(Button::Left, true, true), true);
    let nav = n.handle(Input::Drag { x: 104.0, y: 100.0 }, true);
    assert_eq!(
        nav,
        Some(Nav {
            pan: Some([4.0, 0.0]),
            ..Nav::default()
        })
    );
}

#[test]
fn plain_left_clicks_and_presses_outside_the_viewport_go_to_the_page() {
    let mut n = Navigator::default();
    assert_eq!(
        n.handle(down(Button::Left, false, false), true),
        None,
        "selection and gizmos stay in the web UI"
    );
    assert_eq!(n.handle(Input::Drag { x: 150.0, y: 100.0 }, true), None);
    assert_eq!(
        n.handle(down(Button::Middle, false, false), false),
        None,
        "outside the slot"
    );
}

#[test]
fn a_drag_that_leaves_the_viewport_keeps_steering() {
    let mut n = Navigator::default();
    n.handle(down(Button::Middle, false, false), true);
    assert!(
        n.handle(Input::Drag { x: 900.0, y: 100.0 }, false)
            .is_some()
    );
}

#[test]
fn the_wheel_zooms_one_step_per_event_in_the_viewport() {
    let mut n = Navigator::default();
    assert_eq!(
        n.handle(Input::Scroll { dy: 3.5 }, true),
        Some(Nav {
            zoom: Some(-1.0),
            ..Nav::default()
        })
    );
    assert_eq!(
        n.handle(Input::Scroll { dy: -0.2 }, true),
        Some(Nav {
            zoom: Some(1.0),
            ..Nav::default()
        })
    );
    assert_eq!(n.handle(Input::Scroll { dy: 0.0 }, true), None);
    assert_eq!(n.handle(Input::Scroll { dy: 1.0 }, false), None);
}

#[test]
fn queued_moves_merge_into_one() {
    let a = Nav {
        orbit: Some([1.0, 2.0]),
        ..Nav::default()
    };
    let b = Nav {
        orbit: Some([3.0, -1.0]),
        zoom: Some(1.0),
        ..Nav::default()
    };
    assert_eq!(
        a.merge(b),
        Nav {
            orbit: Some([4.0, 1.0]),
            zoom: Some(1.0),
            ..Nav::default()
        }
    );
}
