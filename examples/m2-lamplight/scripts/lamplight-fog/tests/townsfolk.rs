//! The townsfolk's pure rules (M2 phase 2.10c.2). By hand:
//! - Fear: a wisp within 5 m on the ground frightens a townsperson (height doesn't count), so a
//!   wisp at 4.9 m does, one at 5.1 m doesn't, and one 4 m away but 10 m up does.
//! - The walk blend: metres moved this update over the update's seconds, over the strolling speed
//!   1.2 m/s, held to 0..1. 0.02 m in 1/60 s is 1.2 m/s, so 1; 0.01 m is 0.5; fleeing (3 m/s) is 1.

use concordia_core::math::Vec3;
use lamplight_fog::townsfolk::{STROLL_SPEED, frightened, walk_amount};

const DT: f32 = 1.0 / 60.0;

#[test]
fn a_close_wisp_frightens() {
    let at = Vec3::new(10.0, 0.0, 10.0);
    assert!(frightened(at, &[Vec3::new(14.9, 0.0, 10.0)]));
    assert!(!frightened(at, &[Vec3::new(15.1, 0.0, 10.0)]));
    assert!(frightened(
        at,
        &[Vec3::new(30.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 14.0)]
    ));
    assert!(!frightened(at, &[]));
}

#[test]
fn the_walk_blend_follows_the_speed() {
    assert_eq!(STROLL_SPEED, 1.2);
    assert!((walk_amount(0.02, DT) - 1.0).abs() < 1e-4);
    assert!((walk_amount(0.01, DT) - 0.5).abs() < 1e-4);
    assert_eq!(walk_amount(0.05, DT), 1.0, "fleeing is a full walk");
    assert_eq!(walk_amount(0.0, DT), 0.0);
    assert_eq!(walk_amount(0.01, 0.0), 0.0, "no time, no blend");
}
