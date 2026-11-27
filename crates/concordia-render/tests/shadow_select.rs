//! The shadowed local lights' selection (M4 4.7c, C3): the top `max` by importance, ties by key,
//! with hysteresis and stable slots; the same for any input order.

use concordia_render::shadow_select::{LocalSelection, importance};

/// `n` candidates (key, importance), importance falling with the key.
fn candidates(n: u32) -> Vec<(u32, f32)> {
    (0..n).map(|k| (k, 1000.0 - k as f32)).collect()
}

#[test]
fn many_local_lights_select_top_64_by_importance() {
    let mut s = LocalSelection::new(64);
    let picked = s.select(&candidates(200));
    assert_eq!(picked.dropped, 136);
    let keys: Vec<u32> = (0..64).map(|slot| s.key(slot).copied().unwrap()).collect();
    assert_eq!(
        keys,
        (0..64).collect::<Vec<_>>(),
        "the top 64, slots in importance order"
    );
    assert_eq!(s.slot_of(&63), Some(63));
    assert_eq!(s.slot_of(&64), None);
}

#[test]
fn ties_go_to_the_lower_key() {
    let mut s = LocalSelection::new(2);
    s.select(&[(9, 5.0), (3, 5.0), (7, 5.0)]);
    assert_eq!(
        (s.slot_of(&3), s.slot_of(&7), s.slot_of(&9)),
        (Some(0), Some(1), None)
    );
}

/// A selected light keeps its slot unless an outsider beats it by more than 10 %: it holds against
/// 5 %, yields at 15 %, and the newcomer takes the freed slot.
#[test]
fn selection_has_hysteresis_and_stable_slots() {
    let mut s = LocalSelection::new(2);
    s.select(&[(1, 10.0), (2, 8.0), (3, 1.0)]);
    assert_eq!((s.slot_of(&1), s.slot_of(&2)), (Some(0), Some(1)));
    let picked = s.select(&[(1, 10.0), (2, 8.0), (3, 8.4)]);
    assert_eq!(
        (s.slot_of(&2), s.slot_of(&3)),
        (Some(1), None),
        "holds at 5 %"
    );
    assert!(picked.entered.is_empty() && picked.left.is_empty());
    let picked = s.select(&[(1, 10.0), (2, 8.0), (3, 9.2)]);
    assert_eq!(
        (s.slot_of(&2), s.slot_of(&3)),
        (None, Some(1)),
        "yields at 15 %"
    );
    assert_eq!((picked.entered, picked.left), (vec![3], vec![2]));
    s.select(&[(1, 2.0), (3, 9.2)]);
    assert_eq!(
        (s.slot_of(&1), s.slot_of(&3)),
        (Some(0), Some(1)),
        "slots stay put"
    );
}

#[test]
fn local_selection_is_deterministic() {
    let mut list = candidates(150);
    let mut a = LocalSelection::new(64);
    a.select(&list);
    list.reverse();
    list.swap(3, 90);
    let mut b = LocalSelection::new(64);
    b.select(&list);
    for slot in 0..64 {
        assert_eq!(a.key(slot), b.key(slot), "slot {slot}");
    }
}

/// Importance is the range sphere's share of the view times the light's candela: bigger, nearer
/// and brighter lights first; zero for a dark light.
#[test]
fn importance_grows_with_coverage_and_intensity() {
    use concordia_core::math::Vec3;
    let eye = Vec3::ZERO;
    let at = |z: f32| Vec3::new(0.0, 0.0, -z);
    let fov = 1.0;
    let near = importance(at(10.0), 2.0, 800.0, eye, fov);
    let far = importance(at(40.0), 2.0, 800.0, eye, fov);
    let wide = importance(at(10.0), 4.0, 800.0, eye, fov);
    let bright = importance(at(10.0), 2.0, 1600.0, eye, fov);
    assert!(
        near > far && wide > near && bright > near,
        "{near} {far} {wide} {bright}"
    );
    assert_eq!(importance(at(10.0), 2.0, 0.0, eye, fov), 0.0);
    let inside = importance(at(1.0), 2.0, 800.0, eye, fov);
    assert!(
        inside.is_finite() && inside >= wide,
        "the camera inside the range: {inside}"
    );
}
