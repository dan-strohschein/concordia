//! Building a [`Scene`] from a world and its assets: what the renderer would draw, read the same
//! way (world transforms, entity order, the camera rule), without depending on the renderer.

use std::collections::BTreeMap;

use concordia_assets::{AssetRef, AssetSource, MeshRenderer};
use concordia_core::id::EntityId;
use concordia_core::math::{Mat4, Vec3};
use concordia_render_types::{
    Camera, DirectionalLight, Environment, PointLight, Projection, SpotLight, pick_camera,
};
use concordia_scene::builtin::world_transforms;
use concordia_terra::World;

use crate::env::EnvMap;
use crate::scene::{MaterialId, MeshId, SceneBuilder};
use crate::{PathTraceError, Scene};

/// Which viewpoint to render.
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// A camera entity (None: the highest priority, as the renderer chooses).
    Camera(Option<concordia_core::id::EntityId>),
    /// A free viewpoint; exposure and tone mapping come from the scene's chosen camera.
    Look {
        /// Eye.
        eye: Vec3,
        /// Point looked at.
        target: Vec3,
        /// Projection.
        projection: Projection,
    },
}

/// The scene `world` shows from `view`, its assets loaded from `assets`.
pub fn scene_from_world(
    world: &World,
    assets: &mut dyn AssetSource,
    view: &View,
) -> Result<Scene, PathTraceError> {
    let transforms = world_transforms(world);
    let mut reader = Reader {
        builder: SceneBuilder::new(),
        meshes: BTreeMap::new(),
        materials: BTreeMap::new(),
        cameras: Vec::new(),
        sky: None,
    };
    for id in world.entity_ids_sorted() {
        if reader.sky.is_none()
            && let Some(env) = world.get::<Environment>(id)
        {
            reader.sky = Some(env.clone());
        }
        let Some(&placed) = transforms.get(id) else {
            continue;
        };
        reader.entity(world, assets, id, placed)?;
    }
    if let Some(sky) = reader.sky.take() {
        let texture = assets
            .texture(&sky.map)
            .map_err(|e| PathTraceError::Environment(format!("{}: {e}", sky.map)))?;
        let pixels = texture
            .linear_rgb()
            .map_err(|e| PathTraceError::Environment(format!("{}: {e}", sky.map)))?;
        let map = EnvMap::new(texture.width, texture.height, &pixels, sky.intensity)?;
        reader.builder.environment_with_background(map, sky.skybox);
    }
    reader.camera(view)?;
    reader.builder.build()
}

/// Reads entities into a builder, sharing meshes and materials used more than once.
struct Reader {
    builder: SceneBuilder,
    meshes: BTreeMap<AssetRef, MeshId>,
    materials: BTreeMap<AssetRef, MaterialId>,
    cameras: Vec<(EntityId, Mat4, Camera)>,
    sky: Option<Environment>,
}

impl Reader {
    /// What entity `id`, placed at `world`, adds: its mesh, lights and camera.
    fn entity(
        &mut self,
        w: &World,
        assets: &mut dyn AssetSource,
        id: EntityId,
        world: Mat4,
    ) -> Result<(), PathTraceError> {
        if let Some(draw) = w.get::<MeshRenderer>(id) {
            let mesh = self.mesh(assets, id, &draw.mesh)?;
            let material = self.material(assets, id, &draw.material)?;
            self.builder.instance(mesh, world, material);
        }
        let at = world.transform_point3(Vec3::ZERO);
        let forward = world.transform_vector3(Vec3::NEG_Z);
        if let Some(light) = w.get::<PointLight>(id) {
            self.builder.point_light(at, light);
        }
        if let Some(light) = w.get::<SpotLight>(id) {
            self.builder.spot_light(at, forward, light);
        }
        if let Some(light) = w.get::<DirectionalLight>(id) {
            self.builder.sun(forward, light);
        }
        if let Some(camera) = w.get::<Camera>(id) {
            self.cameras.push((id, world, camera.clone()));
        }
        Ok(())
    }

    fn mesh(
        &mut self,
        assets: &mut dyn AssetSource,
        id: EntityId,
        r: &AssetRef,
    ) -> Result<MeshId, PathTraceError> {
        if let Some(m) = self.meshes.get(r) {
            return Ok(*m);
        }
        let data = assets
            .mesh(r)
            .map_err(|e| PathTraceError::Mesh(format!("entity {id}: {r}: {e}")))?;
        let m = self
            .builder
            .mesh(&data)
            .map_err(|e| PathTraceError::Mesh(format!("entity {id}: {r}: {e}")))?;
        self.meshes.insert(r.clone(), m);
        Ok(m)
    }

    fn material(
        &mut self,
        assets: &mut dyn AssetSource,
        id: EntityId,
        r: &AssetRef,
    ) -> Result<MaterialId, PathTraceError> {
        if let Some(m) = self.materials.get(r) {
            return Ok(*m);
        }
        let data = assets
            .material(r)
            .map_err(|e| PathTraceError::Material(format!("entity {id}: {r}: {e}")))?;
        let m = self
            .builder
            .material(&data)
            .map_err(|e| PathTraceError::Material(format!("entity {id}: {r}: {e}")))?;
        self.materials.insert(r.clone(), m);
        Ok(m)
    }

    /// Sets the builder's camera from `view` (the renderer's rule for which camera).
    fn camera(&mut self, view: &View) -> Result<(), PathTraceError> {
        let requested = match view {
            View::Camera(id) => *id,
            View::Look { .. } => None,
        };
        let keys = self.cameras.iter().map(|(id, _, c)| (*id, c.priority));
        let chosen =
            pick_camera(keys, requested).and_then(|id| self.cameras.iter().find(|c| c.0 == id));
        match (view, chosen) {
            (View::Camera(_), Some((_, world, camera))) => self.builder.camera(*world, camera),
            (View::Camera(_), None) => return Err(PathTraceError::NoCamera),
            (
                View::Look {
                    eye,
                    target,
                    projection,
                },
                scene,
            ) => {
                let mut camera = scene.map(|c| c.2.clone()).unwrap_or_default();
                camera.projection = *projection;
                self.builder.camera(look(*eye, *target)?, &camera);
            }
        }
        Ok(())
    }
}

/// Camera → world for an eye looking at `target` (Y up; −Z forward).
fn look(eye: Vec3, target: Vec3) -> Result<Mat4, PathTraceError> {
    let forward = (target - eye).normalize_or_zero();
    if forward == Vec3::ZERO || !eye.is_finite() {
        return Err(PathTraceError::Settings(format!(
            "view eye {eye} and target {target} must differ"
        )));
    }
    let up = if forward.y.abs() > 0.999 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let right = forward.cross(up).normalize();
    Ok(Mat4::from_cols(
        right.extend(0.0),
        right.cross(forward).extend(0.0),
        (-forward).extend(0.0),
        eye.extend(1.0),
    ))
}
