//! Fields (M8 8.4a): regions that strain, crumble, anchor or kill. The slab of `common` sits at
//! (0, 3, 0): its piece centres are at x = 0.25, 0.75, 1.25, 1.75 and its bond centres at
//! x = 0.5, 1.0, 1.5, all at y = 3.5, z = 0.5.

mod common;

use common::*;
use concordia_core::id::DeterministicIdGen;
use concordia_core::math::Vec3;
use concordia_destruction::{Falloff, Field, FieldEffect, FieldShape, Fragment};
use concordia_physics::{BodyKind, RigidBody};
use concordia_scene::builtin::Transform;
use concordia_terra::World;

fn field(world: &mut World, at: Vec3, field: Field) {
    let id = world.spawn(&mut DeterministicIdGen::new(99)).unwrap();
    world
        .insert(
            id,
            Transform {
                translation: at,
                ..Transform::default()
            },
        )
        .unwrap();
    world.insert(id, field).unwrap();
}

fn sphere(radius: f32, falloff: Falloff, effect: FieldEffect, magnitude: f32) -> Field {
    Field {
        shape: FieldShape::Sphere { radius },
        falloff,
        effect,
        magnitude,
        duration: 0.0,
    }
}

fn on_bond(x: f32) -> Vec3 {
    Vec3::new(x, 3.5, 0.5)
}

#[test]
fn strain_field_breaks_only_inside_its_shape() {
    let r = Run {
        setup: Some(|w| {
            field(
                w,
                on_bond(1.0),
                sphere(0.2, Falloff::None, FieldEffect::Strain, 1000.0),
            );
        }),
        ..Run::default()
    };
    let mut world = run(&r, 30);
    assert_eq!(
        fragments(&mut world).keys().cloned().collect::<Vec<_>>(),
        [vec![0, 1], vec![2, 3]],
        "only the bond inside the sphere broke"
    );
    // A box over the last bond only.
    let r = Run {
        setup: Some(|w| {
            let f = Field {
                shape: FieldShape::Box {
                    half_extents: Vec3::new(0.2, 1.0, 1.0),
                },
                ..sphere(0.0, Falloff::None, FieldEffect::Strain, 1000.0)
            };
            field(w, on_bond(1.5), f);
        }),
        ..Run::default()
    };
    let mut world = run(&r, 30);
    assert_eq!(
        fragments(&mut world).keys().cloned().collect::<Vec<_>>(),
        [vec![0, 1, 2], vec![3]]
    );
}

#[test]
fn falloff_scales_effect_with_distance() {
    // Too weak to break anything in 30 ticks; compare the damage the three bonds took.
    let damage = |setup: fn(&mut World)| {
        let r = Run {
            setup: Some(setup),
            ..Run::default()
        };
        let world = run(&r, 30);
        world.get::<Fragment>(root()).unwrap().damage.clone()
    };
    // Bonds 0–1, 1–2, 2–3 sit 0.5, 0, 0.5 m from the centre.
    let linear = damage(|w| {
        field(
            w,
            on_bond(1.0),
            sphere(1.0, Falloff::Linear, FieldEffect::Strain, 10.0),
        )
    });
    assert!(linear[1] > 0.0);
    assert!((linear[0] / linear[1] - 0.5).abs() < 1e-3, "{linear:?}");
    assert!((linear[2] / linear[1] - 0.5).abs() < 1e-3, "{linear:?}");
    let inverse = damage(|w| {
        field(
            w,
            on_bond(1.0),
            sphere(1.0, Falloff::InverseSquare, FieldEffect::Strain, 10.0),
        )
    });
    assert!(
        (inverse[0] / inverse[1] - 0.8).abs() < 1e-3,
        "1 / (1 + 0.5²): {inverse:?}"
    );
}

#[test]
fn anchor_field_pins_pieces_it_covers() {
    let r = Run {
        gravity: true,
        blows: vec![middle(5)],
        setup: Some(|w| {
            field(
                w,
                Vec3::new(0.25, 3.5, 0.5),
                sphere(0.2, Falloff::None, FieldEffect::Anchor, 1.0),
            );
        }),
        ..Run::default()
    };
    let mut world = run(&r, 90);
    let parts = fragments(&mut world);
    assert_eq!(parts.len(), 2, "{parts:?}");
    let (pinned, loose) = (parts[&vec![0, 1]], parts[&vec![2, 3]]);
    assert_eq!(
        world.get::<RigidBody>(pinned).unwrap().kind,
        BodyKind::Static
    );
    let at = |id| world.get::<Transform>(id).unwrap().translation;
    assert!((at(pinned) - START).length() < 0.01, "{}", at(pinned));
    assert!(at(loose).y < START.y - 1.0, "{}", at(loose));
}

#[test]
fn kill_field_despawns_debris_but_not_the_intact_breakable() {
    // The kill sphere covers the right half's centre of mass (x = 1.5) and the whole's (x = 1)
    // is outside it; the whole is a breakable, not debris, either way.
    let r = Run {
        blows: vec![middle(5)],
        setup: Some(|w| {
            field(
                w,
                on_bond(1.5),
                sphere(0.3, Falloff::None, FieldEffect::Kill, 1.0),
            );
        }),
        ..Run::default()
    };
    let before = run(
        &Run {
            setup: r.setup,
            ..Run::default()
        },
        20,
    );
    assert!(
        before.get::<Fragment>(root()).is_some(),
        "intact: not killed"
    );
    let mut world = run(&r, 20);
    assert_eq!(
        fragments(&mut world).keys().cloned().collect::<Vec<_>>(),
        [vec![0, 1]],
        "the right half fell into the kill field and was removed"
    );
}

#[test]
fn crumble_field_breaks_every_bond_of_what_it_touches() {
    let r = Run {
        setup: Some(|w| {
            field(
                w,
                Vec3::new(0.25, 3.5, 0.5),
                sphere(0.1, Falloff::None, FieldEffect::Crumble, 1.0),
            );
        }),
        ..Run::default()
    };
    let mut world = run(&r, 10);
    assert_eq!(fragments(&mut world).len(), 4, "every piece on its own");
}

#[test]
fn a_field_with_a_duration_stops_acting_when_it_expires() {
    // 1000/s for 0.05 s is 50 < 100: the bond is damaged but holds.
    let r = Run {
        setup: Some(|w| {
            let f = Field {
                duration: 0.05,
                ..sphere(0.2, Falloff::None, FieldEffect::Strain, 1000.0)
            };
            field(w, on_bond(1.0), f);
        }),
        ..Run::default()
    };
    let mut world = run(&r, 60);
    assert_eq!(fragments(&mut world).len(), 1);
    let damage = world.get::<Fragment>(root()).unwrap().damage[1];
    assert!((40.0..=70.0).contains(&damage), "{damage}");
}

/// The slab's x velocity and position after `ticks` with one field at x = −2 (so pushes point +x).
fn pushed(effect: FieldEffect, ticks: u32) -> (f32, f32) {
    let r = Run {
        setup: Some(match effect {
            FieldEffect::Force => |w: &mut World| {
                field(
                    w,
                    Vec3::new(-2.0, 3.5, 0.5),
                    sphere(10.0, Falloff::None, FieldEffect::Force, 2000.0),
                )
            },
            _ => |w: &mut World| {
                field(
                    w,
                    Vec3::new(-2.0, 3.5, 0.5),
                    sphere(10.0, Falloff::None, FieldEffect::Impulse, 1000.0),
                )
            },
        } as fn(&mut World)),
        ..Run::default()
    };
    let world = run(&r, ticks);
    let v = world
        .get::<concordia_scene::builtin::Velocity>(root())
        .unwrap()
        .linear;
    (v.x, world.get::<Transform>(root()).unwrap().translation.x)
}

#[test]
fn force_field_pushes_steadily_away_from_its_centre() {
    // 2000 N on 2000 kg: 1 m/s², so about 0.5 m/s after half a second.
    let (v30, _) = pushed(FieldEffect::Force, 31);
    let (v60, x60) = pushed(FieldEffect::Force, 61);
    assert!((v30 - 0.5).abs() < 0.05, "{v30}");
    assert!((v60 - 1.0).abs() < 0.05, "it keeps pushing: {v60}");
    assert!(x60 > 0.2, "{x60}");
}

#[test]
fn impulse_field_kicks_once() {
    // 1000 N·s on 2000 kg: 0.5 m/s, once.
    let (v10, _) = pushed(FieldEffect::Impulse, 10);
    let (v40, _) = pushed(FieldEffect::Impulse, 40);
    assert!((v10 - 0.5).abs() < 0.02, "{v10}");
    assert!((v40 - v10).abs() < 1e-3, "only once: {v10} then {v40}");
}

#[test]
fn sleep_field_stops_what_it_covers() {
    let r = Run {
        velocity: concordia_scene::builtin::Velocity {
            linear: Vec3::new(1.0, 0.0, 0.0),
            angular: Vec3::ZERO,
        },
        setup: Some(|w| {
            field(
                w,
                on_bond(1.0),
                sphere(10.0, Falloff::None, FieldEffect::Sleep, 1.0),
            );
        }),
        ..Run::default()
    };
    let world = run(&r, 20);
    let v = world
        .get::<concordia_scene::builtin::Velocity>(root())
        .unwrap()
        .linear;
    assert_eq!(v, Vec3::ZERO, "asleep");
}

#[test]
fn disable_field_takes_what_it_covers_out_of_physics() {
    let r = Run {
        gravity: true,
        setup: Some(|w| {
            field(
                w,
                on_bond(1.0),
                sphere(10.0, Falloff::None, FieldEffect::Disable, 1.0),
            );
        }),
        ..Run::default()
    };
    let world = run(&r, 60);
    assert!(
        world
            .get::<concordia_physics::PhysicsDisabled>(root())
            .is_some()
    );
    let y = world.get::<Transform>(root()).unwrap().translation.y;
    assert!((y - START.y).abs() < 0.01, "it didn't fall: {y}");
}
