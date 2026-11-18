//! Locating non-finite floats before saving (M0 phase 0.5).

mod common;

use common::{Health, Mode, Zoo};
use concordia_reflect::find_non_finite;

#[test]
fn finite_values_have_no_pointer() {
    assert_eq!(find_non_finite(&Health { hp: 1.0, max: -0.0 }), None);
}

#[test]
fn pointer_follows_the_json_layout() {
    assert_eq!(
        find_non_finite(&Health {
            hp: f32::NAN,
            max: 1.0
        }),
        Some("/hp".into())
    );
    assert_eq!(
        find_non_finite(&Mode::Walk(f32::INFINITY)),
        Some("/Walk".into())
    );
    assert_eq!(
        find_non_finite(&Mode::Jump(1.0, f32::NEG_INFINITY)),
        Some("/Jump/1".into())
    );
    let patrol = Mode::Patrol {
        points: vec![concordia_core::math::Vec3::new(0.0, f32::NAN, 0.0)],
        looping: false,
    };
    assert_eq!(find_non_finite(&patrol), Some("/Patrol/points/0/1".into()));
}

#[test]
fn options_and_maps_are_followed() {
    let mut zoo = common::zoo_sample();
    zoo.offset = Some(concordia_core::math::Vec2::new(f32::NAN, 0.0));
    assert_eq!(find_non_finite(&zoo), Some("/offset/0".into()));
    let mut zoo: Zoo = common::zoo_sample();
    zoo.stats.insert("a/b".into(), f64::NAN);
    assert_eq!(find_non_finite(&zoo), Some("/stats/a~1b".into()));
}
