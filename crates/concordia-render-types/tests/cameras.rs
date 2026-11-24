//! Which camera a frame uses (shared by the renderer and the path tracer).

use concordia_core::id::EntityId;
use concordia_render_types::pick_camera;

fn id(n: u128) -> EntityId {
    format!("0193a1c2-7e40-7000-8000-{n:012x}").parse().unwrap()
}

#[test]
fn the_highest_priority_wins_and_ties_go_to_the_lowest_id() {
    let cameras = [(id(3), 0), (id(1), 5), (id(2), 5), (id(4), -1)];
    assert_eq!(
        pick_camera(cameras, None),
        Some(id(1)),
        "priority 5, lowest id of the tie"
    );
    assert_eq!(
        pick_camera(cameras, Some(id(4))),
        Some(id(4)),
        "a request wins"
    );
    assert_eq!(
        pick_camera(cameras, Some(id(9))),
        None,
        "an unknown request"
    );
    assert_eq!(pick_camera([], None), None);
}
