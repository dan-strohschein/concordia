//! Behavior tree assets (M4 4.12, M3): nodes in a flat, depth-first array; each node records
//! where its subtree ends, so children are found by skipping subtrees.

use std::collections::BTreeMap;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::utility::{Selection, UtilityOption};
use crate::{BbType, BbValue};

/// When a condition is checked again while something runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum Abort {
    /// Only when entered.
    #[default]
    None,
    /// While its own child runs: it stops the child when it no longer holds.
    SelfOnly,
    /// While lower-priority siblings run: it interrupts them when it starts to hold.
    Lower,
}

/// How a condition compares a key with a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum CmpOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less (numbers).
    Lt,
    /// Less or equal.
    Le,
    /// Greater.
    Gt,
    /// Greater or equal.
    Ge,
    /// The key has a value.
    IsSet,
    /// The key has none.
    NotSet,
}

/// When a parallel node finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum ParallelPolicy {
    /// Succeeds when every child has; fails when one does.
    All,
    /// Succeeds when one child does; fails when every child has.
    One,
}

/// What a leaf does. Built in: `Wait`, `SetBlackboard`, `Succeed`, `Fail`; the rest go to the
/// task host (the engine, or a test's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum TaskKind {
    /// Walk to the entity or point in a key (succeeds on arrival).
    MoveTo {
        /// The key.
        key: String,
    },
    /// Wait some seconds.
    Wait {
        /// Seconds.
        seconds: f32,
    },
    /// Fire an animation graph trigger.
    PlayAnim {
        /// The trigger.
        trigger: String,
    },
    /// Turn toward the entity or point in a key.
    FaceTarget {
        /// The key.
        key: String,
    },
    /// Write a key.
    SetBlackboard {
        /// The key.
        key: String,
        /// The value.
        value: BbValue,
    },
    /// Cast a skill at the entity in a key (raises `ai.Cast`).
    CastSkill {
        /// The skill.
        skill: String,
        /// The key holding the target.
        target_key: String,
    },
    /// Ask a graph to run (raises `ai.Run`).
    RunGraph {
        /// The graph module.
        graph: String,
    },
    /// Ask the LLM planner (opt-in), failing over to the tree on anything but a valid, timely plan.
    Plan {
        /// What to plan for.
        goal: String,
        /// Ticks to wait.
        deadline_ticks: u32,
        /// The actions it may choose (the answer's schema allows only these).
        options: Vec<String>,
        /// The `Text` key the chosen action is written to.
        key: String,
    },
    /// Succeed.
    Succeed,
    /// Fail.
    Fail,
    /// A host-defined task.
    Custom {
        /// Its name.
        name: String,
    },
}

/// A node's kind and parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum NodeKind {
    /// Children in order until one fails.
    Sequence,
    /// Children in order until one succeeds.
    Selector,
    /// Every child at once.
    Parallel {
        /// When it finishes.
        policy: ParallelPolicy,
    },
    /// The child whose option scores best (one option per child).
    UtilitySelector {
        /// The options, in child order.
        options: Vec<UtilityOption>,
        /// How the winner is picked.
        selection: Selection,
    },
    /// A child picked at random (one weight per child; seeded).
    RandomSelector {
        /// Weights, in child order.
        weights: Vec<f32>,
    },
    /// Runs its child while a key compares true with a value.
    Condition {
        /// The key.
        key: String,
        /// The comparison.
        op: CmpOp,
        /// The value.
        value: BbValue,
        /// When it is checked again.
        abort: Abort,
    },
    /// Fails for some seconds after its child finished.
    Cooldown {
        /// Seconds.
        seconds: f32,
    },
    /// Runs its child `count` times (0: forever) while it succeeds.
    Loop {
        /// Times.
        count: u32,
    },
    /// Fails its child after some seconds.
    TimeLimit {
        /// Seconds.
        seconds: f32,
    },
    /// Swaps its child's success and failure.
    Inverter,
    /// A leaf.
    Task(TaskKind),
}

impl NodeKind {
    /// How many children it takes: `(min, max)`.
    fn arity(&self) -> (usize, usize) {
        match self {
            Self::Sequence
            | Self::Selector
            | Self::Parallel { .. }
            | Self::UtilitySelector { .. }
            | Self::RandomSelector { .. } => (1, usize::MAX),
            Self::Condition { .. }
            | Self::Cooldown { .. }
            | Self::Loop { .. }
            | Self::TimeLimit { .. }
            | Self::Inverter => (1, 1),
            Self::Task(_) => (0, 0),
        }
    }
}

/// A node in the flat array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct BtNode {
    /// What it is.
    pub kind: NodeKind,
    /// One past the last node of its subtree.
    pub end: u32,
}

/// A behavior tree: its blackboard keys and its nodes (index 0 is the root).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(name = "ai.BehaviorTree")]
pub struct BehaviorTree {
    /// Blackboard keys and their types.
    pub keys: BTreeMap<String, BbType>,
    /// Nodes, depth first.
    pub nodes: Vec<BtNode>,
}

/// A tree written as nested nodes (flattened by [`BehaviorTree::build`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    kind: NodeKind,
    children: Vec<Spec>,
}

impl Spec {
    /// A node of `kind` with no children.
    pub fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            children: Vec::new(),
        }
    }

    /// With `children`.
    #[must_use]
    pub fn children(mut self, children: Vec<Spec>) -> Self {
        self.children = children;
        self
    }
}

/// Why a tree is malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("behavior tree node {node}: {message}")]
pub struct TreeError {
    /// The node's index.
    pub node: u32,
    /// What's wrong.
    pub message: String,
}

impl BehaviorTree {
    /// Flattens and validates `root` over the declared `keys`.
    pub fn build(keys: BTreeMap<String, BbType>, root: Spec) -> Result<Self, TreeError> {
        let mut nodes = Vec::new();
        flatten(root, &mut nodes);
        let tree = Self { keys, nodes };
        tree.validate()?;
        Ok(tree)
    }

    /// The children of node `i`, in order.
    pub fn children(&self, i: u32) -> impl Iterator<Item = u32> + '_ {
        let end = self.nodes[i as usize].end;
        let mut next = i + 1;
        std::iter::from_fn(move || {
            (next < end).then(|| {
                let child = next;
                next = self.nodes[child as usize].end;
                child
            })
        })
    }

    /// Arity, declared keys, option counts and finite parameters.
    pub fn validate(&self) -> Result<(), TreeError> {
        if self.nodes.is_empty() {
            return Err(TreeError {
                node: 0,
                message: "a tree needs a root".into(),
            });
        }
        for (i, node) in self.nodes.iter().enumerate() {
            let i = i as u32;
            let fail = |message: String| Err(TreeError { node: i, message });
            let count = self.children(i).count();
            let (min, max) = node.kind.arity();
            if count < min || count > max {
                return fail(format!(
                    "{:?} takes {min}..{max} children, has {count}",
                    node.kind
                ));
            }
            if let Some(key) = keys_of(&node.kind)
                .into_iter()
                .find(|k| !self.keys.contains_key(*k))
            {
                return fail(format!("blackboard key {key:?} isn't declared"));
            }
            match &node.kind {
                NodeKind::UtilitySelector { options, .. } if options.len() != count => {
                    return fail(format!("{} options for {count} children", options.len()));
                }
                NodeKind::RandomSelector { weights }
                    if weights.len() != count
                        || weights.iter().any(|w| !(w.is_finite() && *w >= 0.0)) =>
                {
                    return fail(format!("needs {count} finite, non-negative weights"));
                }
                NodeKind::Cooldown { seconds }
                | NodeKind::TimeLimit { seconds }
                | NodeKind::Task(TaskKind::Wait { seconds })
                    if !(seconds.is_finite() && *seconds >= 0.0) =>
                {
                    return fail("seconds must be finite and non-negative".into());
                }
                NodeKind::Task(TaskKind::Plan { options, key, .. })
                    if options.is_empty() || self.keys.get(key) != Some(&BbType::Text) =>
                {
                    return fail(format!("a plan needs actions and a Text key ({key:?})"));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn flatten(spec: Spec, nodes: &mut Vec<BtNode>) {
    let at = nodes.len();
    nodes.push(BtNode {
        kind: spec.kind,
        end: 0,
    });
    for child in spec.children {
        flatten(child, nodes);
    }
    nodes[at].end = nodes.len() as u32;
}

/// The blackboard keys a node reads or writes.
fn keys_of(kind: &NodeKind) -> Vec<&str> {
    match kind {
        NodeKind::Condition { key, .. } => vec![key],
        NodeKind::Task(
            TaskKind::MoveTo { key }
            | TaskKind::FaceTarget { key }
            | TaskKind::SetBlackboard { key, .. },
        ) => vec![key],
        NodeKind::Task(TaskKind::CastSkill { target_key, .. }) => vec![target_key],
        NodeKind::Task(TaskKind::Plan { key, .. }) => vec![key],
        NodeKind::UtilitySelector { options, .. } => {
            options.iter().flat_map(|o| o.keys()).collect()
        }
        _ => Vec::new(),
    }
}
