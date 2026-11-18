//! The render snapshot: what to draw, copied from the ECS once per frame.

use concordia_assets::{AssetRef, MeshRenderer};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Vec3};
use concordia_scene::builtin::world_transforms;
use concordia_terra::World;

use crate::{Camera, DirectionalLight, PointLight, RenderError, SpotLight};

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
    /// Cameras.
    pub cameras: Vec<CameraView>,
}

impl RenderWorld {
    /// Copies render components and world transforms out of `world`.
    pub fn extract(world: &World) -> Self {
        let transforms = world_transforms(world);
        let mut out = Self::default();
        for (&entity, &matrix) in transforms.iter() {
            if let Some(r) = world.get::<MeshRenderer>(entity) {
                out.draws.push(DrawItem {
                    entity,
                    world: matrix,
                    mesh: r.mesh.clone(),
                    material: r.material.clone(),
                    cast_shadows: r.cast_shadows,
                });
            }
            if let Some(light) = world.get::<DirectionalLight>(entity) {
                let direction = matrix
                    .transform_vector3(Vec3::NEG_Z)
                    .normalize_or(Vec3::NEG_Z);
                out.lights.push(LightView {
                    entity,
                    direction,
                    light: light.clone(),
                });
            }
            let position = matrix.transform_point3(Vec3::ZERO);
            let forward = matrix
                .transform_vector3(Vec3::NEG_Z)
                .normalize_or(Vec3::NEG_Z);
            if let Some(p) = world.get::<PointLight>(entity) {
                out.punctual.push(PunctualLight {
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
                let kind = PunctualKind::Spot {
                    inner: s.inner_angle.min(s.outer_angle),
                    outer: s.outer_angle,
                };
                out.punctual.push(PunctualLight {
                    entity,
                    position,
                    direction: forward,
                    kind,
                    color: s.color,
                    intensity_lumens: s.intensity_lumens,
                    range: s.range,
                });
            }
            if let Some(camera) = world.get::<Camera>(entity) {
                out.cameras.push(CameraView {
                    entity,
                    world: matrix,
                    camera: camera.clone(),
                });
            }
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
