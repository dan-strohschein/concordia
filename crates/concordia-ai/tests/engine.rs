//! Brains in a world (M4 4.12b, M3, M4, M6, M7): the `ai.think` processor ticks each agent's tree
//! over its blackboard, drives movement and animation through components, raises decisions as
//! signals, and stays deterministic and within its per-tick budget.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use concordia_ai::{
    AiConfig, AiDecision, AiStats, BbType, BbValue, BehaviorTree, Blackboard, Brain, BtState,
    NodeKind, Spec, TaskKind, Trees, add_ai_processors,
};
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::math::Vec3;
use concordia_core::rng::SimRng;
use concordia_core::time::TickRate;
use concordia_nav::{NavAgent, NavArrived};
use concordia_scene::builtin::Transform;
use concordia_terra::{Schedule, SimLoop, World, WorldHash};

fn jobs(n: usize) -> JobSystem {
    JobSystem::new(NonZeroUsize::new(n).unwrap()).unwrap()
}

/// Walk to `goal`, then mark `done`.
fn walker() -> BehaviorTree {
    let keys = BTreeMap::from([("goal".into(), BbType::Vec3), ("done".into(), BbType::Bool)]);
    BehaviorTree::build(
        keys,
        Spec::new(NodeKind::Sequence).children(vec![
            Spec::new(NodeKind::Task(TaskKind::MoveTo { key: "goal".into() })),
            Spec::new(NodeKind::Task(TaskKind::SetBlackboard {
                key: "done".into(),
                value: BbValue::Bool(true),
            })),
        ]),
    )
    .unwrap()
}

/// Forever: wait a random 0.1–0.3 s.
fn fidget() -> BehaviorTree {
    let wait = |s| Spec::new(NodeKind::Task(TaskKind::Wait { seconds: s }));
    BehaviorTree::build(
        BTreeMap::new(),
        Spec::new(NodeKind::Loop { count: 0 }).children(vec![
            Spec::new(NodeKind::RandomSelector {
                weights: vec![1.0, 1.0, 1.0],
            })
            .children(vec![wait(0.1), wait(0.2), wait(0.3)]),
        ]),
    )
    .unwrap()
}

struct Setup {
    world: World,
    sim: SimLoop,
    ids: DeterministicIdGen,
}

fn setup(trees: &[(&str, BehaviorTree)], config: AiConfig) -> Setup {
    let mut world = World::new();
    world.insert_resource(SimRng::new(11));
    world.insert_resource(Trees::from_trees(
        trees.iter().map(|(n, t)| ((*n).to_owned(), t.clone())),
    ));
    concordia_ai::insert_ai_resources(&mut world);
    let mut schedule = Schedule::new();
    add_ai_processors(&mut schedule, config);
    let sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    Setup {
        world,
        sim,
        ids: DeterministicIdGen::new(5),
    }
}

impl Setup {
    fn agent(&mut self, tree: &str, bb: Blackboard) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        self.world.insert(id, Brain { tree: tree.into() }).unwrap();
        self.world.insert(id, bb).unwrap();
        self.world.insert(id, BtState::default()).unwrap();
        self.world.insert(id, Transform::default()).unwrap();
        id
    }

    fn step(&mut self, ticks: u32, threads: usize) {
        self.sim.step(&mut self.world, &jobs(threads), ticks);
    }
}

#[test]
fn a_brain_moves_its_agent_and_waits_for_arrival() {
    let mut s = setup(&[("walker.bt.ron", walker())], AiConfig::default());
    let mut bb = Blackboard::for_tree(&walker());
    bb.set("goal", BbValue::Vec3([4.0, 0.0, 2.0])).unwrap();
    let e = s.agent("walker.bt.ron", bb);
    s.world.insert(e, NavAgent::default()).unwrap();
    s.step(2, 1);
    assert_eq!(
        s.world.get::<NavAgent>(e).unwrap().target,
        Some(Vec3::new(4.0, 0.0, 2.0))
    );
    assert_eq!(
        s.world.get::<Blackboard>(e).unwrap().get("done"),
        None,
        "still walking"
    );
    s.world.signal(
        e,
        NavArrived {
            target: Vec3::new(4.0, 0.0, 2.0),
            position: Vec3::new(4.0, 0.0, 2.0),
            partial: false,
        },
    );
    s.step(2, 1);
    assert_eq!(
        s.world.get::<Blackboard>(e).unwrap().get("done"),
        Some(&BbValue::Bool(true))
    );
}

/// Navigation raises `NavArrived` only on reaching a target, so an agent already within its
/// arrival distance would wait forever (found by the Snuffer, 4.13a): such a MoveTo succeeds at once.
#[test]
fn moving_to_where_the_agent_already_is_succeeds_at_once() {
    let mut s = setup(&[("walker.bt.ron", walker())], AiConfig::default());
    let mut bb = Blackboard::for_tree(&walker());
    bb.set("goal", BbValue::Vec3([0.3, 0.0, 0.2])).unwrap();
    let e = s.agent("walker.bt.ron", bb);
    s.world.insert(e, NavAgent::default()).unwrap();
    s.step(2, 1);
    assert_eq!(
        s.world.get::<Blackboard>(e).unwrap().get("done"),
        Some(&BbValue::Bool(true)),
        "0.36 m away, inside the default 0.5 m"
    );
}

#[test]
fn moving_without_a_nav_agent_fails_the_task() {
    let mut s = setup(&[("walker.bt.ron", walker())], AiConfig::default());
    let mut bb = Blackboard::for_tree(&walker());
    bb.set("goal", BbValue::Vec3([4.0, 0.0, 2.0])).unwrap();
    let e = s.agent("walker.bt.ron", bb);
    s.step(3, 1);
    assert_eq!(s.world.get::<Blackboard>(e).unwrap().get("done"), None);
}

#[test]
fn decisions_are_raised_as_signals() {
    let mut s = setup(&[("fidget.bt.ron", fidget())], AiConfig::default());
    let e = s.agent("fidget.bt.ron", Blackboard::default());
    s.step(1, 1);
    let raised: Vec<AiDecision> = s
        .world
        .pending_signals::<AiDecision>()
        .into_iter()
        .filter(|(t, _)| *t == e)
        .map(|(_, d)| d.clone())
        .collect();
    assert!(raised.iter().any(|d| d.reason == "random"), "{raised:?}");
    assert!(
        s.world.pending_signal_count("ai.Decision") > 0,
        "assertable as event(\"ai.Decision\")"
    );
}

fn crowd(threads: usize) -> WorldHash {
    let mut s = setup(
        &[("fidget.bt.ron", fidget())],
        AiConfig {
            agents_per_tick: 64,
            ..AiConfig::default()
        },
    );
    for _ in 0..500 {
        s.agent("fidget.bt.ron", Blackboard::default());
    }
    s.step(90, threads);
    s.world.state_hash().unwrap()
}

#[test]
fn bt_tick_is_deterministic_across_thread_counts() {
    let one = crowd(1);
    assert_eq!(one, crowd(1), "runs repeat");
    assert_eq!(one, crowd(4), "thread count doesn't matter");
}

#[test]
fn bt_time_slicing_respects_budget() {
    let mut s = setup(
        &[("fidget.bt.ron", fidget())],
        AiConfig {
            agents_per_tick: 64,
            ..AiConfig::default()
        },
    );
    for _ in 0..500 {
        s.agent("fidget.bt.ron", Blackboard::default());
    }
    let mut most = 0;
    let mut total_us = 0u128;
    for _ in 0..60 {
        s.step(1, 1);
        let stats = s.world.resource::<AiStats>().unwrap();
        most = most.max(stats.thought);
        total_us += stats.micros as u128;
    }
    assert!(
        most <= 64,
        "at most agents_per_tick re-think a tick: {most}"
    );
    // Every agent re-thinks within ceil(500 / 64) = 8 ticks.
    let states: Vec<f32> = s
        .world
        .entity_ids_sorted()
        .into_iter()
        .filter_map(|id| s.world.get::<BtState>(id).map(|b| b.time))
        .collect();
    assert_eq!(states.len(), 500);
    assert!(states.iter().all(|t| *t > 0.0), "every agent has run");
    println!(
        "ai.think: {:.3} ms per tick on average for 500 agents (target 1.0 ms on the reference machine)",
        total_us as f64 / 60.0 / 1000.0
    );
}
