//! Property tweens (2.4.4): Godot's easing equations (hand-computed values), tweens over reflected
//! fields at the scaled fixed step, removal when finished, with the entity or when the field is gone.

use std::num::NonZeroUsize;

use concordia_anim::{Ease, PropertyTween, Transition, Tweens, add_tweens, ease};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::math::{Quat, Vec3};
use concordia_core::time::TickRate;
use concordia_reflect::TypeRegistry;
use concordia_scene::builtin::Transform;
use concordia_terra::{Schedule, SimClock, SimLoop, TimeScale, World};

const EPS: f32 = 1e-5;

#[test]
fn easing_curves_match_godots_equations() {
    use Ease::*;
    use Transition::*;
    let cases: &[(Transition, Ease, f32, f32)] = &[
        (Linear, In, 0.25, 0.25),
        (Linear, OutIn, 0.25, 0.25),
        (Sine, In, 0.5, 1.0 - std::f32::consts::FRAC_1_SQRT_2),
        (Sine, Out, 0.5, std::f32::consts::FRAC_1_SQRT_2),
        (Sine, InOut, 0.25, 0.146_446_6),
        (Sine, OutIn, 0.25, 0.353_553_4),
        (Quad, In, 0.5, 0.25),
        (Quad, Out, 0.5, 0.75),
        (Quad, InOut, 0.25, 0.125),
        (Quad, InOut, 0.75, 0.875),
        (Quad, OutIn, 0.75, 0.625),
        (Cubic, In, 0.5, 0.125),
        (Cubic, Out, 0.5, 0.875),
        (Quart, In, 0.5, 0.0625),
        (Quint, In, 0.5, 0.031_25),
        (Expo, In, 0.5, 0.030_25),
        (Expo, Out, 0.5, 0.969_718_75),
        (Elastic, In, 0.5, -0.015_625),
        (Elastic, Out, 0.5, 1.015_625),
        (Circ, In, 0.5, 0.133_974_6),
        (Bounce, Out, 0.5, 0.765_625),
        (Bounce, In, 0.5, 0.234_375),
        (Back, In, 0.5, -0.087_697_5),
        (Back, Out, 0.5, 1.087_697_5),
        // Godot's own in-out forms: back's overshoot × 1.525, expo's halved offsets, elastic's 0.45 period.
        (Back, InOut, 0.25, -0.099_681_84),
        (Back, InOut, 0.75, 1.099_681_8),
        (Expo, InOut, 0.25, 0.015_125),
        (Expo, InOut, 0.75, 0.984_867_2),
        (Elastic, InOut, 0.25, 0.011_969_44),
        (Elastic, InOut, 0.75, 0.988_030_6),
        (Spring, Out, 0.5, 1.051_015_8),
        (Spring, In, 0.25, 0.013_654_86),
    ];
    for (t, e, x, want) in cases {
        let got = ease(*t, *e, *x);
        assert!(
            (got - want).abs() < EPS,
            "{t:?} {e:?} at {x}: {got} vs {want}"
        );
    }
    let transitions = [
        Linear, Sine, Quint, Quart, Quad, Expo, Elastic, Cubic, Circ, Bounce, Back, Spring,
    ];
    for t in transitions {
        for e in [In, Out, InOut, OutIn] {
            // Godot's expo is 0.001 short at its far ends (2^0 − 0.001); a finished tween
            // writes its target exactly, so only the curve shows it.
            let slack = if t == Expo { 1.1e-3 } else { EPS };
            assert!(ease(t, e, 0.0).abs() < slack, "{t:?} {e:?} starts at 0");
            assert!(
                (ease(t, e, 1.0) - 1.0).abs() < slack,
                "{t:?} {e:?} ends at 1"
            );
            assert!(
                ease(t, e, f32::NAN).is_finite(),
                "{t:?} {e:?} NaN is clamped"
            );
        }
    }
}

fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    concordia_assets::register_types(&mut registry).unwrap();
    concordia_anim::register_types(&mut registry).unwrap();
    registry
}

fn tween(field: &str, to: &[f32], duration: f32) -> PropertyTween {
    PropertyTween {
        component: "Transform".into(),
        field: field.into(),
        to: to.to_vec(),
        from: None,
        duration,
        delay: 0.0,
        elapsed: 0.0,
        transition: Transition::Linear,
        ease: Ease::InOut,
    }
}

struct Stage {
    world: World,
    sim: SimLoop,
    jobs: JobSystem,
    ids: DeterministicIdGen,
}

impl Stage {
    /// Ticks of 1/4 s.
    fn new() -> Self {
        let mut world = World::new();
        let mut schedule = Schedule::new();
        add_tweens(&mut schedule, &registry());
        let sim = SimLoop::new(TickRate::from_hz(4).unwrap(), schedule, &mut world).unwrap();
        Self {
            world,
            sim,
            jobs: JobSystem::new(NonZeroUsize::new(2).unwrap()).unwrap(),
            ids: DeterministicIdGen::new(4),
        }
    }

    fn spawn(&mut self, tweens: Vec<PropertyTween>) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        self.world.insert(id, Transform::default()).unwrap();
        self.world.insert(id, Tweens { tweens }).unwrap();
        id
    }

    fn step(&mut self, ticks: u32) {
        self.sim.step(&mut self.world, &self.jobs, ticks);
    }

    fn x(&self, id: EntityId) -> f32 {
        self.world.get::<Transform>(id).unwrap().translation.x
    }
}

#[test]
fn tween_reaches_target_with_easing_and_dies_with_its_entity() {
    let mut stage = Stage::new();
    let mut quad = tween("/translation", &[4.0, 0.0, 0.0], 1.0);
    quad.transition = Transition::Quad;
    quad.ease = Ease::In;
    let id = stage.spawn(vec![quad]);
    // Quad in: 4 × x²: 0.25, 1, 2.25, then exactly 4.
    for want in [0.25, 1.0, 2.25] {
        stage.step(1);
        assert!(
            (stage.x(id) - want).abs() < EPS,
            "{} vs {want}",
            stage.x(id)
        );
        assert!(stage.world.get::<Tweens>(id).is_some());
    }
    stage.step(1);
    assert_eq!(
        stage.world.get::<Transform>(id).unwrap().translation,
        Vec3::new(4.0, 0.0, 0.0)
    );
    assert!(
        stage.world.get::<Tweens>(id).is_none(),
        "a finished tween is removed, and the empty component with it"
    );

    // Expo-in's curve ends at 0.999 (Godot's equation); the finished tween still lands exactly.
    let mut expo = tween("/translation/2", &[10.0], 0.5);
    expo.transition = Transition::Expo;
    expo.ease = Ease::In;
    let id = stage.spawn(vec![expo]);
    stage.step(2);
    assert_eq!(
        stage.world.get::<Transform>(id).unwrap().translation.z,
        10.0
    );

    // A delayed tween captures its start when the delay ends: x moved to 2 meanwhile.
    let mut later = tween("/translation/0", &[6.0], 0.5);
    later.delay = 0.5;
    let id = stage.spawn(vec![later]);
    stage.step(1);
    stage.world.get_mut::<Transform>(id).unwrap().translation.x = 2.0;
    stage.step(1);
    assert!(
        (stage.x(id) - 2.0).abs() < EPS,
        "the delay ends: from = 2, nothing moves yet"
    );
    stage.step(1);
    assert!(
        (stage.x(id) - 4.0).abs() < EPS,
        "linear: half-way from 2 to 6: {}",
        stage.x(id)
    );
    let t = &stage.world.get::<Tweens>(id).unwrap().tweens[0];
    assert_eq!(
        t.from,
        Some(vec![2.0]),
        "the captured start is kept (hashed)"
    );

    // Despawning the entity takes its tweens with it; nothing else changes.
    let other = stage.spawn(vec![tween("/translation/1", &[1.0], 10.0)]);
    stage.world.despawn(id).unwrap();
    stage.step(2);
    assert!(!stage.world.contains(id));
    assert!(
        stage.world.get::<Tweens>(other).is_some(),
        "other tweens run on"
    );

    // Rotations are normalized as they move.
    let q = Quat::from_rotation_y(std::f32::consts::PI * 0.9).to_array();
    let spin = stage.spawn(vec![tween("/rotation", &q, 1.0)]);
    stage.step(2);
    let r = stage.world.get::<Transform>(spin).unwrap().rotation;
    assert!((r.length() - 1.0).abs() < EPS, "{r:?}");
}

#[test]
fn tween_holds_at_time_scale_zero() {
    let mut stage = Stage::new();
    let id = stage.spawn(vec![tween("/translation/0", &[1.0], 1.0)]);
    stage.step(1);
    // In-out linear is linear: 0.25 after one tick.
    assert!((stage.x(id) - 0.25).abs() < EPS);
    stage.world.resource_mut::<SimClock>().unwrap().time_scale = TimeScale::new(0.0).unwrap();
    stage.step(10);
    assert!((stage.x(id) - 0.25).abs() < EPS, "frozen");
    assert!((stage.world.get::<Tweens>(id).unwrap().tweens[0].elapsed - 0.25).abs() < EPS);
    stage.world.resource_mut::<SimClock>().unwrap().time_scale = TimeScale::new(0.5).unwrap();
    stage.step(2);
    assert!(
        (stage.x(id) - 0.5).abs() < EPS,
        "half speed: 2 × 1/8 s more"
    );
}

#[test]
fn several_tweens_on_one_field_apply_in_order_and_the_last_wins() {
    let mut stage = Stage::new();
    // Explicit starts (a captured start would see the earlier tween's write, as in Godot).
    let from_zero = |to: f32| PropertyTween {
        from: Some(vec![0.0]),
        ..tween("/translation/0", &[to], 1.0)
    };
    let id = stage.spawn(vec![
        from_zero(8.0),
        from_zero(-4.0),
        tween("/translation/1", &[4.0], 1.0),
    ]);
    stage.step(1);
    let t = stage.world.get::<Transform>(id).unwrap().translation;
    assert!((t.x + 1.0).abs() < EPS, "the second x tween wins: {t}");
    assert!(
        (t.y - 1.0).abs() < EPS,
        "a tween on another field runs too: {t}"
    );
}

#[test]
fn a_tween_whose_field_is_gone_or_not_numeric_is_dropped() {
    let mut stage = Stage::new();
    let mut gone = tween("/nope", &[1.0], 1.0);
    gone.component = "Transform".into();
    let mut no_component = tween("/translation", &[1.0, 1.0, 1.0], 1.0);
    no_component.component = "Velocity".into();
    let wrong_size = tween("/translation", &[1.0], 1.0);
    let id = stage.spawn(vec![
        gone,
        no_component,
        wrong_size,
        tween("/scale/2", &[3.0], 0.5),
    ]);
    stage.step(1);
    let left = &stage.world.get::<Tweens>(id).unwrap().tweens;
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(left[0].field, "/scale/2");
    assert!((stage.world.get::<Transform>(id).unwrap().scale.z - 2.0).abs() < EPS);
    assert_eq!(
        stage.world.get::<Transform>(id).unwrap().translation,
        Vec3::ZERO
    );
}
