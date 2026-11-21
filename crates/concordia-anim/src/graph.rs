//! `BoundGraph`: an `AnimGraph` bound to one skeleton (clips by bone name, filters to joints) and
//! evaluated at the fixed step (M2 phase 2.4.3; Godot `AnimationTree` semantics).

use std::collections::{BTreeMap, BTreeSet};

use concordia_assets::{
    AdvanceMode, AnimGraph, AnimNode, AssetRef, FadeCurve, Parameter, StateTransition, SwitchMode,
};

use crate::clip::BoundClip;
use crate::error::AnimError;
use crate::graph_eval::Eval;
use crate::graph_player::{AnimGraphPlayer, AnimGraphState, NodeState};
use crate::pose::Pose;
use crate::skeleton::Skeleton;

/// A validated graph bound to one skeleton; nodes are indexed in name order.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundGraph {
    pub(crate) names: Vec<String>,
    pub(crate) nodes: Vec<Node>,
    pub(crate) root: usize,
    pub(crate) parameters: BTreeMap<String, Parameter>,
    unmatched: Vec<String>,
}

/// A bound node; inputs are node indices, masks are per joint.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    Clip {
        clip: BoundClip,
        looping: bool,
    },
    Blend2 {
        a: usize,
        b: usize,
        amount: String,
        mask: Option<Vec<bool>>,
    },
    BlendN {
        inputs: Vec<usize>,
        weights: Vec<String>,
    },
    OneShot {
        base: usize,
        shot: usize,
        request: String,
        fade_in: f32,
        fade_out: f32,
        curve: FadeCurve,
        mask: Option<Vec<bool>>,
    },
    TimeScale {
        input: usize,
        scale: String,
    },
    Transition {
        inputs: Vec<(usize, bool)>,
        select: String,
        xfade: f32,
        curve: FadeCurve,
    },
    Machine(Machine),
}

/// A bound state machine: states in name order, each with its node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Machine {
    pub(crate) states: Vec<String>,
    pub(crate) nodes: Vec<usize>,
    pub(crate) start: u32,
    pub(crate) edges: Vec<Edge>,
    /// Edge indices by (priority, file order): the order travel and `Auto` try them in.
    pub(crate) by_priority: Vec<u32>,
}

/// A bound transition between state indices.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Edge {
    pub(crate) from: u32,
    pub(crate) to: u32,
    pub(crate) switch: SwitchMode,
    pub(crate) advance: AdvanceMode,
    pub(crate) condition: Option<String>,
    pub(crate) xfade: f32,
    pub(crate) curve: FadeCurve,
    pub(crate) reset: bool,
}

impl BoundGraph {
    /// Validates `graph` and binds it to `skeleton`, getting each clip node's clip from `clips`.
    pub fn bind(
        graph: &AnimGraph,
        skeleton: &Skeleton,
        clips: &mut dyn FnMut(&AssetRef) -> Result<BoundClip, AnimError>,
    ) -> Result<Self, AnimError> {
        graph.validate()?;
        let names: Vec<String> = graph.nodes.keys().cloned().collect();
        let index = |n: &str| names.binary_search_by(|k| k.as_str().cmp(n)).unwrap_or(0);
        let mut unmatched = BTreeSet::new();
        let mut nodes = Vec::with_capacity(names.len());
        for (name, node) in &graph.nodes {
            let bound =
                bind_node(node, &index, skeleton, &mut unmatched, clips).map_err(|reason| {
                    AnimError::Graph {
                        node: name.clone(),
                        reason,
                    }
                })?;
            nodes.push(bound);
        }
        Ok(Self {
            root: index(&graph.root),
            names,
            nodes,
            parameters: graph.parameters.clone(),
            unmatched: unmatched.into_iter().collect(),
        })
    }

    /// Filter bone names the skeleton lacks (sorted, once each).
    pub fn unmatched(&self) -> &[String] {
        &self.unmatched
    }

    /// The playback every node starts with.
    pub fn initial_state(&self) -> AnimGraphState {
        AnimGraphState {
            nodes: (0..self.nodes.len()).map(|n| self.initial(n)).collect(),
        }
    }

    /// The state a state machine node is playing.
    pub fn active_state<'a>(&'a self, state: &AnimGraphState, machine: &str) -> Option<&'a str> {
        let n = self.index_of(machine)?;
        let Node::Machine(m) = &self.nodes[n] else {
            return None;
        };
        let current = state.nodes.get(n)?.current;
        m.states.get(current as usize).map(String::as_str)
    }

    /// Advances the graph by `dt` and writes `pose`; then clears the player's triggers and travel.
    pub fn evaluate(
        &self,
        dt: f32,
        player: &mut AnimGraphPlayer,
        state: &mut AnimGraphState,
        skeleton: &Skeleton,
        pose: &mut Pose,
    ) -> Result<(), AnimError> {
        if state.nodes.len() != self.nodes.len() {
            *state = self.initial_state();
        }
        self.sanitize(state);
        let dt = if dt.is_finite() { dt } else { 0.0 };
        Eval::new(self, player, skeleton).node(self.root, dt, &mut state.nodes, pose);
        player.triggers.clear();
        player.travel.clear();
        Ok(())
    }

    /// The node named `name`.
    pub(crate) fn index_of(&self, name: &str) -> Option<usize> {
        self.names.binary_search_by(|k| k.as_str().cmp(name)).ok()
    }

    /// Node `n`'s starting playback.
    pub(crate) fn initial(&self, n: usize) -> NodeState {
        let current = match &self.nodes[n] {
            Node::Transition { select, .. } => match self.parameters.get(select) {
                Some(Parameter::Int(v)) => u32::try_from(*v).unwrap_or(0),
                _ => 0,
            },
            Node::Machine(m) => m.start,
            _ => 0,
        };
        NodeState {
            current,
            ..NodeState::default()
        }
    }

    /// Resets entries that point outside the graph (a state edited by hand), so evaluation never
    /// indexes out of range.
    fn sanitize(&self, state: &mut AnimGraphState) {
        for (n, s) in state.nodes.iter_mut().enumerate() {
            if !self.fits(n, s) {
                *s = self.initial(n);
            }
        }
    }

    fn fits(&self, n: usize, s: &NodeState) -> bool {
        let finite = s.time.is_finite() && s.fade.is_finite() && s.position.is_finite();
        let count = match &self.nodes[n] {
            Node::Transition { inputs, .. } => inputs.len(),
            Node::Machine(m) => {
                let edges_ok = s
                    .via
                    .iter()
                    .chain(&s.path)
                    .all(|e| (*e as usize) < m.edges.len());
                if !edges_ok {
                    return false;
                }
                m.states.len()
            }
            _ => 1,
        };
        let index_ok = |i: u32| (i as usize) < count;
        finite && index_ok(s.current) && s.previous.is_none_or(index_ok)
    }
}

/// Binds one node: names become indices, clips are bound, filters become joint masks; a clip
/// that can't be bound is the reason returned.
fn bind_node(
    node: &AnimNode,
    index: &dyn Fn(&str) -> usize,
    skeleton: &Skeleton,
    unmatched: &mut BTreeSet<String>,
    clips: &mut dyn FnMut(&AssetRef) -> Result<BoundClip, AnimError>,
) -> Result<Node, String> {
    Ok(match node {
        AnimNode::Clip { clip, looping } => Node::Clip {
            clip: clips(clip).map_err(|e| format!("clip {clip}: {e}"))?,
            looping: *looping,
        },
        AnimNode::Blend2 {
            a,
            b,
            amount,
            filter,
        } => Node::Blend2 {
            a: index(a),
            b: index(b),
            amount: amount.clone(),
            mask: mask(filter, skeleton, unmatched),
        },
        AnimNode::BlendN { inputs, weights } => Node::BlendN {
            inputs: inputs.iter().map(|i| index(i)).collect(),
            weights: weights.clone(),
        },
        AnimNode::OneShot {
            base,
            shot,
            request,
            fade_in,
            fade_out,
            curve,
            filter,
        } => Node::OneShot {
            base: index(base),
            shot: index(shot),
            request: request.clone(),
            fade_in: *fade_in,
            fade_out: *fade_out,
            curve: *curve,
            mask: mask(filter, skeleton, unmatched),
        },
        AnimNode::TimeScale { input, scale } => Node::TimeScale {
            input: index(input),
            scale: scale.clone(),
        },
        AnimNode::Transition {
            inputs,
            select,
            xfade,
            curve,
        } => Node::Transition {
            inputs: inputs.iter().map(|i| (index(&i.node), i.reset)).collect(),
            select: select.clone(),
            xfade: *xfade,
            curve: *curve,
        },
        AnimNode::StateMachine {
            states,
            start,
            transitions,
        } => Node::Machine(machine(states, start, transitions, index)),
    })
}

/// Joints a filter names, as a mask (`None`: an empty filter, every joint); unknown names are
/// collected.
fn mask(
    filter: &[String],
    skeleton: &Skeleton,
    unmatched: &mut BTreeSet<String>,
) -> Option<Vec<bool>> {
    if filter.is_empty() {
        return None;
    }
    let mut mask = vec![false; skeleton.len()];
    for bone in filter {
        match skeleton.index_of(bone) {
            Some(j) => mask[j] = true,
            None => {
                unmatched.insert(bone.clone());
            }
        }
    }
    Some(mask)
}

fn machine(
    states: &BTreeMap<String, String>,
    start: &str,
    transitions: &[StateTransition],
    index: &dyn Fn(&str) -> usize,
) -> Machine {
    let names: Vec<String> = states.keys().cloned().collect();
    let state = |s: &str| {
        let i = names.binary_search_by(|k| k.as_str().cmp(s)).unwrap_or(0);
        u32::try_from(i).unwrap_or(0)
    };
    let edges: Vec<Edge> = transitions
        .iter()
        .map(|t| Edge {
            from: state(&t.from),
            to: state(&t.to),
            switch: t.switch,
            advance: t.advance,
            condition: t.condition.clone(),
            xfade: t.xfade,
            curve: t.curve,
            reset: t.reset,
        })
        .collect();
    let mut by_priority: Vec<u32> = (0..edges.len())
        .map(|e| u32::try_from(e).unwrap_or(u32::MAX))
        .collect();
    by_priority.sort_by_key(|e| (transitions[*e as usize].priority, *e));
    Machine {
        nodes: states.values().map(|n| index(n)).collect(),
        start: state(start),
        states: names,
        edges,
        by_priority,
    }
}
