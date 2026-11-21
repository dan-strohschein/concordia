//! Concordia animation (M2 phase 2.4, ADR-0074): skeletons from imported skins, a hashed `Pose` per
//! character, clips bound by bone name and sampled per the glTF 2.0 specification, and players
//! advanced at Terra's fixed step in `FrameEnd`.

mod clip;
mod error;
mod library;
mod module;
mod player;
mod pose;
mod processors;
mod skeleton;
mod skin;

pub use clip::{BoundClip, validate_clip};
pub use error::AnimError;
pub use library::{AnimLibrary, LoadReport};
pub use module::AnimModule;
pub use player::{AnimPlayer, advance};
pub use pose::Pose;
pub use processors::{CHUNK, POSE, SAMPLE, add_processors, insert_rest_poses};
pub use skeleton::{Joint, Skeleton, bind_mesh, model_matrix};
pub use skin::{JointBounds, SkinnedVertices, joint_bounds, skin_vertices, skinned_bounds};

/// Registers the animation components (`Pose`, `AnimPlayer`).
pub fn register_types(
    registry: &mut concordia_reflect::TypeRegistry,
) -> Result<(), concordia_reflect::RegistryError> {
    concordia_terra::register_component::<Pose>(registry)?;
    concordia_terra::register_component::<AnimPlayer>(registry)
}
