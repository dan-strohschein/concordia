//! Animation graphs (`.animgraph.ron`, M2 phase 2.4.3): named parameters and a flat map of named
//! blend, one-shot, time-scale, transition and state-machine nodes over clips (Godot
//! `AnimationTree` semantics). Evaluation is in `concordia-anim`.

use std::collections::{BTreeMap, BTreeSet};

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::reference::AssetRef;

/// An animation graph: parameters, named nodes and the node whose output is the pose.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct AnimGraph {
    /// Parameters by name, with their types and defaults.
    pub parameters: BTreeMap<String, Parameter>,
    /// Nodes by name.
    pub nodes: BTreeMap<String, AnimNode>,
    /// The node whose output is the pose.
    pub root: String,
}

/// A graph parameter: its type and default value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Parameter {
    /// A number (blend amounts, weights, time scales).
    Float(f32),
    /// An integer (a transition node's input).
    Int(i32),
    /// A flag (state machine conditions).
    Bool(bool),
    /// A one-tick request (one-shots, conditions), cleared after every evaluation.
    Trigger,
}

/// How a fade's progress (0..1) becomes a blend weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum FadeCurve {
    /// The progress itself.
    #[default]
    Linear,
    /// Smoothstep: 3x² − 2x³.
    Smooth,
}

/// When a state machine transition may switch (Godot's switch modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum SwitchMode {
    /// Switch now; the destination starts from its beginning (or where it was, without reset).
    #[default]
    Immediate,
    /// Switch now, seeking the destination to the current state's playback position.
    Sync,
    /// Wait until the current state's playback ends.
    AtEnd,
}

/// Whether a transition is used (Godot's advance modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Reflect)]
pub enum AdvanceMode {
    /// Never used, not even by travel.
    Disabled,
    /// Used only by travel.
    #[default]
    Enabled,
    /// Taken automatically when its condition holds.
    Auto,
}

/// One input of a transition node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct TransitionInput {
    /// The input node.
    pub node: String,
    /// Restart the input when switching to it.
    #[serde(default = "yes")]
    pub reset: bool,
}

/// A state machine transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct StateTransition {
    /// The state it leaves.
    pub from: String,
    /// The state it enters.
    pub to: String,
    /// When it may switch.
    #[serde(default)]
    pub switch: SwitchMode,
    /// Whether it is used.
    #[serde(default)]
    pub advance: AdvanceMode,
    /// A bool or trigger parameter that must hold for `Auto` (none: always).
    #[serde(default)]
    pub condition: Option<String>,
    /// Cross-fade seconds.
    #[serde(default)]
    #[reflect(range(min = 0.0, max = 60.0))]
    pub xfade: f32,
    /// Cross-fade curve.
    #[serde(default)]
    pub curve: FadeCurve,
    /// Play the destination from its beginning.
    #[serde(default = "yes")]
    pub reset: bool,
    /// Lower is preferred by travel and `Auto`.
    #[serde(default = "one")]
    pub priority: u32,
}

fn yes() -> bool {
    true
}

fn one() -> u32 {
    1
}

/// One node of a graph; other nodes and parameters are named.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum AnimNode {
    /// Plays a clip, bound to the player's skeleton by bone name.
    Clip {
        /// The clip (`asset:<id>#clip/<name>`).
        clip: AssetRef,
        /// Wraps at the end; otherwise holds the last pose.
        #[serde(default = "yes")]
        looping: bool,
    },
    /// `a` blended toward `b` by `amount` (0..1).
    Blend2 {
        /// The input at amount 0.
        a: String,
        /// The input at amount 1.
        b: String,
        /// A float parameter.
        amount: String,
        /// Bones blended (empty: all); the others keep `a`.
        #[serde(default)]
        filter: Vec<String>,
    },
    /// Inputs blended by their weights, normalized by the sum.
    BlendN {
        /// The inputs.
        inputs: Vec<String>,
        /// One float parameter per input.
        weights: Vec<String>,
    },
    /// Plays `shot` once over `base` when `request` fires, fading in and out.
    OneShot {
        /// The underlying input.
        base: String,
        /// The input played once.
        shot: String,
        /// A trigger parameter that fires the shot.
        request: String,
        /// Fade-in seconds.
        #[serde(default)]
        #[reflect(range(min = 0.0, max = 60.0))]
        fade_in: f32,
        /// Fade-out seconds, ending at the shot's end.
        #[serde(default)]
        #[reflect(range(min = 0.0, max = 60.0))]
        fade_out: f32,
        /// Fade curve.
        #[serde(default)]
        curve: FadeCurve,
        /// Bones the shot drives (empty: all); the others keep `base`.
        #[serde(default)]
        filter: Vec<String>,
    },
    /// Plays `input` at `scale` times the rate.
    TimeScale {
        /// The input.
        input: String,
        /// A float parameter.
        scale: String,
    },
    /// Plays the input named by `select`, cross-fading when it changes.
    Transition {
        /// The inputs.
        inputs: Vec<TransitionInput>,
        /// An int parameter: the input's index.
        select: String,
        /// Cross-fade seconds.
        #[serde(default)]
        #[reflect(range(min = 0.0, max = 60.0))]
        xfade: f32,
        /// Cross-fade curve.
        #[serde(default)]
        curve: FadeCurve,
    },
    /// States (each a node) joined by transitions; one state plays at a time.
    StateMachine {
        /// States by name, each naming its node.
        states: BTreeMap<String, String>,
        /// The state played first.
        start: String,
        /// The transitions, in priority order within equal priorities.
        #[serde(default)]
        transitions: Vec<StateTransition>,
    },
}

impl AnimNode {
    /// The nodes this one plays (states in name order).
    pub fn children(&self) -> Vec<&str> {
        match self {
            Self::Clip { .. } => Vec::new(),
            Self::Blend2 { a, b, .. } => vec![a, b],
            Self::BlendN { inputs, .. } => inputs.iter().map(String::as_str).collect(),
            Self::OneShot { base, shot, .. } => vec![base, shot],
            Self::TimeScale { input, .. } => vec![input],
            Self::Transition { inputs, .. } => inputs.iter().map(|i| i.node.as_str()).collect(),
            Self::StateMachine { states, .. } => states.values().map(String::as_str).collect(),
        }
    }
}

/// A graph is malformed (each error names the node, or the parameter).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AnimGraphError {
    /// Something about one node is wrong.
    #[error("node {node:?}: {reason}")]
    Node {
        /// The node at fault.
        node: String,
        /// What is wrong.
        reason: String,
    },
    /// Nodes play each other in a cycle.
    #[error("graph nodes form a cycle: {}", path.join(" -> "))]
    Cycle {
        /// The cycle, starting and ending at the same node.
        path: Vec<String>,
    },
    /// A parameter is malformed.
    #[error("parameter {parameter:?}: {reason}")]
    Parameter {
        /// The parameter.
        parameter: String,
        /// What is wrong.
        reason: String,
    },
    /// The root names no node.
    #[error("graph root {root:?} names no node; nodes: {known}")]
    MissingRoot {
        /// The root.
        root: String,
        /// Node names.
        known: String,
    },
    /// The text is not a graph.
    #[error("animation graph text: {0}")]
    Parse(String),
}

impl AnimGraphError {
    /// The node at fault (node and cycle errors).
    pub fn node(&self) -> Option<&str> {
        match self {
            Self::Node { node, .. } => Some(node),
            Self::Cycle { path } => path.first().map(String::as_str),
            Self::Parameter { .. } | Self::MissingRoot { .. } | Self::Parse(_) => None,
        }
    }
}

impl AnimGraph {
    /// Checks names, references, parameter types, cycles, parents, reachability and values.
    pub fn validate(&self) -> Result<(), AnimGraphError> {
        self.check_parameters()?;
        for name in self.nodes.keys() {
            check_name(name).map_err(|reason| node_error(name, reason))?;
        }
        if !self.nodes.contains_key(&self.root) {
            let known = self.nodes.keys().cloned().collect::<Vec<_>>().join(", ");
            return Err(AnimGraphError::MissingRoot {
                root: self.root.clone(),
                known,
            });
        }
        for (name, node) in &self.nodes {
            for child in node.children() {
                if !self.nodes.contains_key(child) {
                    return Err(node_error(
                        name,
                        format!("plays {child:?}, which doesn't exist"),
                    ));
                }
            }
        }
        self.check_acyclic()?;
        self.check_parents()?;
        let reachable = self.reachable();
        if let Some(node) = self.nodes.keys().find(|n| !reachable.contains(n.as_str())) {
            return Err(node_error(
                node,
                format!("is not reachable from the root {:?}", self.root),
            ));
        }
        self.nodes
            .iter()
            .try_for_each(|(name, node)| self.check_node(node).map_err(|r| node_error(name, r)))
    }

    /// The node named `name`.
    pub fn node(&self, name: &str) -> Option<&AnimNode> {
        self.nodes.get(name)
    }

    /// Every clip the graph can play, sorted, once each.
    pub fn clips(&self) -> BTreeSet<&AssetRef> {
        self.nodes
            .values()
            .filter_map(|n| match n {
                AnimNode::Clip { clip, .. } => Some(clip),
                _ => None,
            })
            .collect()
    }

    /// Nodes reachable from the root (missing names are skipped).
    pub fn reachable(&self) -> BTreeSet<&str> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![self.root.as_str()];
        while let Some(name) = stack.pop() {
            let Some((key, node)) = self.nodes.get_key_value(name) else {
                continue;
            };
            if seen.insert(key.as_str()) {
                stack.extend(node.children());
            }
        }
        seen
    }

    fn check_parameters(&self) -> Result<(), AnimGraphError> {
        for (name, p) in &self.parameters {
            let fail = |reason: String| AnimGraphError::Parameter {
                parameter: name.clone(),
                reason,
            };
            check_name(name).map_err(fail)?;
            if let Parameter::Float(v) = p
                && !v.is_finite()
            {
                return Err(fail(format!("default {v} is not finite")));
            }
        }
        Ok(())
    }

    fn check_acyclic(&self) -> Result<(), AnimGraphError> {
        let mut done = BTreeSet::new();
        for start in self.nodes.keys() {
            self.visit(start, &mut Vec::new(), &mut done)?;
        }
        Ok(())
    }

    /// Depth-first search; `path` holds the nodes being visited, `done` the finished ones.
    fn visit<'a>(
        &'a self,
        name: &'a str,
        path: &mut Vec<&'a str>,
        done: &mut BTreeSet<&'a str>,
    ) -> Result<(), AnimGraphError> {
        if done.contains(name) {
            return Ok(());
        }
        if let Some(at) = path.iter().position(|n| *n == name) {
            let mut cycle: Vec<String> = path[at..].iter().map(|n| (*n).to_owned()).collect();
            cycle.push(name.to_owned());
            return Err(AnimGraphError::Cycle { path: cycle });
        }
        path.push(name);
        let children = self.nodes.get(name).map(AnimNode::children);
        for child in children.into_iter().flatten() {
            self.visit(child, path, done)?;
        }
        path.pop();
        done.insert(name);
        Ok(())
    }

    /// Each node is played by at most one other (node playback is kept per node).
    fn check_parents(&self) -> Result<(), AnimGraphError> {
        let mut parents: BTreeMap<&str, &str> = BTreeMap::new();
        for (name, node) in &self.nodes {
            for child in node.children() {
                if let Some(first) = parents.insert(child, name) {
                    return Err(node_error(
                        child,
                        format!("is played by both {first:?} and {name:?}; give each its own node"),
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_node(&self, node: &AnimNode) -> Result<(), String> {
        match node {
            AnimNode::Clip { .. } => Ok(()),
            AnimNode::Blend2 { amount, .. } => {
                let v = self.float(amount)?;
                if !(0.0..=1.0).contains(&v) {
                    return Err(format!("amount {amount:?} default {v} is outside 0..=1"));
                }
                Ok(())
            }
            AnimNode::BlendN { inputs, weights } => self.check_blend_n(inputs, weights),
            AnimNode::OneShot {
                request,
                fade_in,
                fade_out,
                ..
            } => {
                self.typed(request, Kind::Trigger)?;
                seconds("fade_in", *fade_in)?;
                seconds("fade_out", *fade_out)
            }
            AnimNode::TimeScale { scale, .. } => self.float(scale).map(|_| ()),
            AnimNode::Transition {
                inputs,
                select,
                xfade,
                ..
            } => {
                seconds("xfade", *xfade)?;
                let Parameter::Int(v) = self.typed(select, Kind::Int)? else {
                    unreachable!("typed checked the kind")
                };
                if usize::try_from(v).map_or(true, |i| i >= inputs.len()) {
                    return Err(format!(
                        "select {select:?} default {v} is not one of its {} inputs",
                        inputs.len()
                    ));
                }
                Ok(())
            }
            AnimNode::StateMachine {
                states,
                start,
                transitions,
            } => self.check_machine(states, start, transitions),
        }
    }

    fn check_blend_n(&self, inputs: &[String], weights: &[String]) -> Result<(), String> {
        if inputs.is_empty() || inputs.len() != weights.len() {
            return Err(format!(
                "needs one weight per input: {} inputs, {} weights",
                inputs.len(),
                weights.len()
            ));
        }
        for w in weights {
            let v = self.float(w)?;
            if v < 0.0 {
                return Err(format!("weight {w:?} default {v} is negative"));
            }
        }
        Ok(())
    }

    fn check_machine(
        &self,
        states: &BTreeMap<String, String>,
        start: &str,
        transitions: &[StateTransition],
    ) -> Result<(), String> {
        for name in states.keys() {
            check_name(name).map_err(|r| format!("state {r}"))?;
        }
        if !states.contains_key(start) {
            return Err(format!("start state {start:?} is not one of its states"));
        }
        for (i, t) in transitions.iter().enumerate() {
            for end in [&t.from, &t.to] {
                if !states.contains_key(end) {
                    return Err(format!("transition {i} names unknown state {end:?}"));
                }
            }
            if t.from == t.to {
                return Err(format!("transition {i} goes from {:?} to itself", t.from));
            }
            if let Some(c) = &t.condition
                && !matches!(
                    self.parameters.get(c),
                    Some(Parameter::Bool(_) | Parameter::Trigger)
                )
            {
                return Err(format!(
                    "transition {i}'s condition {c:?} must be a Bool or Trigger parameter"
                ));
            }
            seconds(&format!("transition {i} xfade"), t.xfade)?;
        }
        let reachable = reachable_states(start, transitions);
        match states.keys().find(|s| !reachable.contains(s.as_str())) {
            Some(lonely) => Err(format!(
                "state {lonely:?} is not reachable from the start {start:?}"
            )),
            None => Ok(()),
        }
    }

    /// The default of float parameter `name`.
    fn float(&self, name: &str) -> Result<f32, String> {
        match self.typed(name, Kind::Float)? {
            Parameter::Float(v) => Ok(v),
            _ => unreachable!("typed checked the kind"),
        }
    }

    /// Parameter `name`, which must be of `kind`.
    fn typed(&self, name: &str, kind: Kind) -> Result<Parameter, String> {
        let p = *self
            .parameters
            .get(name)
            .ok_or_else(|| format!("unknown parameter {name:?}"))?;
        if Kind::of(p) != kind {
            return Err(format!("parameter {name:?} must be a {kind:?} parameter"));
        }
        Ok(p)
    }
}

/// A parameter's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Float,
    Int,
    Bool,
    Trigger,
}

impl Kind {
    fn of(p: Parameter) -> Self {
        match p {
            Parameter::Float(_) => Self::Float,
            Parameter::Int(_) => Self::Int,
            Parameter::Bool(_) => Self::Bool,
            Parameter::Trigger => Self::Trigger,
        }
    }
}

/// States reachable from `start` through transitions that aren't disabled.
fn reachable_states<'a>(start: &'a str, transitions: &'a [StateTransition]) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::from([start]);
    let mut stack = vec![start];
    while let Some(state) = stack.pop() {
        for t in transitions {
            if t.from == state && t.advance != AdvanceMode::Disabled && seen.insert(t.to.as_str()) {
                stack.push(&t.to);
            }
        }
    }
    seen
}

fn node_error(node: &str, reason: impl Into<String>) -> AnimGraphError {
    AnimGraphError::Node {
        node: node.to_owned(),
        reason: reason.into(),
    }
}

fn check_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "name {name:?} must be non-empty and use only letters, digits, '_' and '-'"
        ))
    }
}

/// A duration field: finite and not negative.
fn seconds(field: &str, value: f32) -> Result<(), String> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(format!("{field} {value} must be finite seconds ≥ 0"))
    }
}

/// Reads and validates `.animgraph.ron` text.
pub fn read_animgraph(text: &str) -> Result<AnimGraph, AnimGraphError> {
    let graph = concordia_scene::read_document::<AnimGraph>(text)
        .map_err(|e| AnimGraphError::Parse(e.to_string()))?;
    graph.validate()?;
    Ok(graph)
}

/// Writes a graph as canonical `.animgraph.ron` text.
pub fn write_animgraph(graph: &AnimGraph) -> Result<String, AnimGraphError> {
    graph.validate()?;
    concordia_scene::write_document(graph).map_err(|e| AnimGraphError::Parse(e.to_string()))
}
