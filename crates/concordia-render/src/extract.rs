//! The render snapshot: what to draw, copied from the ECS once per frame.

use std::sync::Arc;

use concordia_anim::{AnimLibrary, Pose, Skeleton};
use concordia_assets::{AssetRef, MeshRenderer, Skinned};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Vec3};
use concordia_scene::builtin::{Parent, Transform, world_transform, world_transforms};
use concordia_terra::{SimLod, World};

use crate::{Camera, DirectionalLight, Environment, PointLight, RenderError, SpotLight};

/// A mesh to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawItem {
    /// The entity.
    pub entity: EntityId,
    /// World matrix.
    pub world: Mat4,
    /// Mesh.
    pub mesh: AssetRef,
    /// Material.
    pub material: AssetRef,
    /// Rendered into shadow maps.
    pub cast_shadows: bool,
    /// The shadow cache's hint (`MeshRenderer.mobility`, M4 4.7b).
    pub mobility: concordia_assets::Mobility,
    /// The entity whose pose deforms it (itself or an ancestor with the same skin), if any.
    pub skin: Option<EntityId>,
    /// How it is drawn by distance from the camera, and whose it is: its own, else its nearest
    /// ancestor's with the same skin (a character's parts follow the bind mesh; 2.4b.2).
    pub representation: Option<(EntityId, Arc<crate::Representation>)>,
    /// Drawn as its representation's cheaper mesh this frame (set by [`RenderWorld::represented`]).
    pub instanced: bool,
    /// Its simulation LOD level (its own `SimLod`, else its pose owner's; the `lod` view mode).
    pub sim_lod: Option<u8>,
}

/// A posed character as extracted: its skeleton, pose and world matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct PoseView {
    /// The posed entity.
    pub entity: EntityId,
    /// World matrix of the posed entity (every draw it deforms is placed here).
    pub world: Mat4,
    /// The skeleton the pose belongs to.
    pub skeleton: Arc<Skeleton>,
    /// Each joint's local transform.
    pub pose: Pose,
}

/// A posed character as drawn: its world matrix, joint palette (bind mesh space) and joints.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinView {
    /// The posed entity.
    pub entity: EntityId,
    /// Its world matrix.
    pub world: Mat4,
    /// One skinning matrix per joint ([`Skeleton::palette`]).
    pub palette: Vec<Mat4>,
    /// Each joint's matrix in the entity's space ([`Skeleton::entity_space`]; the skeleton overlay).
    pub joints: Vec<Mat4>,
    /// The skeleton (joint names and parents).
    pub skeleton: Arc<Skeleton>,
}

/// A light.
#[derive(Debug, Clone, PartialEq)]
pub struct LightView {
    /// The entity.
    pub entity: EntityId,
    /// Unit direction the light travels (the entity's -Z).
    pub direction: Vec3,
    /// The component.
    pub light: DirectionalLight,
}

/// Point or spot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PunctualKind {
    /// Omnidirectional.
    Point,
    /// Cone with inner and outer half-angles (radians).
    Spot {
        /// Full intensity inside.
        inner: f32,
        /// Zero outside.
        outer: f32,
    },
}

/// A point or spot light in world space.
#[derive(Debug, Clone, PartialEq)]
pub struct PunctualLight {
    /// The entity.
    pub entity: EntityId,
    /// World position.
    pub position: Vec3,
    /// Unit direction (the entity's -Z; spots only).
    pub direction: Vec3,
    /// Kind.
    pub kind: PunctualKind,
    /// Linear color.
    pub color: [f32; 3],
    /// Lumens.
    pub intensity_lumens: f32,
    /// Range.
    pub range: f32,
    /// Shadow settings (`None`: unshadowed; M4 4.7c).
    pub shadows: Option<concordia_render_types::LocalShadowSettings>,
}

/// A camera.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraView {
    /// The entity.
    pub entity: EntityId,
    /// World matrix.
    pub world: Mat4,
    /// The component.
    pub camera: Camera,
}

/// Everything the renderer needs from one world, in entity-id order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderWorld {
    /// Meshes.
    pub draws: Vec<DrawItem>,
    /// Directional lights.
    pub lights: Vec<LightView>,
    /// Point and spot lights.
    pub punctual: Vec<PunctualLight>,
    /// The first environment (sky light and background).
    pub environment: Option<Environment>,
    /// Cameras.
    pub cameras: Vec<CameraView>,
    /// Posed characters as extracted, in entity-id order.
    pub poses: Vec<PoseView>,
    /// Posed characters as drawn (palettes), in entity-id order.
    pub skins: Vec<SkinView>,
}

impl RenderWorld {
    /// Copies render components and world transforms out of `world`.
    pub fn extract(world: &World) -> Self {
        Self::assemble_owned(
            placements(world)
                .iter()
                .map(|(&entity, &matrix)| EntityItems::extract(world, entity, matrix)),
        )
    }

    /// Joins per-entity items (in entity-id order) into a render world (palettes are built per
    /// frame by [`RenderWorld::represented`]).
    pub fn assemble<'a>(items: impl Iterator<Item = &'a EntityItems>) -> Self {
        Self::assemble_owned(items.cloned())
    }

    /// [`RenderWorld::assemble`] from owned items, without copying them.
    pub fn assemble_owned(items: impl IntoIterator<Item = EntityItems>) -> Self {
        let mut out = Self::default();
        for item in items {
            out.poses.extend(item.pose);
            out.draws.extend(item.draw);
            out.lights.extend(item.light);
            out.punctual.extend(item.punctual);
            if out.environment.is_none() {
                out.environment = item.environment;
            }
            out.cameras.extend(item.camera);
        }
        out.drop_missing_poses();
        out
    }

    /// Draws whose posed entity isn't extracted (removed) use their bind mesh.
    fn drop_missing_poses(&mut self) {
        let poses = &self.poses;
        for draw in &mut self.draws {
            if let Some(owner) = draw.skin
                && poses.binary_search_by_key(&owner, |p| p.entity).is_err()
            {
                draw.skin = None;
            }
        }
    }

    /// The requested camera, or the highest-priority one (ties: lowest id).
    pub fn camera(&self, requested: Option<EntityId>) -> Result<&CameraView, RenderError> {
        crate::pointer::choose_camera(&self.cameras, requested)
    }
}

/// One entity's share of a [`RenderWorld`], plus its parent (for incremental updates).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntityItems {
    /// Its mesh.
    pub draw: Option<DrawItem>,
    /// Its directional light.
    pub light: Option<LightView>,
    /// Its point and spot lights.
    pub punctual: Vec<PunctualLight>,
    /// Its environment.
    pub environment: Option<Environment>,
    /// Its camera.
    pub camera: Option<CameraView>,
    /// Its pose, when it is a posed character.
    pub pose: Option<PoseView>,
    /// Its `Parent`, if any.
    pub parent: Option<EntityId>,
}

/// Every rendered entity's matrix, in id order (see [`placement`]).
pub(crate) fn placements(world: &World) -> std::collections::BTreeMap<EntityId, Mat4> {
    let mut placed: std::collections::BTreeMap<EntityId, Mat4> = world_transforms(world)
        .iter()
        .map(|(&entity, &matrix)| (entity, matrix))
        .collect();
    for id in world.entity_ids_sorted() {
        if world.get::<Environment>(id).is_some() {
            placed.entry(id).or_insert(Mat4::IDENTITY);
        }
    }
    placed
}

/// Where `id` is drawn from: its world matrix, or the origin for an `Environment` without a
/// `Transform` (it has no position); `None` when it is not rendered.
pub(crate) fn placement(world: &World, id: EntityId) -> Option<Mat4> {
    match world.get::<Transform>(id) {
        Some(_) => world_transform(world, id),
        None => world.get::<Environment>(id).map(|_| Mat4::IDENTITY),
    }
}

/// `entity`'s `Representation` and whose it is: its own, else the nearest ancestor's whose
/// `Skinned` names the same skin.
fn representation_of(
    world: &World,
    entity: EntityId,
) -> Option<(EntityId, Arc<crate::Representation>)> {
    if let Some(own) = world.get::<crate::Representation>(entity) {
        return Some((entity, Arc::new(own.clone())));
    }
    let skin = &world.get::<Skinned>(entity)?.skin;
    let mut seen = std::collections::BTreeSet::from([entity]);
    let mut at = world.get::<Parent>(entity).map(|p| p.0);
    while let Some(id) = at.filter(|id| seen.insert(*id)) {
        if world.get::<Skinned>(id).is_none_or(|s| s.skin != *skin) {
            return None;
        }
        if let Some(r) = world.get::<crate::Representation>(id) {
            return Some((id, Arc::new(r.clone())));
        }
        at = world.get::<Parent>(id).map(|p| p.0);
    }
    None
}

/// `entity`'s simulation LOD level: its own `SimLod`, else its pose owner's.
fn sim_lod_of(world: &World, entity: EntityId) -> Option<u8> {
    let own = world.get::<SimLod>(entity);
    own.or_else(|| world.get::<SimLod>(pose_owner(world, entity)?))
        .map(|l| l.level)
}

impl SkinView {
    /// The palette of `pose`, or `None` when the pose doesn't fit its skeleton.
    pub(crate) fn of(pose: &PoseView) -> Option<Self> {
        let joints = pose.skeleton.entity_space(&pose.pose).ok()?;
        Some(Self {
            entity: pose.entity,
            world: pose.world,
            palette: pose.skeleton.skin_matrices(&joints),
            joints,
            skeleton: pose.skeleton.clone(),
        })
    }
}

/// The skeleton of `skin` in the world's animation library (play worlds only, ADR-0074).
fn skeleton_of(world: &World, skin: &AssetRef) -> Option<Arc<Skeleton>> {
    world.resource::<Arc<AnimLibrary>>()?.shared_skeleton(skin)
}

/// `entity`'s pose, when it is a posed character with a known skeleton.
fn pose_view(world: &World, entity: EntityId, matrix: Mat4) -> Option<PoseView> {
    let pose = world.get::<Pose>(entity)?;
    let skeleton = skeleton_of(world, &world.get::<Skinned>(entity)?.skin)?;
    Some(PoseView {
        entity,
        world: matrix,
        skeleton,
        pose: pose.clone(),
    })
}

/// The entity whose pose deforms `entity`: itself or its nearest ancestor with a pose and the
/// same skin (an import's extra mesh parts are children of the posed entity).
fn pose_owner(world: &World, entity: EntityId) -> Option<EntityId> {
    let skin = &world.get::<Skinned>(entity)?.skin;
    skeleton_of(world, skin)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut at = Some(entity);
    while let Some(id) = at.filter(|id| seen.insert(*id)) {
        let same = world.get::<Skinned>(id).is_some_and(|s| s.skin == *skin);
        if same && world.get::<Pose>(id).is_some() {
            return Some(id);
        }
        at = world.get::<Parent>(id).map(|p| p.0);
    }
    None
}

impl EntityItems {
    /// The render items of `entity`, whose world matrix is `matrix`.
    pub fn extract(world: &World, entity: EntityId, matrix: Mat4) -> Self {
        let position = matrix.transform_point3(Vec3::ZERO);
        let forward = matrix
            .transform_vector3(Vec3::NEG_Z)
            .normalize_or(Vec3::NEG_Z);
        let mut punctual = Vec::new();
        if let Some(p) = world.get::<PointLight>(entity) {
            punctual.push(PunctualLight {
                entity,
                position,
                direction: forward,
                kind: PunctualKind::Point,
                color: p.color,
                intensity_lumens: p.intensity_lumens,
                range: p.range,
                shadows: p.shadows,
            });
        }
        if let Some(s) = world.get::<SpotLight>(entity) {
            punctual.push(PunctualLight {
                entity,
                position,
                direction: forward,
                kind: PunctualKind::Spot {
                    inner: s.inner_angle.min(s.outer_angle),
                    outer: s.outer_angle,
                },
                color: s.color,
                intensity_lumens: s.intensity_lumens,
                range: s.range,
                shadows: s.shadows,
            });
        }
        Self {
            draw: world.get::<MeshRenderer>(entity).map(|r| DrawItem {
                entity,
                world: matrix,
                mesh: r.mesh.clone(),
                material: r.material.clone(),
                cast_shadows: r.cast_shadows,
                mobility: r.mobility,
                skin: pose_owner(world, entity),
                representation: representation_of(world, entity),
                instanced: false,
                sim_lod: sim_lod_of(world, entity),
            }),
            light: world
                .get::<DirectionalLight>(entity)
                .map(|light| LightView {
                    entity,
                    direction: forward,
                    light: light.clone(),
                }),
            punctual,
            environment: world.get::<Environment>(entity).cloned(),
            camera: world.get::<Camera>(entity).map(|camera| CameraView {
                entity,
                world: matrix,
                camera: camera.clone(),
            }),
            pose: pose_view(world, entity, matrix),
            parent: world.get::<Parent>(entity).map(|p| p.0),
        }
    }
}
