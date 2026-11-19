//! Native viewport input (M1 phase 1.9d/e): mouse gestures over the viewport become camera moves
//! and gizmo drags in Rust, matching the web UI's Godot-style navigation. Positions are viewport
//! pixels with y down (like the page's).

use concordia_editor::input::{Action, Button, GizmoStep, Handle, Input, Nav, Navigator};

fn down(button: Button, x: f64, y: f64, alt: bool, shift: bool) -> Input {
    Input::Down {
        button,
        x,
        y,
        alt,
        shift,
    }
}

fn nav(n: Nav) -> Option<Action> {
    Some(Action::Nav(n))
}

#[test]
fn middle_drag_orbits_by_the_pointer_delta() {
    let mut n = Navigator::default();
    assert_eq!(
        n.handle(down(Button::Middle, 100.0, 100.0, false, false), true),
        nav(Nav::default())
    );
    let moved = n.handle(Input::Drag { x: 110.0, y: 105.0 }, true);
    assert_eq!(
        moved,
        nav(Nav {
            orbit: Some([10.0, 5.0]),
            ..Nav::default()
        })
    );
    assert_eq!(n.handle(Input::Up, true), nav(Nav::default()));
    assert_eq!(
        n.handle(Input::Drag { x: 120.0, y: 95.0 }, true),
        None,
        "no drag after release"
    );
}

#[test]
fn shift_pans_and_alt_left_emulates_the_middle_button() {
    let mut n = Navigator::default();
    n.handle(down(Button::Left, 100.0, 100.0, true, true), true);
    let moved = n.handle(Input::Drag { x: 104.0, y: 100.0 }, true);
    assert_eq!(
        moved,
        nav(Nav {
            pan: Some([4.0, 0.0]),
            ..Nav::default()
        })
    );
}

#[test]
fn plain_left_clicks_and_presses_outside_the_viewport_go_to_the_page() {
    let mut n = Navigator::default();
    assert_eq!(
        n.handle(down(Button::Left, 100.0, 100.0, false, false), true),
        None,
        "selection stays in the web UI"
    );
    assert_eq!(n.handle(Input::Drag { x: 150.0, y: 100.0 }, true), None);
    assert_eq!(
        n.handle(down(Button::Middle, 100.0, 100.0, false, false), false),
        None,
        "outside the slot"
    );
}

#[test]
fn a_drag_that_leaves_the_viewport_keeps_steering() {
    let mut n = Navigator::default();
    n.handle(down(Button::Middle, 100.0, 100.0, false, false), true);
    assert!(
        n.handle(Input::Drag { x: 900.0, y: 100.0 }, false)
            .is_some()
    );
}

#[test]
fn the_wheel_zooms_one_step_per_event_in_the_viewport() {
    let mut n = Navigator::default();
    let zoom = |z| {
        nav(Nav {
            zoom: Some(z),
            ..Nav::default()
        })
    };
    assert_eq!(n.handle(Input::Scroll { dy: 3.5 }, true), zoom(-1.0));
    assert_eq!(n.handle(Input::Scroll { dy: -0.2 }, true), zoom(1.0));
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

#[test]
fn pressing_a_gizmo_handle_drags_it_natively() {
    let mut n = Navigator::default();
    n.set_gizmo(
        "e1",
        vec![Handle {
            axis: "x".into(),
            x: 160.0,
            y: 100.0,
            radius: 10.0,
        }],
    );
    let begin = n.handle(down(Button::Left, 163.0, 98.0, false, false), true);
    assert_eq!(
        begin,
        Some(Action::Gizmo(GizmoStep::Begin {
            entity: "e1".into(),
            axis: "x".into(),
            x: 163.0,
            y: 98.0
        }))
    );
    assert_eq!(
        n.handle(Input::Drag { x: 180.0, y: 98.0 }, true),
        Some(Action::Gizmo(GizmoStep::Move { x: 180.0, y: 98.0 }))
    );
    assert_eq!(
        n.handle(Input::Up, true),
        Some(Action::Gizmo(GizmoStep::End))
    );
    assert_eq!(
        n.handle(down(Button::Left, 100.0, 100.0, false, false), true),
        None,
        "a miss is a page click"
    );
    n.set_gizmo("e1", Vec::new());
    assert_eq!(
        n.handle(down(Button::Left, 160.0, 100.0, false, false), true),
        None,
        "no gizmo, no drag"
    );
}
