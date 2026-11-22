//! `anim.inspect` (M2 phase 2.4.4): a posed character's bones, clip or graph playback and pending
//! events, read from the play world.

use std::sync::Arc;

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, EntityRef, Method, WorldSel};
use concordia_assets::{AssetRef, Parameter, Skinned};
use concordia_core::id::EntityId;
use concordia_core::math::Mat4;
use concordia_reflect::Reflect;
use concordia_scene::builtin::{Transform, world_transform};
use concordia_terra::World;
use serde::{Deserialize, Serialize};

use crate::clip::BoundClip;
use crate::events::AnimEvent;
use crate::graph::{BoundGraph, Node};
use crate::graph_player::{AnimGraphPlayer, AnimGraphState, NodeState};
use crate::library::AnimLibrary;
use crate::player::AnimPlayer;
use crate::pose::Pose;
use crate::skeleton::Skeleton;

/// Inspect an animated character in the running game: every bone's local, model and world
/// transform, its clip player or graph (node states, state names, effective parameters), its
/// clips' events (with frames when the clip has a frame rate) and events raised last tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AnimInspect;

/// `anim.inspect` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct InspectParams {
    /// The posed entity (the one with `Skinned` and `Pose`).
    pub entity: EntityRef,
}

/// `anim.inspect` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct InspectResult {
    /// The skin the pose belongs to.
    pub skin: AssetRef,
    /// Every bone, in skin joint order.
    pub bones: Vec<BoneReport>,
    /// The clip player, if it plays one clip.
    pub clip: Option<ClipReport>,
    /// The graph player, if it plays a graph (it wins over a clip player).
    pub graph: Option<GraphReport>,
    /// Events raised last tick, delivered as signals on the next tick.
    pub pending_events: Vec<AnimEvent>,
}

/// One bone's transforms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct BoneReport {
    /// Its name.
    pub name: String,
    /// Its parent bone.
    pub parent: Option<String>,
    /// Relative to its parent (the pose).
    pub local: Transform,
    /// Relative to the model's root.
    pub model: Transform,
    /// In the world.
    pub world: Transform,
}

/// A clip being played.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ClipReport {
    /// The clip.
    pub clip: AssetRef,
    /// Playback position in seconds.
    pub time: f32,
    /// The position in frames, when the clip has a frame rate.
    pub frame: Option<f32>,
    /// Length in seconds.
    pub duration: f32,
    /// Playback rate (clip players only; 1 in graphs).
    pub speed: f32,
    /// Wraps at the end.
    pub looping: bool,
    /// The clip's events.
    pub events: Vec<EventReport>,
}

/// One clip event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct EventReport {
    /// Its name.
    pub name: String,
    /// Seconds from the clip's start.
    pub time: f32,
    /// The same in frames, when the clip has a frame rate.
    pub frame: Option<f32>,
}

/// A graph being played.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GraphReport {
    /// The graph.
    pub graph: AssetRef,
    /// Every node, in name order.
    pub nodes: Vec<NodeReport>,
    /// Every parameter's effective value (the player's, else the graph's default).
    pub parameters: Vec<ParameterReport>,
}

/// One graph node's playback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct NodeReport {
    /// Its name.
    pub name: String,
    /// Its kind: Clip, Blend2, BlendN, OneShot, TimeScale, Transition or StateMachine.
    pub kind: String,
    /// Clip nodes: the clip, its position and events.
    pub clip: Option<ClipReport>,
    /// One-shots: playing.
    pub active: bool,
    /// Transitions: the input played; state machines: the state.
    pub current: Option<String>,
    /// The input or state fading out.
    pub previous: Option<String>,
    /// Seconds into the fade.
    pub fade: f32,
}

/// A parameter and its effective value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ParameterReport {
    /// Its name.
    pub name: String,
    /// Its kind and value (a trigger shows whether it is requested in `requested`).
    pub value: Parameter,
    /// Triggers: fired for the next evaluation.
    pub requested: bool,
}

impl Method for AnimInspect {
    const NAME: &'static str = "anim.inspect";
    const MUTATES: bool = false;
    type Params = InspectParams;
    type Result = InspectResult;

    fn call(engine: &mut Engine, _: &CallCtx, p: InspectParams) -> Result<InspectResult, ApiError> {
        let world = engine.world(WorldSel::play)?;
        let id = p.entity.resolve(world)?;
        inspect(world, id)
    }
}

/// The report for posed entity `id` of a play world.
fn inspect(world: &World, id: EntityId) -> Result<InspectResult, ApiError> {
    let skinned = world.get::<Skinned>(id).ok_or_else(|| {
        invalid(format!(
            "entity {id} has no Skinned component; inspect the entity that draws the skinned mesh"
        ))
    })?;
    let library = world
        .resource::<Arc<AnimLibrary>>()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the [anim] module isn't running"))?;
    let skeleton = library.skeleton(&skinned.skin).ok_or_else(|| {
        invalid(format!(
            "skin {} wasn't loaded when play started (see log.tail)",
            skinned.skin
        ))
    })?;
    let pose = world
        .get::<Pose>(id)
        .cloned()
        .unwrap_or_else(|| skeleton.rest_pose());
    let at = world_transform(world, id).unwrap_or(Mat4::IDENTITY);
    let graph = world.get::<AnimGraphPlayer>(id).and_then(|player| {
        let bound = library.graph(&player.graph, &skinned.skin)?;
        let state = world.get::<AnimGraphState>(id).cloned().unwrap_or_default();
        Some(graph_report(bound, player, &state, library, &skinned.skin))
    });
    let clip = match graph {
        Some(_) => None,
        None => world.get::<AnimPlayer>(id).and_then(|p| {
            clip_report(
                library.clip(&p.clip, &skinned.skin)?,
                &p.clip,
                p.time,
                p.speed,
                p.looping,
            )
        }),
    };
    Ok(InspectResult {
        skin: skinned.skin.clone(),
        bones: bones(skeleton, &pose, at)?,
        clip,
        graph,
        pending_events: world
            .pending_signals::<AnimEvent>()
            .into_iter()
            .filter(|(target, _)| *target == id)
            .map(|(_, e)| e.clone())
            .collect(),
    })
}

/// Every bone's local, model and world transform.
fn bones(skeleton: &Skeleton, pose: &Pose, at: Mat4) -> Result<Vec<BoneReport>, ApiError> {
    let model = skeleton
        .model_space(pose)
        .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))?;
    let to_entity = skeleton.mesh().inverse();
    let joints = skeleton.joints();
    Ok(joints
        .iter()
        .enumerate()
        .map(|(j, joint)| BoneReport {
            name: joint.name.clone(),
            parent: joint.parent.map(|p| joints[p].name.clone()),
            local: pose.bones[j],
            model: transform_of(model[j]),
            world: transform_of(at * to_entity * model[j]),
        })
        .collect())
}

fn clip_report(
    clip: &BoundClip,
    reference: &AssetRef,
    time: f32,
    speed: f32,
    looping: bool,
) -> Option<ClipReport> {
    let frame = |t: f32| clip.frame_rate().map(|r| t * r);
    Some(ClipReport {
        clip: reference.clone(),
        time,
        frame: frame(time),
        duration: clip.duration(),
        speed,
        looping,
        events: clip
            .events()
            .iter()
            .map(|e| EventReport {
                name: e.name.clone(),
                time: e.time,
                frame: frame(e.time),
            })
            .collect(),
    })
}

/// Each node's playback and every parameter's effective value.
fn graph_report(
    graph: &BoundGraph,
    player: &AnimGraphPlayer,
    state: &AnimGraphState,
    library: &AnimLibrary,
    skin: &AssetRef,
) -> GraphReport {
    let nodes = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(n, node)| node_report(graph, n, node, state.nodes.get(n), library, skin))
        .collect();
    let parameters = graph
        .parameters
        .iter()
        .map(|(name, default)| ParameterReport {
            name: name.clone(),
            value: effective(name, default, player),
            requested: player.triggers.contains(name),
        })
        .collect();
    GraphReport {
        graph: player.graph.clone(),
        nodes,
        parameters,
    }
}

fn node_report(
    graph: &BoundGraph,
    n: usize,
    node: &Node,
    state: Option<&NodeState>,
    library: &AnimLibrary,
    skin: &AssetRef,
) -> NodeReport {
    let state = state.cloned().unwrap_or_default();
    let names = |options: &[usize], i: Option<u32>| {
        i.and_then(|i| options.get(i as usize))
            .map(|o| graph.names[*o].clone())
    };
    let (kind, clip, current, previous) = match node {
        Node::Clip {
            reference, looping, ..
        } => {
            let report = library
                .clip(reference, skin)
                .and_then(|c| clip_report(c, reference, state.time, 1.0, *looping));
            ("Clip", report, None, None)
        }
        Node::Blend2 { .. } => ("Blend2", None, None, None),
        Node::BlendN { .. } => ("BlendN", None, None, None),
        Node::OneShot { .. } => ("OneShot", None, None, None),
        Node::TimeScale { .. } => ("TimeScale", None, None, None),
        Node::Transition { inputs, .. } => {
            let inputs: Vec<usize> = inputs.iter().map(|(i, _)| *i).collect();
            (
                "Transition",
                None,
                names(&inputs, Some(state.current)),
                names(&inputs, state.previous),
            )
        }
        Node::Machine(m) => {
            let state_name = |i: Option<u32>| i.and_then(|i| m.states.get(i as usize)).cloned();
            (
                "StateMachine",
                None,
                state_name(Some(state.current)),
                state_name(state.previous),
            )
        }
    };
    NodeReport {
        name: graph.names[n].clone(),
        kind: kind.to_owned(),
        clip,
        active: state.active,
        current,
        previous,
        fade: state.fade,
    }
}

/// A parameter's value as the graph sees it: the player's, else the default.
fn effective(name: &str, default: &Parameter, player: &AnimGraphPlayer) -> Parameter {
    match default {
        Parameter::Float(v) => Parameter::Float(player.floats.get(name).copied().unwrap_or(*v)),
        Parameter::Int(v) => Parameter::Int(player.ints.get(name).copied().unwrap_or(*v)),
        Parameter::Bool(v) => Parameter::Bool(player.bools.get(name).copied().unwrap_or(*v)),
        Parameter::Trigger => Parameter::Trigger,
    }
}

fn transform_of(m: Mat4) -> Transform {
    let (scale, rotation, translation) = m.to_scale_rotation_translation();
    Transform {
        translation,
        rotation: rotation.normalize(),
        scale,
    }
}

fn invalid(message: String) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidParams, message).at("/entity".to_owned())
}
