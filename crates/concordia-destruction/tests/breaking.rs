//! Breaking at runtime on Jolt (M8 8.3). The breakable is a 2 × 1 × 1 slab sliced into four
//! 0.5 m slabs along x (cells 0–3, bonds 0–1, 1–2, 2–3, each 1 m² × strength 100 = 100). Impacts
//! are raised by a test system, so every run sees exactly the same blows.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::math::{Quat, Vec3};
use concordia_core::time::TickRate;
use concordia_destruction::{
    Breakable, DestructionSettings, Fragment, Loaded, MemorySource, destruction_system,
};
use concordia_fracture::{Mesh, Pattern, StructureSpec, fracture, structure};
use concordia_physics::{
    BodyKind, Collider, ColliderShape, ContactImpact, PhysicsRuntime, PhysicsSettings, RigidBody,
};
use concordia_scene::builtin::{Transform, Velocity};
use concordia_terra::{Phase, Schedule, SimLoop, SystemCtx, World, system};

const RECIPE: &str = "breakables/slab.breakable.ron";

fn slab() -> Mesh {
    let mut positions = Vec::new();
    for i in 0..8u32 {
        let s = |bit: u32, hi: f32| if i & bit == 0 { 0.0 } else { hi };
        positions.push([s(1, 2.0), s(2, 1.0), s(4, 1.0)]);
    }
    let triangles = vec![
        [0, 4, 6],
        [0, 6, 2],
        [1, 3, 7],
        [1, 7, 5],
        [0, 1, 5],
        [0, 5, 4],
        [2, 6, 7],
        [2, 7, 3],
        [0, 2, 3],
        [0, 3, 1],
        [4, 5, 7],
        [4, 7, 6],
    ];
    Mesh {
        positions,
        triangles,
    }
}

fn source() -> Arc<MemorySource> {
    let f = fracture(
        &slab(),
        &Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 4,
        },
        0,
    )
    .unwrap();
    let spec = StructureSpec {
        density: 1000.0,
        strength: 100.0,
        levels: Vec::new(),
        edges: Vec::new(),
    };
    let s = structure(&f, &spec).unwrap();
    Arc::new(MemorySource::default().with(
        RECIPE,
        Loaded {
            fracture: f,
            structure: s,
        },
    ))
}

/// A blow: on tick `tick`, an impact of `strength` at `local` (in the slab's mesh space).
#[derive(Clone, Copy)]
struct Blow {
    tick: u64,
    local: Vec3,
    strength: f32,
}

struct Run {
    threads: usize,
    gravity: bool,
    anchors: Vec<u32>,
    radius: f32,
    breaks_per_tick: u32,
    velocity: Velocity,
    blows: Vec<Blow>,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            threads: 1,
            gravity: false,
            anchors: Vec::new(),
            radius: 0.3,
            breaks_per_tick: 64,
            velocity: Velocity::default(),
            blows: Vec::new(),
        }
    }
}

const START: Vec3 = Vec3::new(0.0, 3.0, 0.0);

fn root() -> EntityId {
    "0193a1c2-7e40-7000-8000-0000000000aa".parse().unwrap()
}

/// Runs `ticks` ticks of physics plus destruction and returns the world.
fn run(r: &Run, ticks: u32) -> World {
    let mut settings = PhysicsSettings::default();
    if !r.gravity {
        settings.gravity = Vec3::ZERO;
    }
    let mut physics = PhysicsRuntime::new(settings, NonZeroUsize::new(r.threads).unwrap()).unwrap();
    let mut world = World::new();
    let mut ids = DeterministicIdGen::new(5);
    if r.gravity {
        let ground = world.spawn(&mut ids).unwrap();
        world
            .insert(
                ground,
                Transform {
                    translation: Vec3::new(0.0, -0.5, 0.0),
                    ..Transform::default()
                },
            )
            .unwrap();
        world.insert(ground, RigidBody::fixed()).unwrap();
        world
            .insert(
                ground,
                Collider::new(ColliderShape::Box {
                    half_extents: Vec3::new(30.0, 0.5, 30.0),
                }),
            )
            .unwrap();
    }
    world.spawn_with_id(root()).unwrap();
    world
        .insert(
            root(),
            Transform {
                translation: START,
                ..Transform::default()
            },
        )
        .unwrap();
    world
        .insert(
            root(),
            RigidBody {
                linear_damping: 0.0,
                angular_damping: 0.0,
                ..RigidBody::dynamic()
            },
        )
        .unwrap();
    world.insert(root(), r.velocity).unwrap();
    world
        .insert(
            root(),
            Breakable {
                recipe: RECIPE.into(),
                anchors: r.anchors.clone(),
                impact_radius: r.radius,
            },
        )
        .unwrap();
    let mut schedule = Schedule::new();
    let blows = r.blows.clone();
    schedule.add_in(
        Phase::PrePhysics,
        system(
            "test.blows",
            move |world: &mut World, ctx: &SystemCtx<'_>| {
                for b in blows.iter().filter(|b| b.tick == ctx.tick.0) {
                    // Hit whichever fragment holds the point, in world space.
                    let mut targets: Vec<(EntityId, Transform)> = Vec::new();
                    if let Ok(mut q) = world.query::<(&Fragment, &Transform)>() {
                        q.for_each(world, |id, (_, t)| targets.push((id, *t)));
                    }
                    targets.sort_by_key(|t| t.0);
                    for (id, t) in targets {
                        let point = t.translation + t.rotation * b.local;
                        world.signal(
                            id,
                            ContactImpact {
                                other: id,
                                strength: b.strength,
                                point,
                            },
                        );
                    }
                }
            },
        ),
    );
    schedule.add_in(
        Phase::Physics,
        system(
            "physics.step",
            move |world: &mut World, ctx: &SystemCtx<'_>| {
                physics.tick(world, ctx.dt).unwrap();
            },
        ),
    );
    schedule.add_in(
        Phase::PostPhysics,
        destruction_system(
            source(),
            DestructionSettings {
                breaks_per_tick: r.breaks_per_tick,
            },
        ),
    );
    let mut sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    let jobs = JobSystem::new(NonZeroUsize::new(r.threads).unwrap()).unwrap();
    sim.step(&mut world, &jobs, ticks);
    world
}

/// Every fragment, by its sorted pieces.
fn fragments(world: &mut World) -> BTreeMap<Vec<u32>, EntityId> {
    let mut out = BTreeMap::new();
    if let Ok(mut q) = world.query::<&Fragment>() {
        q.for_each(world, |id, f| {
            out.insert(f.pieces.clone(), id);
        });
    }
    out
}

/// A blow on the middle cut (x = 1), strong enough to break that bond only.
fn middle(tick: u64) -> Blow {
    Blow {
        tick,
        local: Vec3::new(1.0, 0.5, 0.5),
        strength: 150.0,
    }
}

#[test]
fn an_unstrained_cluster_stays_one_body() {
    let mut world = run(&Run::default(), 30);
    let all = fragments(&mut world);
    assert_eq!(all.keys().collect::<Vec<_>>(), [&vec![0, 1, 2, 3]]);
    let id = all[&vec![0, 1, 2, 3]];
    assert_eq!(id, root(), "the breakable itself is the first fragment");
    let ColliderShape::Compound { parts } = &world.get::<Collider>(id).unwrap().shape else {
        panic!("one compound body of its pieces");
    };
    assert_eq!(parts.len(), 4);
    // Its mass comes from the structure: 2 m³ at 1000 kg/m³.
    let mass = world.get::<RigidBody>(id).unwrap().mass.unwrap();
    assert!((mass - 2000.0).abs() < 1e-2, "{mass}");
}

#[test]
fn a_weak_blow_damages_without_breaking() {
    let r = Run {
        blows: vec![Blow {
            strength: 60.0,
            ..middle(5)
        }],
        ..Run::default()
    };
    let mut world = run(&r, 20);
    let all = fragments(&mut world);
    assert_eq!(all.len(), 1, "60 < 100: still whole");
    let f = world.get::<Fragment>(root()).unwrap();
    assert!(
        f.damage.iter().any(|d| *d > 0.0),
        "the bond remembers the blow"
    );
    assert!(f.broken.is_empty());
}

#[test]
fn impulse_above_threshold_splits_into_connected_components() {
    let mut r = Run {
        blows: vec![middle(5)],
        ..Run::default()
    };
    let mut world = run(&r, 20);
    let all = fragments(&mut world);
    assert_eq!(
        all.keys().cloned().collect::<Vec<_>>(),
        [vec![0, 1], vec![2, 3]],
        "only the middle bond broke"
    );
    assert!(world.get::<Fragment>(root()).is_none(), "the whole is gone");
    for (pieces, id) in &all {
        let ColliderShape::Compound { parts } = &world.get::<Collider>(*id).unwrap().shape else {
            panic!("each part is a compound of its pieces");
        };
        assert_eq!(parts.len(), pieces.len());
        let mass = world.get::<RigidBody>(*id).unwrap().mass.unwrap();
        assert!((mass - 1000.0).abs() < 1e-2, "{mass}");
    }
    // A second blow on the same spot finds nothing left to break.
    r.blows.push(middle(10));
    assert_eq!(fragments(&mut run(&r, 20)).len(), 2);
}

#[test]
fn momentum_is_conserved_across_a_split() {
    // Spinning about z (a principal axis of the slab, so ω stays constant) while drifting.
    let (v, w) = (Vec3::new(1.0, 0.5, 0.0), Vec3::new(0.0, 0.0, 2.0));
    let r = Run {
        velocity: Velocity {
            linear: v,
            angular: w,
        },
        blows: vec![middle(5)],
        ..Run::default()
    };
    // Read the parts on the tick they appear, before physics moves them again.
    let mut before = r.blows[0].tick + 1;
    let mut world = loop {
        let mut world = run(&r, before as u32);
        if fragments(&mut world).len() == 2 {
            break world;
        }
        before += 1;
        assert!(before < 20, "the split never happened");
    };
    let parts = fragments(&mut world);
    // The whole's centre of mass in mesh space is (1, 0.5, 0.5); each half's is ±0.5 m along x.
    let rotation = world.get::<Transform>(parts[&vec![0, 1]]).unwrap().rotation;
    let (mut momentum, mut mass) = (Vec3::ZERO, 0.0);
    for (pieces, id) in &parts {
        let m = world.get::<RigidBody>(*id).unwrap().mass.unwrap();
        let vel = *world.get::<Velocity>(*id).unwrap();
        let dx = if pieces[0] == 0 { -0.5 } else { 0.5 };
        let expected = v + w.cross(rotation * Vec3::new(dx, 0.0, 0.0));
        assert!(
            (vel.linear - expected).length() < 1e-3,
            "{pieces:?}: {vel:?} vs {expected}"
        );
        assert!((vel.angular - w).length() < 1e-4, "{vel:?}");
        momentum += vel.linear * m;
        mass += m;
    }
    // Equal halves: the linear momentum is the whole's, and each part moves rigidly with it,
    // so the angular momentum is the whole's too.
    assert!(
        (momentum - v * mass).length() < 1e-2,
        "{momentum} vs {}",
        v * mass
    );
}

#[test]
fn anchored_components_stay_static() {
    let r = Run {
        gravity: true,
        anchors: vec![0],
        blows: vec![middle(5)],
        ..Run::default()
    };
    let mut world = run(&r, 90);
    let parts = fragments(&mut world);
    assert_eq!(parts.len(), 2);
    let (anchored, loose) = (parts[&vec![0, 1]], parts[&vec![2, 3]]);
    assert_eq!(
        world.get::<RigidBody>(anchored).unwrap().kind,
        BodyKind::Static
    );
    assert_eq!(
        world.get::<RigidBody>(loose).unwrap().kind,
        BodyKind::Dynamic
    );
    let at = |id| world.get::<Transform>(id).unwrap().translation;
    assert_eq!(at(anchored), START, "the anchored half never moved");
    assert!(
        at(loose).y < START.y - 1.0,
        "the loose half fell: {}",
        at(loose)
    );
}

#[test]
fn break_budget_defers_excess_breaks_to_later_ticks() {
    // One wide blow over every bond, with a budget of one break a tick.
    let r = Run {
        radius: 5.0,
        breaks_per_tick: 1,
        blows: vec![Blow {
            tick: 5,
            local: Vec3::new(1.0, 0.5, 0.5),
            strength: 1000.0,
        }],
        ..Run::default()
    };
    let counts: Vec<usize> = (6..=10).map(|t| fragments(&mut run(&r, t)).len()).collect();
    assert_eq!(counts.first(), Some(&1), "{counts:?}");
    assert_eq!(counts.last(), Some(&4), "{counts:?}");
    assert!(
        counts.windows(2).all(|w| w[1] <= w[0] + 1),
        "at most one more piece a tick: {counts:?}"
    );
}

/// A fragment's id, pieces and pose bits.
type Pose = (EntityId, Vec<u32>, [u32; 3], [u32; 4]);

#[test]
fn destruction_hash_is_identical_at_1_2_and_8_threads() {
    let hash = |threads| {
        let r = Run {
            threads,
            gravity: true,
            velocity: Velocity {
                linear: Vec3::new(0.5, 0.0, 0.0),
                angular: Vec3::new(0.0, 1.0, 0.0),
            },
            blows: vec![
                middle(5),
                Blow {
                    tick: 20,
                    local: Vec3::new(0.5, 0.5, 0.5),
                    strength: 500.0,
                },
            ],
            ..Run::default()
        };
        let mut world = run(&r, 90);
        let mut parts: Vec<Pose> = Vec::new();
        if let Ok(mut q) = world.query::<(&Fragment, &Transform)>() {
            q.for_each(&mut world, |id, (f, t)| {
                parts.push((
                    id,
                    f.pieces.clone(),
                    t.translation.to_array().map(f32::to_bits),
                    Quat::to_array(&t.rotation).map(f32::to_bits),
                ))
            });
        }
        parts.sort();
        assert!(
            parts.len() > 2,
            "both blows split something: {}",
            parts.len()
        );
        parts
    };
    let one = hash(1);
    assert_eq!(one, hash(2));
    assert_eq!(one, hash(8));
    assert_eq!(one, hash(1), "and a rerun");
}
