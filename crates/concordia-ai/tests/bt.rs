//! Behavior trees as a pure runtime (M4 4.12a, M2–M4): Colledanchise & Ögren semantics for the
//! composites, memory and aborts, decorators, and the decisions they raise.

use std::collections::BTreeMap;

use concordia_ai::{
    Abort, BbType, BbValue, BehaviorTree, Blackboard, BtState, CmpOp, Decision, NodeKind,
    ParallelPolicy, Spec, Status, TaskHost, TaskKind, Ticker,
};
use concordia_core::rng::SimRng;

/// A host whose tasks answer from a script: each `Custom` task's name maps to the statuses it
/// returns on successive updates (the last repeats).
#[derive(Default)]
struct Host {
    script: BTreeMap<String, Vec<Status>>,
    calls: BTreeMap<String, usize>,
    log: Vec<String>,
}

impl Host {
    fn with(script: &[(&str, &[Status])]) -> Self {
        Self {
            script: script
                .iter()
                .map(|(n, s)| ((*n).to_owned(), s.to_vec()))
                .collect(),
            ..Self::default()
        }
    }
}

impl TaskHost for Host {
    fn start(&mut self, _: u32, task: &TaskKind, _: &mut Blackboard) -> Status {
        let TaskKind::Custom { name } = task else {
            return Status::Success;
        };
        self.log.push(format!("start {name}"));
        self.update(0, task, &mut Blackboard::default())
    }

    fn update(&mut self, _: u32, task: &TaskKind, _: &mut Blackboard) -> Status {
        let TaskKind::Custom { name } = task else {
            return Status::Success;
        };
        let k = self.calls.entry(name.clone()).or_default();
        let statuses = &self.script[name];
        let s = statuses[(*k).min(statuses.len() - 1)];
        *k += 1;
        s
    }

    fn abort(&mut self, _: u32, task: &TaskKind) {
        if let TaskKind::Custom { name } = task {
            self.log.push(format!("abort {name}"));
        }
    }
}

fn task(name: &str) -> Spec {
    Spec::new(NodeKind::Task(TaskKind::Custom { name: name.into() }))
}

fn tree(root: Spec) -> BehaviorTree {
    BehaviorTree::build(
        BTreeMap::from([("alert".into(), BbType::Bool), ("hp".into(), BbType::Float)]),
        root,
    )
    .unwrap()
}

struct Run {
    tree: BehaviorTree,
    state: BtState,
    bb: Blackboard,
    host: Host,
    rng: SimRng,
    decisions: Vec<Decision>,
}

impl Run {
    fn new(root: Spec, host: Host) -> Self {
        let tree = tree(root);
        let state = BtState::new(&tree);
        let bb = Blackboard::for_tree(&tree);
        Self {
            tree,
            state,
            bb,
            host,
            rng: SimRng::new(7),
            decisions: Vec::new(),
        }
    }

    fn tick(&mut self) -> Status {
        Ticker {
            tree: &self.tree,
            state: &mut self.state,
            bb: &mut self.bb,
            host: &mut self.host,
            rng: &mut self.rng,
            dt: 0.1,
            decisions: &mut self.decisions,
        }
        .tick()
    }
}

use Status::{Failure as F, Running as R, Success as S};

#[test]
fn bt_sequence_selector_parallel_semantics() {
    // (composite, child scripts, expected first tick, expected second tick)
    let cases: Vec<(NodeKind, Vec<&[Status]>, Status, Status)> = vec![
        (NodeKind::Sequence, vec![&[S], &[S]], S, S),
        (NodeKind::Sequence, vec![&[S], &[F]], F, F),
        (NodeKind::Sequence, vec![&[F], &[S]], F, F),
        (NodeKind::Sequence, vec![&[R, S], &[S]], R, S),
        (NodeKind::Selector, vec![&[F], &[S]], S, S),
        (NodeKind::Selector, vec![&[F], &[F]], F, F),
        (NodeKind::Selector, vec![&[S], &[F]], S, S),
        (NodeKind::Selector, vec![&[F], &[R, F]], R, F),
        (
            NodeKind::Parallel {
                policy: ParallelPolicy::All,
            },
            vec![&[S], &[R, S]],
            R,
            S,
        ),
        (
            NodeKind::Parallel {
                policy: ParallelPolicy::All,
            },
            vec![&[F], &[R]],
            F,
            F,
        ),
        (
            NodeKind::Parallel {
                policy: ParallelPolicy::One,
            },
            vec![&[R], &[S]],
            S,
            S,
        ),
        (
            NodeKind::Parallel {
                policy: ParallelPolicy::One,
            },
            vec![&[F], &[F]],
            F,
            F,
        ),
    ];
    for (k, (kind, scripts, first, second)) in cases.into_iter().enumerate() {
        let names: Vec<String> = (0..scripts.len()).map(|i| format!("t{i}")).collect();
        let script: Vec<(&str, &[Status])> =
            names.iter().map(String::as_str).zip(scripts).collect();
        let mut run = Run::new(
            Spec::new(kind.clone()).children(names.iter().map(|n| task(n)).collect()),
            Host::with(&script),
        );
        assert_eq!(run.tick(), first, "case {k} ({kind:?}) first tick");
        assert_eq!(run.tick(), second, "case {k} ({kind:?}) second tick");
    }
}

#[test]
fn a_sequence_remembers_its_running_child() {
    // The first child succeeded once; while the second runs, the first isn't started again.
    let mut run = Run::new(
        Spec::new(NodeKind::Sequence).children(vec![task("a"), task("b")]),
        Host::with(&[("a", &[S]), ("b", &[R, R, S])]),
    );
    assert_eq!(run.tick(), R);
    assert_eq!(run.tick(), R);
    assert_eq!(run.tick(), S);
    assert_eq!(run.host.log, ["start a", "start b"]);
}

fn alert_is(value: bool, abort: Abort, child: Spec) -> Spec {
    Spec::new(NodeKind::Condition {
        key: "alert".into(),
        op: CmpOp::Eq,
        value: BbValue::Bool(value),
        abort,
    })
    .children(vec![child])
}

#[test]
fn bt_condition_abort_lower_priority_on_blackboard_change() {
    // Selector: [alert? → flee] then patrol. Patrol runs until `alert` turns true; then the
    // higher-priority branch aborts it.
    let root = Spec::new(NodeKind::Selector).children(vec![
        alert_is(true, Abort::Lower, task("flee")),
        task("patrol"),
    ]);
    let mut run = Run::new(root, Host::with(&[("flee", &[R]), ("patrol", &[R])]));
    run.bb.set("alert", BbValue::Bool(false)).unwrap();
    assert_eq!(run.tick(), R);
    assert_eq!(run.tick(), R);
    run.bb.set("alert", BbValue::Bool(true)).unwrap();
    assert_eq!(run.tick(), R);
    assert_eq!(run.host.log, ["start patrol", "abort patrol", "start flee"]);
    // Without `Lower`, the running patrol isn't interrupted.
    let root = Spec::new(NodeKind::Selector).children(vec![
        alert_is(true, Abort::None, task("flee")),
        task("patrol"),
    ]);
    let mut run = Run::new(root, Host::with(&[("flee", &[R]), ("patrol", &[R])]));
    run.bb.set("alert", BbValue::Bool(false)).unwrap();
    run.tick();
    run.bb.set("alert", BbValue::Bool(true)).unwrap();
    run.tick();
    assert_eq!(run.host.log, ["start patrol"]);
}

#[test]
fn a_condition_aborts_its_own_child_when_it_stops_holding() {
    let mut run = Run::new(
        alert_is(false, Abort::SelfOnly, task("rest")),
        Host::with(&[("rest", &[R])]),
    );
    run.bb.set("alert", BbValue::Bool(false)).unwrap();
    assert_eq!(run.tick(), R);
    run.bb.set("alert", BbValue::Bool(true)).unwrap();
    assert_eq!(run.tick(), F);
    assert_eq!(run.host.log, ["start rest", "abort rest"]);
}

#[test]
fn decorators_invert_loop_cool_down_and_time_out() {
    let mut run = Run::new(
        Spec::new(NodeKind::Inverter).children(vec![task("t")]),
        Host::with(&[("t", &[F])]),
    );
    assert_eq!(run.tick(), S);
    // Loop 3 times, one success per tick.
    let mut run = Run::new(
        Spec::new(NodeKind::Loop { count: 3 }).children(vec![task("t")]),
        Host::with(&[("t", &[S])]),
    );
    assert_eq!([run.tick(), run.tick(), run.tick()], [R, R, S]);
    // Cooldown 0.25 s (dt 0.1): after a success, the next two ticks fail, then it runs again.
    let mut run = Run::new(
        Spec::new(NodeKind::Cooldown { seconds: 0.25 }).children(vec![task("t")]),
        Host::with(&[("t", &[S])]),
    );
    assert_eq!(
        [run.tick(), run.tick(), run.tick(), run.tick()],
        [S, F, F, S]
    );
    // Time limit 0.25 s on a task that never ends: time counts from the tick it started (as
    // `Wait`), so it is aborted on the fourth tick (0.3 s after the start).
    let mut run = Run::new(
        Spec::new(NodeKind::TimeLimit { seconds: 0.25 }).children(vec![task("t")]),
        Host::with(&[("t", &[R])]),
    );
    assert_eq!(
        [run.tick(), run.tick(), run.tick(), run.tick()],
        [R, R, R, F]
    );
    assert_eq!(run.host.log, ["start t", "abort t"]);
}

#[test]
fn wait_and_set_blackboard_are_built_in() {
    let root = Spec::new(NodeKind::Sequence).children(vec![
        Spec::new(NodeKind::Task(TaskKind::Wait { seconds: 0.2 })),
        Spec::new(NodeKind::Task(TaskKind::SetBlackboard {
            key: "hp".into(),
            value: BbValue::Float(5.0),
        })),
    ]);
    let mut run = Run::new(root, Host::default());
    assert_eq!([run.tick(), run.tick(), run.tick()], [R, R, S]);
    assert_eq!(run.bb.get("hp"), Some(&BbValue::Float(5.0)));
}

#[test]
fn choices_raise_decisions() {
    let root = Spec::new(NodeKind::Selector).children(vec![
        alert_is(true, Abort::Lower, task("flee")),
        task("patrol"),
    ]);
    let mut run = Run::new(root, Host::with(&[("flee", &[R]), ("patrol", &[R])]));
    run.bb.set("alert", BbValue::Bool(false)).unwrap();
    run.tick();
    run.tick();
    run.bb.set("alert", BbValue::Bool(true)).unwrap();
    run.tick();
    let choices: Vec<(u32, u32)> = run
        .decisions
        .iter()
        .filter(|d| d.node == 0)
        .map(|d| (d.node, d.choice))
        .collect();
    assert_eq!(
        choices,
        [(0, 1), (0, 0)],
        "the selector chose patrol, then flee: {:?}",
        run.decisions
    );
}

#[test]
fn trees_build_flat_and_validate() {
    let t = tree(Spec::new(NodeKind::Sequence).children(vec![
        task("a"),
        Spec::new(NodeKind::Inverter).children(vec![task("b")]),
    ]));
    assert_eq!(t.nodes.len(), 4);
    assert_eq!(
        t.nodes.iter().map(|n| n.end).collect::<Vec<_>>(),
        [4, 2, 4, 4]
    );
    // Decorators take exactly one child; tasks none; conditions name declared keys.
    assert!(BehaviorTree::build(BTreeMap::new(), Spec::new(NodeKind::Inverter)).is_err());
    assert!(BehaviorTree::build(BTreeMap::new(), task("a").children(vec![task("b")])).is_err());
    assert!(BehaviorTree::build(BTreeMap::new(), alert_is(true, Abort::None, task("a"))).is_err());
}
