//! The render snapshot: what to draw, copied from the ECS once per frame.

use concordia_assets::{AssetRef, MeshRenderer};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Vec3};
use concordia_scene::builtin::world_transforms;
use concordia_terra::World;

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
}

impl RenderWorld {
    /// Copies render components and world transforms out of `world`.
    pub fn extract(world: &World) -> Self {
        let transforms = world_transforms(world);
        let items: Vec<EntityItems> = transforms
            .iter()
            .map(|(&entity, &matrix)| EntityItems::extract(world, entity, matrix))
            .collect();
        Self::assemble(items.iter())
    }

    /// Joins per-entity items (in entity-id order) into a render world.
    pub fn assemble<'a>(items: impl Iterator<Item = &'a EntityItems>) -> Self {
        let mut out = Self::default();
        for item in items {
            out.draws.extend(item.draw.iter().cloned());
            out.lights.extend(item.light.iter().cloned());
            out.punctual.extend(item.punctual.iter().cloned());
            if out.environment.is_none() {
                out.environment.clone_from(&item.environment);
            }
            out.cameras.extend(item.camera.iter().cloned());
        }
        out
    }

    /// The requested camera, or the highest-priority one (ties: lowest id).
    pub fn camera(&self, requested: Option<EntityId>) -> Result<&CameraView, RenderError> {
        match requested {
            Some(id) => self
                .cameras
                .iter()
                .find(|c| c.entity == id)
                .ok_or(RenderError::UnknownCamera(id)),
            None => self
                .cameras
                .iter()
                .max_by_key(|c| (c.camera.priority, std::cmp::Reverse(c.entity)))
                .ok_or(RenderError::NoCamera),
        }
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
    /// Its `Parent`, if any.
    pub parent: Option<EntityId>,
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
            });
        }
        Self {
            draw: world.get::<MeshRenderer>(entity).map(|r| DrawItem {
                entity,
                world: matrix,
                mesh: r.mesh.clone(),
                material: r.material.clone(),
                cast_shadows: r.cast_shadows,
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
            parent: world
                .get::<concordia_scene::builtin::Parent>(entity)
                .map(|p| p.0),
        }
    }
}
