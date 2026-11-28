//! Debris (M8 8.5a): parts that split off are debris. A budget caps how many there are (oldest
//! go first), a lifetime removes them, and parts below a size lose their collision.

mod common;

use common::*;
use concordia_core::math::Vec3;
use concordia_destruction::DestructionSettings;
use concordia_physics::Collider;

/// A blow on the first cut (x = 0.5), strong enough to break that bond only.
fn first(tick: u64) -> Blow {
    Blow {
        tick,
        local: Vec3::new(0.5, 0.5, 0.5),
        strength: 150.0,
    }
}

fn settings(max_debris: u32, lifetime: f32, min_volume: f32) -> DestructionSettings {
    DestructionSettings {
        max_debris,
        debris_lifetime: lifetime,
        min_debris_volume: min_volume,
        ..DestructionSettings::default()
    }
}

#[test]
fn debris_over_budget_removes_the_oldest_first() {
    // Tick 5 splits the slab in halves (two debris); tick 10 splits the left half again, making
    // three. With room for two, the oldest, the right half, goes.
    let r = Run {
        blows: vec![middle(5), first(10)],
        debris: settings(2, 0.0, 0.0),
        ..Run::default()
    };
    let mut world = run(&r, 20);
    assert_eq!(
        fragments(&mut world).keys().cloned().collect::<Vec<_>>(),
        [vec![0], vec![1]]
    );
    // With room for three, all stay.
    let r = Run {
        debris: settings(3, 0.0, 0.0),
        ..r
    };
    assert_eq!(fragments(&mut run(&r, 20)).len(), 3);
}

#[test]
fn debris_past_its_lifetime_is_removed() {
    let r = Run {
        blows: vec![middle(5)],
        debris: settings(64, 0.2, 0.0),
        ..Run::default()
    };
    assert_eq!(fragments(&mut run(&r, 10)).len(), 2, "young debris stays");
    assert!(
        fragments(&mut run(&r, 30)).is_empty(),
        "0.2 s later it is gone"
    );
}

#[test]
fn small_debris_drops_collision() {
    // A crumbling blow makes four 0.5 m³ quarters.
    let crumble = Blow {
        tick: 5,
        local: Vec3::new(1.0, 0.5, 0.5),
        strength: 1.0e6,
    };
    let quarters = |min_volume| {
        let r = Run {
            radius: 5.0,
            blows: vec![crumble],
            debris: settings(64, 0.0, min_volume),
            ..Run::default()
        };
        let mut world = run(&r, 10);
        let parts = fragments(&mut world);
        assert_eq!(parts.len(), 4);
        parts
            .values()
            .filter(|id| world.get::<Collider>(**id).is_some())
            .count()
    };
    assert_eq!(quarters(0.75), 0, "0.5 m³ < 0.75: no collision");
    assert_eq!(quarters(0.25), 4, "0.5 m³ ≥ 0.25: they keep it");
}
