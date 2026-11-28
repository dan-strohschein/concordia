//! The opt-in LLM seam (M4 4.12d, M9, D8): a `Plan` task asks a provider to pick one of its
//! actions; anything but a valid, timely answer fails the task so the tree's fallback runs, and
//! every applied answer is logged for replays.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use concordia_ai::{
    AiConfig, BbType, BbValue, BehaviorTree, Blackboard, Brain, BtState, LlmService,
    MockLlmProvider, MockReply, NodeKind, Spec, TaskKind, Trees, action_schema, add_ai_processors,
    insert_ai_resources,
};
use concordia_core::external::ExternalInputs;
use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_core::jobs::JobSystem;
use concordia_core::rng::SimRng;
use concordia_core::time::TickRate;
use concordia_terra::{Schedule, SimLoop, World};

/// Ask the planner (deadline 10 ticks) to choose `chase` or `flee` into `plan`; failing that,
/// set `fallback`.
fn planner() -> BehaviorTree {
    let keys = BTreeMap::from([
        ("plan".into(), BbType::Text),
        ("fallback".into(), BbType::Bool),
    ]);
    BehaviorTree::build(
        keys,
        Spec::new(NodeKind::Selector).children(vec![
            Spec::new(NodeKind::Task(TaskKind::Plan {
                goal: "deal with the intruder".into(),
                deadline_ticks: 10,
                options: vec!["chase".into(), "flee".into()],
                key: "plan".into(),
            })),
            Spec::new(NodeKind::Task(TaskKind::SetBlackboard {
                key: "fallback".into(),
                value: BbValue::Bool(true),
            })),
        ]),
    )
    .unwrap()
}

struct Setup {
    world: World,
    sim: SimLoop,
    agent: EntityId,
}

fn setup(service: LlmService) -> Setup {
    let mut world = World::new();
    world.insert_resource(SimRng::new(3));
    world.insert_resource(Trees::from_trees([("p.bt.ron".to_owned(), planner())]));
    insert_ai_resources(&mut world);
    world.insert_resource(service);
    let mut schedule = Schedule::new();
    add_ai_processors(&mut schedule, AiConfig::default());
    let sim = SimLoop::new(TickRate::HZ_60, schedule, &mut world).unwrap();
    let agent = world.spawn(&mut DeterministicIdGen::new(4)).unwrap();
    world
        .insert(
            agent,
            Brain {
                tree: "p.bt.ron".into(),
            },
        )
        .unwrap();
    world
        .insert(agent, Blackboard::for_tree(&planner()))
        .unwrap();
    world.insert(agent, BtState::default()).unwrap();
    Setup { world, sim, agent }
}

impl Setup {
    fn step(&mut self, ticks: u32) {
        self.sim.step(
            &mut self.world,
            &JobSystem::new(NonZeroUsize::MIN).unwrap(),
            ticks,
        );
    }

    fn key(&self, key: &str) -> Option<BbValue> {
        self.world
            .get::<Blackboard>(self.agent)
            .unwrap()
            .get(key)
            .cloned()
    }
}

fn mock(replies: Vec<MockReply>) -> LlmService {
    LlmService::with_provider(Arc::new(MockLlmProvider::new(replies)))
}

#[test]
fn a_valid_answer_writes_the_chosen_action() {
    let mut s = setup(mock(vec![MockReply::ready(
        2,
        r#"{"action": "flee", "reason": "outnumbered"}"#,
    )]));
    s.step(1);
    assert_eq!(s.key("plan"), None, "still waiting");
    s.step(4);
    assert_eq!(s.key("plan"), Some(BbValue::Text("flee".into())));
    assert_eq!(s.key("fallback"), None);
    let log = s.world.resource::<ExternalInputs>().unwrap();
    assert_eq!(log.applied.len(), 1, "every applied answer is logged");
    assert!(log.applied[0].source.starts_with("llm:"));
}

#[test]
fn llm_planner_output_validated_against_action_schema() {
    for bad in [r#"{"action": "dance"}"#, "not json", r#"{"act": "flee"}"#] {
        let mut s = setup(mock(vec![MockReply::ready(0, bad)]));
        s.step(3);
        assert_eq!(s.key("plan"), None, "{bad}");
        assert_eq!(
            s.key("fallback"),
            Some(BbValue::Bool(true)),
            "the tree's fallback runs for {bad}"
        );
    }
    // The schema the provider gets names exactly the task's actions.
    let schema = action_schema(&["chase".into(), "flee".into()]);
    assert_eq!(
        schema["properties"]["action"]["enum"],
        serde_json::json!(["chase", "flee"])
    );
}

#[test]
fn llm_timeout_falls_back_to_behavior_tree() {
    let mut s = setup(mock(vec![MockReply::ready(500, r#"{"action": "chase"}"#)]));
    s.step(9);
    assert_eq!(s.key("fallback"), None, "within the deadline");
    s.step(4);
    assert_eq!(s.key("fallback"), Some(BbValue::Bool(true)));
    // The late answer is never applied (or logged). The tree then plans again; the mock has no
    // second reply, and that failure is logged like any answer, so replays reproduce it.
    let log = &s.world.resource::<ExternalInputs>().unwrap().applied;
    assert!(log.iter().all(|r| !r.payload.contains("chase")), "{log:?}");
}

#[test]
fn without_a_provider_the_fallback_runs_at_once() {
    let mut s = setup(LlmService::default());
    s.step(1);
    assert_eq!(s.key("fallback"), Some(BbValue::Bool(true)));
}

#[test]
fn played_back_answers_need_no_provider() {
    // Record with the mock, then play the log back with no provider: the same choice, same tick.
    let mut live = setup(mock(vec![MockReply::ready(3, r#"{"action": "chase"}"#)]));
    live.step(8);
    let records = live
        .world
        .resource::<ExternalInputs>()
        .unwrap()
        .applied
        .clone();
    let mut replayed = setup(LlmService::default());
    replayed
        .world
        .insert_resource(ExternalInputs::playing(records));
    replayed.step(8);
    assert_eq!(replayed.key("plan"), Some(BbValue::Text("chase".into())));
    assert_eq!(
        replayed.world.state_hash().unwrap(),
        live.world.state_hash().unwrap()
    );
}
