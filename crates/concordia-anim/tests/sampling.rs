//! Clip sampling per the glTF 2.0 specification (Appendix C), playback time and clip validation (2.4.1).

mod common;

use common::{clip, close, joint};
use concordia_anim::{BoundClip, Skeleton, advance, validate_clip};
use concordia_assets::{Channel, ChannelPath, Interpolation};
use concordia_core::math::{Quat, Vec3};
use concordia_scene::builtin::Transform;

const LINEAR: Interpolation = Interpolation::Linear;
const CUBIC: Interpolation = Interpolation::CubicSpline;

fn bone() -> Skeleton {
    Skeleton::new(vec![joint("bone", None, Vec3::new(1.0, 2.0, 3.0))]).unwrap()
}

/// The bone's local transform after sampling `c` at `t`, starting from rest.
fn at(c: &concordia_assets::ClipData, t: f32) -> Transform {
    let skeleton = bone();
    let bound = BoundClip::bind(c, &skeleton).unwrap();
    let mut pose = skeleton.rest_pose();
    bound.sample(t, &mut pose).unwrap();
    pose.bones[0]
}

fn quat(q: Quat) -> [f32; 4] {
    q.to_array()
}

fn angle_about_y(q: Quat) -> f32 {
    let (axis, angle) = q.to_axis_angle();
    angle.to_degrees() * axis.y.signum()
}

#[test]
fn clip_sampling_interpolates_trs_linear_slerp_and_step() {
    let t = clip(
        "bone",
        ChannelPath::Translation,
        LINEAR,
        &[0.0, 1.0],
        &[0., 0., 0., 2., 4., 6.],
    );
    assert!(close(
        at(&t, 0.25).translation,
        Vec3::new(0.5, 1.0, 1.5),
        1e-6
    ));
    assert_eq!(
        at(&t, 0.25).scale,
        Vec3::ONE,
        "unanimated parts keep their rest values"
    );

    let quarter = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
    let mut values = quat(Quat::IDENTITY).to_vec();
    values.extend(quat(quarter));
    let r = clip("bone", ChannelPath::Rotation, LINEAR, &[0.0, 1.0], &values);
    // Slerp gives 22.5° at a quarter; normalized lerp would give about 21.6°.
    let a = angle_about_y(at(&r, 0.25).rotation);
    assert!((a - 22.5).abs() < 0.05, "slerp, not nlerp: {a}");
    let mut flipped = quat(Quat::IDENTITY).to_vec();
    flipped.extend(quat(-quarter));
    let r = clip("bone", ChannelPath::Rotation, LINEAR, &[0.0, 1.0], &flipped);
    let a = angle_about_y(at(&r, 0.5).rotation);
    assert!((a - 45.0).abs() < 0.05, "the shortest path: {a}");

    let s = clip(
        "bone",
        ChannelPath::Scale,
        Interpolation::Step,
        &[0.0, 1.0, 2.0],
        &[1., 1., 1., 2., 2., 2., 3., 3., 3.],
    );
    assert_eq!(at(&s, 0.99).scale, Vec3::ONE);
    assert_eq!(
        at(&s, 1.0).scale,
        Vec3::splat(2.0),
        "a key holds from its own time"
    );
    assert_eq!(at(&s, 1.5).scale, Vec3::splat(2.0));
}

/// Cubic-spline keys for x only: (in-tangent, value, out-tangent) per key, as vec3s.
fn cubic_x(keys: &[(f32, f32, f32)]) -> Vec<f32> {
    keys.iter()
        .flat_map(|&(a, v, b)| [a, 0., 0., v, 0., 0., b, 0., 0.])
        .collect()
}

#[test]
fn cubic_spline_tracks_sample_per_gltf_spec() {
    // Keys at 0, 2 and 2.5 s: unequal spacing, so tangents must be scaled by each segment's length.
    let values = cubic_x(&[(0.0, 0.0, 1.0), (3.0, 1.0, 2.0), (4.0, 3.0, 0.0)]);
    let c = clip(
        "bone",
        ChannelPath::Translation,
        CUBIC,
        &[0.0, 2.0, 2.5],
        &values,
    );
    // s = 0.25, td = 2: 0.28125·b0 + 0.15625·v1 − 0.09375·a1 = 0.28125 + 0.15625 − 0.28125.
    let x = at(&c, 0.5).translation.x;
    assert!((x - 0.15625).abs() < 1e-6, "{x}");
    // s = 0.5, td = 0.5: 0.5·v1 + 0.0625·b1 + 0.5·v2 − 0.0625·a2 = 0.5 + 0.125 + 1.5 − 0.25.
    let x = at(&c, 2.25).translation.x;
    assert!((x - 1.875).abs() < 1e-6, "{x}");
    assert!(
        (at(&c, 2.0).translation.x - 1.0).abs() < 1e-6,
        "passes through each value"
    );

    let q = quat(Quat::from_rotation_y(1.0));
    let zero = [0.0; 4];
    let rot: Vec<f32> = [zero, quat(Quat::IDENTITY), zero, zero, q, zero].concat();
    let c = clip("bone", ChannelPath::Rotation, CUBIC, &[0.0, 1.0], &rot);
    let r = at(&c, 0.3).rotation;
    assert!(
        (r.length() - 1.0).abs() < 1e-6,
        "cubic rotations are normalized: {r}"
    );
}

#[test]
fn sampling_outside_the_keys_holds_the_end_values() {
    let t = clip(
        "bone",
        ChannelPath::Translation,
        LINEAR,
        &[1.0, 2.0],
        &[1., 0., 0., 2., 0., 0.],
    );
    assert_eq!(at(&t, 0.0).translation.x, 1.0, "before the first key");
    assert_eq!(at(&t, 5.0).translation.x, 2.0, "after the last key");
    let values = cubic_x(&[(9.0, 1.0, 9.0), (9.0, 2.0, 9.0)]);
    let c = clip(
        "bone",
        ChannelPath::Translation,
        CUBIC,
        &[1.0, 2.0],
        &values,
    );
    assert_eq!(at(&c, 0.0).translation.x, 1.0, "the value, not a tangent");
    assert_eq!(at(&c, 3.0).translation.x, 2.0);
    let one = clip(
        "bone",
        ChannelPath::Translation,
        LINEAR,
        &[0.5],
        &[7., 0., 0.],
    );
    assert_eq!(
        at(&one, 0.0).translation.x,
        7.0,
        "a single key holds everywhere"
    );
}

#[test]
fn looping_wraps_time_and_clamped_clips_hold_the_last_pose() {
    let eq = |a: f32, b: f32| assert!((a - b).abs() < 1e-6, "{a} != {b}");
    eq(advance(0.9, 0.3, 1.0, true), 0.2);
    eq(advance(0.0, 3.25, 1.0, true), 0.25);
    eq(advance(0.1, -0.3, 1.0, true), 0.8);
    eq(advance(0.9, 0.3, 1.0, false), 1.0);
    eq(advance(0.1, -0.3, 1.0, false), 0.0);
    eq(advance(0.4, 0.3, 0.0, true), 0.0);
    eq(advance(f32::NAN, 0.1, 1.0, true), 0.0);
}

#[test]
fn malformed_clip_is_a_descriptive_error() {
    let err = |c: &concordia_assets::ClipData| validate_clip(c).unwrap_err().to_string();
    let short = clip(
        "bone",
        ChannelPath::Translation,
        LINEAR,
        &[0.0, 1.0],
        &[0., 0., 0.],
    );
    let e = err(&short);
    assert!(e.contains("6 values") && e.contains("bone"), "{e}");
    let unsorted = clip("bone", ChannelPath::Scale, LINEAR, &[1.0, 0.5], &[1.; 6]);
    assert!(err(&unsorted).contains("increasing"), "{}", err(&unsorted));
    let nan = clip(
        "bone",
        ChannelPath::Scale,
        LINEAR,
        &[0.0, f32::NAN],
        &[1.; 6],
    );
    assert!(err(&nan).contains("finite"), "{}", err(&nan));
    let empty = clip("bone", ChannelPath::Scale, LINEAR, &[], &[]);
    assert!(err(&empty).contains("no keys"), "{}", err(&empty));
    let zero = clip("bone", ChannelPath::Rotation, LINEAR, &[0.0], &[0.; 4]);
    assert!(err(&zero).contains("zero"), "{}", err(&zero));
    let mut negative = clip("bone", ChannelPath::Scale, LINEAR, &[0.0], &[1.; 3]);
    negative.duration = -1.0;
    assert!(err(&negative).contains("duration"), "{}", err(&negative));
    assert!(
        BoundClip::bind(&short, &bone()).is_err(),
        "binding validates"
    );

    let long = clip(
        "bone",
        ChannelPath::Rotation,
        LINEAR,
        &[0.0],
        &[0., 0., 0., 2.],
    );
    assert_eq!(
        at(&long, 0.0).rotation,
        Quat::IDENTITY,
        "non-unit rotations are normalized"
    );
}

#[test]
fn binding_reports_unknown_bones_and_skips_morph_weights() {
    let mut c = clip("ghost", ChannelPath::Scale, LINEAR, &[0.0], &[2.; 3]);
    c.channels.push(Channel {
        target_name: "bone".to_owned(),
        path: ChannelPath::Weights,
        values: vec![0.5, 0.5],
        ..c.channels[0].clone()
    });
    c.channels.push(Channel {
        target_name: "ghost".to_owned(),
        ..c.channels[0].clone()
    });
    let bound = BoundClip::bind(&c, &bone()).unwrap();
    assert_eq!(bound.unmatched(), ["ghost"]);
    assert_eq!(bound.skipped(), 1);
    assert_eq!(at(&c, 0.0).scale, Vec3::ONE, "nothing drives the bone");
}
