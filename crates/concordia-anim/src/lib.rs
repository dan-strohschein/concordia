//! Concordia animation (M2 phase 2.4, ADR-0074): skeletons from imported skins, a hashed `Pose` per
//! character, clips bound by bone name and sampled per the glTF 2.0 specification, and players
//! advanced at Terra's fixed step in `FrameEnd`, and blend graphs and state machines (2.4.3).

mod attach;
mod clip;
mod error;
mod events;
mod graph;
mod graph_eval;
mod graph_player;
mod library;
mod methods;
mod module;
mod player;
mod pose;
mod processors;
mod skeleton;
mod skin;
mod tween;

pub use attach::{ATTACH, BoneAttachment};
pub use clip::{BoundClip, validate_clip};
pub use error::AnimError;
pub use events::{AnimEvent, Crossing, crossed_events};
pub use graph::BoundGraph;
pub use graph_player::{AnimGraphPlayer, AnimGraphState, NodeState};
pub use library::{AnimLibrary, LoadReport};
pub use methods::{
    AnimInspect, BoneReport, ClipReport, EventReport, GraphReport, InspectParams, InspectResult,
    NodeReport, ParameterReport,
};
pub use module::AnimModule;
pub use player::{AnimPlayer, advance};
pub use pose::Pose;
pub use processors::{CHUNK, POSE, SAMPLE, add_processors, insert_rest_poses};
pub use skeleton::{Joint, Skeleton, bind_mesh, model_matrix};
pub use skin::{JointBounds, SkinnedVertices, joint_bounds, skin_vertices, skinned_bounds};
pub use tween::{Ease, PropertyTween, TWEEN, Transition, Tweens, add_tweens, ease};

/// Registers the animation components (`Pose`, `AnimPlayer`, `AnimGraphPlayer`, `AnimGraphState`,
/// `BoneAttachment`, `Tweens`) and the `AnimEvent` signal.
pub fn register_types(
    registry: &mut concordia_reflect::TypeRegistry,
) -> Result<(), concordia_reflect::RegistryError> {
    concordia_terra::register_component::<Pose>(registry)?;
    concordia_terra::register_component::<AnimPlayer>(registry)?;
    concordia_terra::register_component::<AnimGraphPlayer>(registry)?;
    concordia_terra::register_component::<AnimGraphState>(registry)?;
    concordia_terra::register_component::<BoneAttachment>(registry)?;
    concordia_terra::register_component::<Tweens>(registry)?;
    concordia_terra::register_component::<AnimEvent>(registry)
}
