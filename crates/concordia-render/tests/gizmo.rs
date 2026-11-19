//! Gizmo drag math, engine-side so native input and the page share it (M1 phase 1.9e; ported from
//! the editor UI's camera and gizmo tests).

use concordia_core::math::Vec3;
use concordia_render::{OrbitCamera, Projection, View, axis_delta};

fn front() -> View {
    OrbitCamera {
        target: Vec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        distance: 10.0,
        ortho: false,
    }
    .view()
}

#[test]
fn project_maps_the_target_to_the_image_center_and_plus_x_to_the_right() {
    let view = front();
    let [x, y] = view.project(Vec3::ZERO, 400.0, 300.0).unwrap();
    assert!((x - 200.0).abs() < 1e-3 && (y - 150.0).abs() < 1e-3);
    assert!(view.project(Vec3::X, 400.0, 300.0).unwrap()[0] > 200.0);
    assert!(
        view.project(Vec3::new(0.0, 0.0, 20.0), 400.0, 300.0)
            .is_none(),
        "behind the eye"
    );
    let ortho = View {
        projection: Projection::Orthographic {
            height: 10.0,
            near: 0.0,
            far: 100.0,
        },
        ..front()
    };
    assert!(ortho.project(Vec3::ZERO, 400.0, 300.0).is_some());
}

#[test]
fn gizmo_drags_convert_pointer_motion_along_the_axis_into_world_units() {
    let view = front();
    let per_unit = view.project(Vec3::X, 400.0, 300.0).unwrap()[0] - 200.0;
    let along = axis_delta(
        &view,
        Vec3::ZERO,
        Vec3::X,
        [per_unit * 2.0, 0.0],
        400.0,
        300.0,
    );
    assert!((along - 2.0).abs() < 1e-3, "{along}");
    assert!(axis_delta(&view, Vec3::ZERO, Vec3::X, [0.0, 50.0], 400.0, 300.0).abs() < 1e-4);
    assert_eq!(
        axis_delta(&view, Vec3::ZERO, Vec3::Z, [30.0, 30.0], 400.0, 300.0),
        0.0,
        "an axis pointing at the eye does not move"
    );
}

#[test]
fn gizmo_handles_point_along_each_axis_at_a_constant_screen_length() {
    use concordia_render::{GizmoAxis, gizmo_handles};
    let view = front();
    let handles = gizmo_handles(&view, Vec3::ZERO, 400.0, 300.0);
    let axes: Vec<_> = handles.iter().map(|h| h.axis).collect();
    assert_eq!(
        axes,
        [GizmoAxis::x, GizmoAxis::y],
        "z points at the eye: no handle"
    );
    let x = &handles[0];
    assert!((x.origin[0] - 200.0).abs() < 1e-3);
    assert!((x.end[0] - 260.0).abs() < 1e-3, "60 px long: {:?}", x.end);
    let far = OrbitCamera {
        target: Vec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        distance: 80.0,
        ortho: false,
    }
    .view();
    let len =
        |h: &concordia_render::GizmoHandle| (h.end[0] - h.origin[0]).hypot(h.end[1] - h.origin[1]);
    assert!(
        (len(&gizmo_handles(&far, Vec3::ZERO, 400.0, 300.0)[0]) - 60.0).abs() < 1e-3,
        "zoom does not change it"
    );
}

#[test]
fn gizmo_hit_test_finds_the_handle_under_the_pointer() {
    use concordia_render::{GizmoAxis, gizmo_handles, gizmo_hit};
    let handles = gizmo_handles(&front(), Vec3::ZERO, 400.0, 300.0);
    assert_eq!(
        gizmo_hit(&handles, 258.0, 151.0),
        Some(GizmoAxis::x),
        "near the x arrow's tip"
    );
    assert_eq!(
        gizmo_hit(&handles, 200.0, 95.0),
        Some(GizmoAxis::y),
        "the y arrow points up"
    );
    assert_eq!(
        gizmo_hit(&handles, 230.0, 150.0),
        Some(GizmoAxis::x),
        "along the shaft"
    );
    assert_eq!(gizmo_hit(&handles, 330.0, 250.0), None);
}

#[test]
fn gizmo_rings_circle_the_entity_at_a_constant_screen_radius_and_hit_on_the_ring() {
    use concordia_render::{GizmoAxis, gizmo_rings, ring_hit};
    let rings = gizmo_rings(&front(), Vec3::ZERO, 400.0, 300.0);
    let z = rings
        .iter()
        .find(|r| r.axis == GizmoAxis::z)
        .expect("z ring faces the eye");
    for p in &z.points {
        let r = (p[0] - 200.0).hypot(p[1] - 150.0);
        assert!((r - 60.0).abs() < 0.5, "radius {r}");
    }
    // At 45° the z ring is alone (at ±x it crosses the edge-on y ring).
    let d = 60.0 * std::f32::consts::FRAC_1_SQRT_2;
    assert_eq!(
        ring_hit(&rings, 200.0 + d, 150.0 - d),
        Some(GizmoAxis::z),
        "on the z ring"
    );
    assert_eq!(
        ring_hit(&rings, 200.0, 120.0),
        Some(GizmoAxis::x),
        "the x ring is edge-on: a vertical line"
    );
    assert_eq!(
        ring_hit(&rings, 230.0, 150.0),
        Some(GizmoAxis::y),
        "the y ring is edge-on: a horizontal line"
    );
    assert_eq!(ring_hit(&rings, 330.0, 250.0), None);
}

#[test]
fn rotate_drags_turn_by_the_angle_swept_around_the_entity() {
    use concordia_render::rotate_angle;
    let view = front();
    let quarter = std::f32::consts::FRAC_PI_2;
    let ccw = rotate_angle(
        &view,
        Vec3::ZERO,
        Vec3::Z,
        [260.0, 150.0],
        [200.0, 90.0],
        400.0,
        300.0,
    );
    assert!(
        (ccw - quarter).abs() < 1e-3,
        "right → top is +90° about +z (toward the eye): {ccw}"
    );
    let cw = rotate_angle(
        &view,
        Vec3::ZERO,
        Vec3::Z,
        [260.0, 150.0],
        [200.0, 210.0],
        400.0,
        300.0,
    );
    assert!((cw + quarter).abs() < 1e-3, "{cw}");
    let away = rotate_angle(
        &view,
        Vec3::ZERO,
        Vec3::NEG_Z,
        [260.0, 150.0],
        [200.0, 90.0],
        400.0,
        300.0,
    );
    assert!(
        (away + quarter).abs() < 1e-3,
        "an axis pointing away turns the other way: {away}"
    );
    assert_eq!(
        rotate_angle(
            &view,
            Vec3::ZERO,
            Vec3::Z,
            [200.0, 150.0],
            [260.0, 150.0],
            400.0,
            300.0
        ),
        0.0,
        "from the center: no angle"
    );
}

#[test]
fn scale_drags_grow_by_arrow_lengths_and_never_flip() {
    use concordia_render::scale_factor;
    let view = front();
    let s =
        |axis: Vec3, delta: [f32; 2]| scale_factor(&view, Vec3::ZERO, axis, delta, 400.0, 300.0);
    assert!(
        (s(Vec3::X, [60.0, 0.0]) - 2.0).abs() < 1e-3,
        "one arrow length outward doubles"
    );
    assert!((s(Vec3::X, [-30.0, 0.0]) - 0.5).abs() < 1e-3);
    assert!(
        (s(Vec3::Y, [0.0, -60.0]) - 2.0).abs() < 1e-3,
        "+y is up on screen"
    );
    assert_eq!(
        s(Vec3::X, [-500.0, 0.0]),
        0.01,
        "clamped, never zero or negative"
    );
    assert_eq!(
        s(Vec3::Z, [40.0, 40.0]),
        1.0,
        "an axis pointing at the eye does not scale"
    );
}
