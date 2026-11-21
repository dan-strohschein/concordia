//! `AnimGraphPlayer` (a graph and its parameters, written by gameplay) and `AnimGraphState` (the
//! graph's playback, written only by `anim.sample`); both are hashed simulation state.

use std::collections::BTreeMap;

use concordia_assets::AssetRef;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// Plays an animation graph on this entity's skeleton (its `Skinned` skin), writing its `Pose`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Animation")]
#[serde(deny_unknown_fields)]
pub struct AnimGraphPlayer {
    /// The graph (`asset:<id>` of an `.animgraph.ron`).
    pub graph: AssetRef,
    /// Float parameters (missing ones use the graph's defaults).
    #[serde(default)]
    pub floats: BTreeMap<String, f32>,
    /// Int parameters.
    #[serde(default)]
    pub ints: BTreeMap<String, i32>,
    /// Bool parameters.
    #[serde(default)]
    pub bools: BTreeMap<String, bool>,
    /// Triggers fired for the next evaluation (cleared after it).
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Travel requests: state machine node → target state (cleared after the next evaluation).
    #[serde(default)]
    pub travel: BTreeMap<String, String>,
}

impl AnimGraphPlayer {
    /// Plays `graph` with its default parameters.
    pub fn new(graph: AssetRef) -> Self {
        Self {
            graph,
            floats: BTreeMap::new(),
            ints: BTreeMap::new(),
            bools: BTreeMap::new(),
            triggers: Vec::new(),
            travel: BTreeMap::new(),
        }
    }
}

/// A graph's playback: one entry per graph node, in node name order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(category = "Animation", default)]
#[serde(deny_unknown_fields)]
pub struct AnimGraphState {
    /// Per-node playback.
    pub nodes: Vec<NodeState>,
}

/// One node's playback; each node kind uses the fields it needs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[reflect(default)]
#[serde(default, deny_unknown_fields)]
pub struct NodeState {
    /// Clip: playback seconds. One-shot: seconds since it fired.
    pub time: f32,
    /// One-shot: playing.
    pub active: bool,
    /// Transition: the input played. State machine: the state played (name order).
    pub current: u32,
    /// The input or state fading out, if any.
    pub previous: Option<u32>,
    /// Seconds into the fade.
    pub fade: f32,
    /// State machine: the transition being faded.
    pub via: Option<u32>,
    /// State machine: travel steps left (transition indices).
    pub path: Vec<u32>,
    /// State machine: the current state's playback ended last tick.
    pub ended: bool,
    /// State machine: the current state's playback position last tick.
    pub position: f32,
}
