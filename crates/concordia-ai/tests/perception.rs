//! Perception (M4 4.12c, M8): sight (cone, range, factions), hearing (noise radius and falloff),
//! damage, a memory that decays and forgets, and the strongest hostile on the blackboard.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use concordia_ai::{
    AiConfig, AiDamaged, AiNoise, BbType, BbValue, BehaviorTree, Blackboard, Brain, BtState,
    Faction, Memory, NodeKind, Perceiver, Sense, Spec, TaskKind, Trees, add_ai_processors,
    insert_ai_resources,
};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::math::{Quat, Vec3};
use concordia_core::rng::SimRng;
use concordia_core::time::TickRate;
use concordia_scene::builtin::Transform;
use concordia_terra::{Schedule, SimLoop, World};

/// Idles forever; declares the keys perception fills.
fn watcher() -> BehaviorTree {
    let keys = BTreeMap::from([
        ("target".into(), BbType::Entity),
        ("target_position".into(), BbType::Vec3),
    ]);
    BehaviorTree::build(
        keys,
        Spec::new(NodeKind::Task(TaskKind::Wait { seconds: 1e6 })),
    )
    .unwrap()
}

struct Setup {
    world: World,
    sim: SimLoop,
    ids: DeterministicIdGen,
}

fn setup() -> Setup {
    let mut world = World::new();
    world.insert_resource(SimRng::new(3));
    world.insert_resource(Trees::from_trees([("w.bt.ron".to_owned(), watcher())]));
    insert_ai_resources(&mut world);
    let mut schedule = Schedule::new();
    add_ai_processors(&mut schedule, AiConfig::default());
    let sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    Setup {
        world,
        sim,
        ids: DeterministicIdGen::new(9),
    }
}

fn at(x: f32, z: f32) -> Transform {
    Transform {
        translation: Vec3::new(x, 0.0, z),
        ..Transform::default()
    }
}

impl Setup {
    /// A guard of `faction` at `t`, facing +Z, noticing everyone else.
    fn guard(&mut self, faction: &str, t: Transform, perceiver: Perceiver) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        self.world
            .insert(
                id,
                Brain {
                    tree: "w.bt.ron".into(),
                },
            )
            .unwrap();
        self.world
            .insert(id, Blackboard::for_tree(&watcher()))
            .unwrap();
        self.world.insert(id, BtState::default()).unwrap();
        self.world.insert(id, Memory::default()).unwrap();
        self.world.insert(id, perceiver).unwrap();
        self.world
            .insert(
                id,
                Faction {
                    name: faction.into(),
                },
            )
            .unwrap();
        self.world.insert(id, t).unwrap();
        id
    }

    fn thing(&mut self, faction: &str, t: Transform) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        self.world
            .insert(
                id,
                Faction {
                    name: faction.into(),
                },
            )
            .unwrap();
        self.world.insert(id, t).unwrap();
        id
    }

    fn step(&mut self, ticks: u32) {
        self.sim.step(
            &mut self.world,
            &JobSystem::new(NonZeroUsize::MIN).unwrap(),
            ticks,
        );
    }

    fn remembers(&self, who: EntityId, what: EntityId) -> Option<(Sense, f32)> {
        self.world
            .get::<Memory>(who)?
            .entries
            .iter()
            .find(|e| e.entity == what)
            .map(|e| (e.sense, e.strength))
    }
}

#[test]
fn sight_respects_cone_range_and_factions() {
    let mut s = setup();
    let guard = s.guard(
        "guards",
        at(0.0, 0.0),
        Perceiver {
            sight_range: 10.0,
            sight_fov: 90.0,
            ..Perceiver::default()
        },
    );
    let ahead = s.thing("thieves", at(1.0, 6.0));
    let behind = s.thing("thieves", at(0.0, -4.0));
    let far = s.thing("thieves", at(0.0, 14.0));
    let wide = s.thing("thieves", at(6.0, 2.0));
    let friend = s.thing("guards", at(0.0, 3.0));
    s.step(2);
    assert!(matches!(s.remembers(guard, ahead), Some((Sense::Sight, _))));
    for (name, other) in [
        ("behind", behind),
        ("too far", far),
        ("outside the cone", wide),
        ("a friend", friend),
    ] {
        assert_eq!(s.remembers(guard, other), None, "{name}");
    }
    // The strongest hostile is on the blackboard.
    let bb = s.world.get::<Blackboard>(guard).unwrap();
    assert_eq!(bb.get("target"), Some(&BbValue::Entity(ahead)));
    assert_eq!(
        bb.get("target_position"),
        Some(&BbValue::Vec3([1.0, 0.0, 6.0]))
    );
    // Turning around sees what was behind.
    s.world
        .insert(
            guard,
            Transform {
                rotation: Quat::from_rotation_y(std::f32::consts::PI),
                ..at(0.0, 0.0)
            },
        )
        .unwrap();
    s.step(2);
    assert!(matches!(
        s.remembers(guard, behind),
        Some((Sense::Sight, _))
    ));
}

#[test]
fn perception_hearing_event_reaches_only_agents_in_radius() {
    let mut s = setup();
    let near = s.guard(
        "guards",
        at(3.0, 0.0),
        Perceiver {
            sight_range: 0.0,
            ..Perceiver::default()
        },
    );
    let far = s.guard(
        "guards",
        at(12.0, 0.0),
        Perceiver {
            sight_range: 0.0,
            ..Perceiver::default()
        },
    );
    let source = s.thing("thieves", at(0.0, 0.0));
    s.world.signal(
        source,
        AiNoise {
            position: [0.0, 0.0, 0.0],
            radius: 8.0,
            loudness: 1.0,
        },
    );
    s.step(1);
    let (sense, strength) = s.remembers(near, source).expect("heard");
    assert_eq!(sense, Sense::Hearing);
    assert!(
        (strength - (1.0 - 3.0 / 8.0)).abs() < 1e-4,
        "linear falloff: {strength}"
    );
    assert_eq!(s.remembers(far, source), None, "outside the radius");
}

#[test]
fn perception_memory_decays_and_forgets() {
    let mut s = setup();
    let guard = s.guard(
        "guards",
        at(0.0, 0.0),
        Perceiver {
            sight_range: 0.0,
            memory_seconds: 0.5,
            ..Perceiver::default()
        },
    );
    let attacker = s.thing("thieves", at(0.0, -20.0));
    s.world.signal(
        guard,
        AiDamaged {
            source: attacker,
            amount: 3.0,
        },
    );
    s.step(1);
    let (sense, strength) = s.remembers(guard, attacker).expect("felt");
    assert_eq!(sense, Sense::Damage);
    assert!((strength - 1.0).abs() < 1e-4);
    s.step(15);
    let (_, half) = s.remembers(guard, attacker).expect("still remembered");
    assert!(
        (half - 0.5).abs() < 0.02,
        "decays linearly over memory_seconds: {half}"
    );
    s.step(16);
    assert_eq!(s.remembers(guard, attacker), None, "forgotten");
    let bb = s.world.get::<Blackboard>(guard).unwrap();
    assert_eq!(bb.get("target"), None, "nothing to chase");
}
