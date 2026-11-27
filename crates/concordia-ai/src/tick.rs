//! Ticking a behavior tree (M4 4.12, M3–M4). Composites keep memory (a running child resumes);
//! conditions re-check per their `abort` mode, so a blackboard change interrupts what it should
//! (Colledanchise & Ögren semantics; UE-style abort modes). Choices are recorded as decisions.

use concordia_core::rng::SimRng;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::utility::{Selection, UtilityOption, choose};
use crate::{Abort, BbValue, BehaviorTree, Blackboard, CmpOp, NodeKind, ParallelPolicy, TaskKind};

/// A node's result this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum Status {
    /// Done, well.
    Success,
    /// Done, badly.
    Failure,
    /// Not done yet.
    Running,
}

/// One node's memory between ticks.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
pub struct NodeMem {
    /// It is running (resumes next tick).
    pub running: bool,
    /// The running child (composites).
    pub cursor: u32,
    /// When it started (waits, time limits), in tree seconds.
    pub start: f32,
    /// When it may run again (cooldowns).
    pub until: f32,
    /// Iterations (loops).
    pub count: u32,
    /// The last chosen child (selectors), for decisions.
    pub choice: Option<u32>,
    /// A parallel child's finished result this activation: 0 none, 1 success, 2 failure.
    pub done: u8,
}

/// An agent's tree state: its clock and each node's memory.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(name = "ai.BtState", category = "AI", default)]
pub struct BtState {
    /// Seconds the tree has run.
    pub time: f32,
    /// Per node, in tree order.
    pub nodes: Vec<NodeMem>,
}

impl BtState {
    /// A fresh state for `tree`.
    pub fn new(tree: &BehaviorTree) -> Self {
        Self {
            time: 0.0,
            nodes: vec![NodeMem::default(); tree.nodes.len()],
        }
    }

    /// The running path: every running node, in tree order.
    pub fn running(&self) -> Vec<u32> {
        (0..self.nodes.len() as u32)
            .filter(|&i| self.nodes[i as usize].running)
            .collect()
    }
}

/// A choice the tree made (raised as `ai.Decision` by the engine, D8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct Decision {
    /// The choosing node (a selector), or the task that started.
    pub node: u32,
    /// The child chosen (0 for a task).
    pub choice: u32,
    /// Why: `selector`, `utility`, `random` or `task`.
    pub reason: String,
}

/// Runs the tasks the tree doesn't run itself (movement, animation, signals: the engine).
pub trait TaskHost {
    /// Starts task node `node`; its first status.
    fn start(&mut self, node: u32, task: &TaskKind, bb: &mut Blackboard) -> Status;
    /// Continues a running task.
    fn update(&mut self, node: u32, task: &TaskKind, bb: &mut Blackboard) -> Status;
    /// Stops a running task early.
    fn abort(&mut self, node: u32, task: &TaskKind);
}

/// One tick of one agent's tree.
pub struct Ticker<'a> {
    /// The tree.
    pub tree: &'a BehaviorTree,
    /// Its state.
    pub state: &'a mut BtState,
    /// Its blackboard.
    pub bb: &'a mut Blackboard,
    /// What runs the tasks.
    pub host: &'a mut dyn TaskHost,
    /// Seeded randomness.
    pub rng: &'a mut SimRng,
    /// Seconds this tick.
    pub dt: f32,
    /// Choices made this tick.
    pub decisions: &'a mut Vec<Decision>,
}

const EPS: f32 = 1e-6;

impl Ticker<'_> {
    /// Ticks the tree from its root.
    pub fn tick(&mut self) -> Status {
        if self.state.nodes.len() != self.tree.nodes.len() {
            *self.state = BtState::new(self.tree);
        }
        self.state.time += self.dt;
        self.exec(0)
    }

    fn mem(&mut self, i: u32) -> &mut NodeMem {
        &mut self.state.nodes[i as usize]
    }

    fn children(&self, i: u32) -> Vec<u32> {
        self.tree.children(i).collect()
    }

    /// Ends node `i`'s activation (cooldowns and last choices are kept).
    fn finish(&mut self, i: u32) {
        let m = self.mem(i);
        m.running = false;
        m.cursor = 0;
        m.count = 0;
    }

    /// Stops every running node of `i`'s subtree.
    fn abort(&mut self, i: u32) {
        let tree = self.tree;
        for j in i..tree.nodes[i as usize].end {
            if self.state.nodes[j as usize].running {
                if let NodeKind::Task(task) = &tree.nodes[j as usize].kind
                    && !builtin(task)
                {
                    self.host.abort(j, task);
                }
                self.finish(j);
            }
            self.mem(j).done = 0;
        }
    }

    fn decide(&mut self, node: u32, choice: u32, reason: &str) {
        if self.mem(node).choice != Some(choice) {
            self.mem(node).choice = Some(choice);
            self.decisions.push(Decision {
                node,
                choice,
                reason: reason.to_owned(),
            });
        }
    }

    fn exec(&mut self, i: u32) -> Status {
        let tree = self.tree;
        match &tree.nodes[i as usize].kind {
            NodeKind::Sequence => self.sequence(i),
            NodeKind::Selector => self.selector(i),
            NodeKind::Parallel { policy } => self.parallel(i, *policy),
            NodeKind::UtilitySelector { options, selection } => self.utility(i, options, selection),
            NodeKind::RandomSelector { weights } => self.random(i, weights),
            NodeKind::Condition { abort, .. } => self.condition(i, *abort),
            NodeKind::Cooldown { seconds } => self.cooldown(i, *seconds),
            NodeKind::Loop { count } => self.repeat(i, *count),
            NodeKind::TimeLimit { seconds } => self.time_limit(i, *seconds),
            NodeKind::Inverter => match self.exec(i + 1) {
                Status::Success => Status::Failure,
                Status::Failure => Status::Success,
                Status::Running => Status::Running,
            },
            NodeKind::Task(task) => self.task(i, task),
        }
    }

    /// Children in order until one fails; a running child resumes.
    fn sequence(&mut self, i: u32) -> Status {
        let children = self.children(i);
        let mut k = if self.mem(i).running {
            self.mem(i).cursor as usize
        } else {
            0
        };
        while k < children.len() {
            match self.exec(children[k]) {
                Status::Running => {
                    let m = self.mem(i);
                    m.running = true;
                    m.cursor = k as u32;
                    return Status::Running;
                }
                Status::Failure => {
                    self.finish(i);
                    return Status::Failure;
                }
                Status::Success => k += 1,
            }
        }
        self.finish(i);
        Status::Success
    }

    /// Children in order until one succeeds; a higher-priority `Lower` condition that starts to
    /// hold interrupts the running child.
    fn selector(&mut self, i: u32) -> Status {
        let children = self.children(i);
        let mut k = if self.mem(i).running {
            self.mem(i).cursor as usize
        } else {
            0
        };
        if self.mem(i).running
            && let Some(j) = (0..k).find(|&j| self.lower_abort_holds(children[j]))
        {
            self.abort(children[k]);
            k = j;
        }
        while k < children.len() {
            match self.exec(children[k]) {
                Status::Failure => k += 1,
                status => {
                    self.decide(i, k as u32, "selector");
                    if status == Status::Running {
                        let m = self.mem(i);
                        m.running = true;
                        m.cursor = k as u32;
                    } else {
                        self.finish(i);
                    }
                    return status;
                }
            }
        }
        self.finish(i);
        Status::Failure
    }

    fn lower_abort_holds(&self, c: u32) -> bool {
        matches!(
            self.tree.nodes[c as usize].kind,
            NodeKind::Condition {
                abort: Abort::Lower,
                ..
            }
        ) && self.holds(c)
    }

    /// Every child each tick until the policy decides.
    fn parallel(&mut self, i: u32, policy: ParallelPolicy) -> Status {
        let children = self.children(i);
        let (mut succeeded, mut failed) = (0, 0);
        for &c in &children {
            let result = match self.mem(c).done {
                1 => Status::Success,
                2 => Status::Failure,
                _ => self.exec(c),
            };
            match result {
                Status::Success => {
                    self.mem(c).done = 1;
                    succeeded += 1;
                }
                Status::Failure => {
                    self.mem(c).done = 2;
                    failed += 1;
                }
                Status::Running => {}
            }
        }
        let n = children.len();
        let outcome = match policy {
            ParallelPolicy::All if failed > 0 => Some(Status::Failure),
            ParallelPolicy::All if succeeded == n => Some(Status::Success),
            ParallelPolicy::One if succeeded > 0 => Some(Status::Success),
            ParallelPolicy::One if failed == n => Some(Status::Failure),
            _ => None,
        };
        match outcome {
            Some(status) => {
                for &c in &children {
                    self.abort(c);
                }
                self.finish(i);
                status
            }
            None => {
                self.mem(i).running = true;
                Status::Running
            }
        }
    }

    /// The best-scoring child, chosen on entry.
    fn utility(&mut self, i: u32, options: &[UtilityOption], selection: &Selection) -> Status {
        let children = self.children(i);
        let k = if self.mem(i).running {
            self.mem(i).cursor as usize
        } else {
            let current = self.mem(i).choice.map(|c| c as usize);
            let Some(choice) = choose(options, self.bb, selection, current, self.rng) else {
                return Status::Failure;
            };
            self.decide(i, choice.index as u32, "utility");
            choice.index
        };
        self.run_chosen(i, children[k], k)
    }

    /// A child drawn by weight on entry.
    fn random(&mut self, i: u32, weights: &[f32]) -> Status {
        let children = self.children(i);
        let k = if self.mem(i).running {
            self.mem(i).cursor as usize
        } else {
            let total: f32 = weights.iter().sum();
            if total <= 0.0 {
                return Status::Failure;
            }
            let mut r = self.rng.next_f32() * total;
            let k = weights
                .iter()
                .position(|w| {
                    r -= w;
                    r < 0.0
                })
                .unwrap_or(weights.len() - 1);
            // A fresh draw is always a decision, even if it repeats the last.
            self.mem(i).choice = None;
            self.decide(i, k as u32, "random");
            k
        };
        self.run_chosen(i, children[k], k)
    }

    fn run_chosen(&mut self, i: u32, child: u32, k: usize) -> Status {
        let status = self.exec(child);
        let m = self.mem(i);
        m.running = status == Status::Running;
        m.cursor = k as u32;
        if status != Status::Running {
            self.finish(i);
        }
        status
    }

    /// Whether condition node `c` holds on the blackboard now.
    fn holds(&self, c: u32) -> bool {
        let NodeKind::Condition { key, op, value, .. } = &self.tree.nodes[c as usize].kind else {
            return false;
        };
        compare(self.bb.get(key), *op, value)
    }

    fn condition(&mut self, i: u32, abort: Abort) -> Status {
        let holds = self.holds(i);
        if self.mem(i).running {
            if abort == Abort::SelfOnly && !holds {
                self.abort(i + 1);
                self.finish(i);
                return Status::Failure;
            }
        } else if !holds {
            return Status::Failure;
        }
        let status = self.exec(i + 1);
        self.mem(i).running = status == Status::Running;
        status
    }

    fn cooldown(&mut self, i: u32, seconds: f32) -> Status {
        let now = self.state.time;
        if !self.mem(i).running && now + EPS < self.mem(i).until {
            return Status::Failure;
        }
        let status = self.exec(i + 1);
        let m = self.mem(i);
        m.running = status == Status::Running;
        if status != Status::Running {
            m.until = now + seconds;
        }
        status
    }

    fn repeat(&mut self, i: u32, count: u32) -> Status {
        match self.exec(i + 1) {
            Status::Running => {
                self.mem(i).running = true;
                Status::Running
            }
            Status::Failure => {
                self.finish(i);
                Status::Failure
            }
            Status::Success => {
                self.mem(i).count += 1;
                if count > 0 && self.mem(i).count >= count {
                    self.finish(i);
                    return Status::Success;
                }
                self.mem(i).running = true;
                Status::Running
            }
        }
    }

    fn time_limit(&mut self, i: u32, seconds: f32) -> Status {
        let now = self.state.time;
        if !self.mem(i).running {
            self.mem(i).start = now;
        } else if now - self.mem(i).start + EPS >= seconds {
            self.abort(i + 1);
            self.finish(i);
            return Status::Failure;
        }
        let status = self.exec(i + 1);
        self.mem(i).running = status == Status::Running;
        status
    }

    fn task(&mut self, i: u32, task: &TaskKind) -> Status {
        let now = self.state.time;
        let starting = !self.mem(i).running;
        if starting {
            self.mem(i).start = now;
            self.decisions.push(Decision {
                node: i,
                choice: 0,
                reason: "task".into(),
            });
        }
        let status = match task {
            TaskKind::Wait { seconds } => match now - self.mem(i).start + EPS >= *seconds {
                true => Status::Success,
                false => Status::Running,
            },
            TaskKind::SetBlackboard { key, value } => match self.bb.set(key, value.clone()) {
                Ok(()) => Status::Success,
                Err(_) => Status::Failure,
            },
            TaskKind::Succeed => Status::Success,
            TaskKind::Fail => Status::Failure,
            _ if starting => self.host.start(i, task, self.bb),
            _ => self.host.update(i, task, self.bb),
        };
        self.mem(i).running = status == Status::Running;
        status
    }
}

/// Tasks the tree runs itself.
fn builtin(task: &TaskKind) -> bool {
    matches!(
        task,
        TaskKind::Wait { .. } | TaskKind::SetBlackboard { .. } | TaskKind::Succeed | TaskKind::Fail
    )
}

/// `value_now op value` (unset compares only with `IsSet`/`NotSet`/`Eq None`/`Ne`).
fn compare(now: Option<&BbValue>, op: CmpOp, value: &BbValue) -> bool {
    let none = BbValue::None;
    let now = now.unwrap_or(&none);
    let numbers = || now.number().zip(value.number());
    match op {
        CmpOp::Eq => now == value,
        CmpOp::Ne => now != value,
        CmpOp::IsSet => *now != BbValue::None,
        CmpOp::NotSet => *now == BbValue::None,
        CmpOp::Lt => numbers().is_some_and(|(a, b)| a < b),
        CmpOp::Le => numbers().is_some_and(|(a, b)| a <= b),
        CmpOp::Gt => numbers().is_some_and(|(a, b)| a > b),
        CmpOp::Ge => numbers().is_some_and(|(a, b)| a >= b),
    }
}
