//! The lens (M2 phase 2.5b.2a, ADR-0078): a pixel under a camera becomes a world ray, as
//! `camera_ray` and the `viewport.pointer_ray` method.

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, EntityRef, Method, WorldSel};
use concordia_core::id::EntityId;
use concordia_core::math::{Ray, Vec2, Vec3};
use concordia_reflect::Reflect;
use concordia_scene::builtin::world_transform;
use concordia_terra::World;
use serde::{Deserialize, Serialize};

use crate::camera::view_matrix;
use crate::{Camera, CameraView, RenderError, View};

/// The world ray under `cursor` (logical pixels, origin top-left) in a `size` image seen by
/// `camera`, starting on its near plane.
pub fn camera_ray(camera: &CameraView, cursor: Vec2, size: Vec2) -> Result<Ray, RenderError> {
    let aspect = size.x / size.y;
    let clip_from_world = camera.camera.projection.matrix(aspect)? * view_matrix(&camera.world);
    concordia_input::pointer_ray(cursor, size, clip_from_world.inverse())
        .map_err(|e| RenderError::InvalidCamera(e.to_string()))
}

/// `world`'s camera `requested`, or its highest-priority one (ties: lowest id), with its world
/// matrix.
pub fn scene_camera(world: &World, requested: Option<EntityId>) -> Result<CameraView, RenderError> {
    let cameras = world.entity_ids_sorted().into_iter().filter_map(|entity| {
        let camera = world.get::<Camera>(entity)?.clone();
        let matrix = world_transform(world, entity)?;
        Some(CameraView {
            entity,
            world: matrix,
            camera,
        })
    });
    choose_camera(cameras, requested)
}

/// The camera `requested`, or the highest-priority one (ties: lowest id).
pub(crate) fn choose_camera<T: std::borrow::Borrow<CameraView>>(
    cameras: impl IntoIterator<Item = T>,
    requested: Option<EntityId>,
) -> Result<T, RenderError> {
    let mut cameras = cameras.into_iter();
    match requested {
        Some(id) => cameras
            .find(|c| c.borrow().entity == id)
            .ok_or(RenderError::UnknownCamera(id)),
        None => cameras
            .max_by_key(|c| {
                (
                    c.borrow().camera.priority,
                    std::cmp::Reverse(c.borrow().entity),
                )
            })
            .ok_or(RenderError::NoCamera),
    }
}

/// The world ray under a pixel of a camera's (or a view's) image: where a click points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportPointerRay;

/// `viewport.pointer_ray` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ViewportPointerRayParams {
    /// Pixel column from the left, in logical pixels (0..=width).
    pub x: f32,
    /// Pixel row from the top, in logical pixels (0..=height).
    pub y: f32,
    /// Image width in logical pixels.
    #[reflect(range(min = 1.0, max = 8192.0))]
    pub width: u32,
    /// Image height in logical pixels.
    #[reflect(range(min = 1.0, max = 8192.0))]
    pub height: u32,
    /// Camera entity; default: the highest-priority Camera.
    #[serde(default)]
    pub camera: Option<EntityRef>,
    /// Use this viewpoint instead of a camera entity (the editor's view).
    #[serde(default)]
    pub view: Option<View>,
    /// Which world: edit (default) or play.
    #[serde(default)]
    pub world: Option<WorldSel>,
}

/// `viewport.pointer_ray` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportPointerRayResult {
    /// Start point, on the camera's near plane.
    pub origin: Vec3,
    /// Unit direction.
    pub direction: Vec3,
}

impl Method for ViewportPointerRay {
    const NAME: &'static str = "viewport.pointer_ray";
    const MUTATES: bool = false;
    type Params = ViewportPointerRayParams;
    type Result = ViewportPointerRayResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: ViewportPointerRayParams,
    ) -> Result<ViewportPointerRayResult, ApiError> {
        check_cursor(&p)?;
        let world = engine.world(p.world.unwrap_or_default())?;
        let camera = match (&p.camera, &p.view) {
            (Some(_), Some(_)) => {
                return Err(invalid("pass either camera or view, not both", "/view"));
            }
            (_, Some(view)) => view_camera(view)?,
            (requested, None) => {
                let id = requested.as_ref().map(|c| c.resolve(world)).transpose()?;
                scene_camera(world, id).map_err(crate::module::api_error)?
            }
        };
        let size = Vec2::new(p.width as f32, p.height as f32);
        let ray =
            camera_ray(&camera, Vec2::new(p.x, p.y), size).map_err(crate::module::api_error)?;
        Ok(ViewportPointerRayResult {
            origin: ray.origin(),
            direction: ray.dir(),
        })
    }
}

/// A view as a camera at its eye.
fn view_camera(view: &View) -> Result<CameraView, ApiError> {
    let world = view
        .world_matrix()
        .map_err(|e| invalid(&e.to_string(), "/view"))?;
    let entity = crate::renderer::EDITOR_VIEW
        .parse()
        .map_err(|e| ApiError::new(ApiErrorKind::Internal, format!("editor view id: {e}")))?;
    Ok(CameraView {
        entity,
        world,
        camera: Camera {
            projection: view.projection,
            ..Camera::default()
        },
    })
}

fn check_cursor(p: &ViewportPointerRayParams) -> Result<(), ApiError> {
    let inside = |v: f32, max: u32| v.is_finite() && (0.0..=max as f32).contains(&v);
    if !inside(p.x, p.width) {
        return Err(invalid(
            &format!("x {} is outside 0..={}", p.x, p.width),
            "/x",
        ));
    }
    if !inside(p.y, p.height) {
        return Err(invalid(
            &format!("y {} is outside 0..={}", p.y, p.height),
            "/y",
        ));
    }
    Ok(())
}

fn invalid(message: &str, pointer: &str) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidParams, message.to_owned()).at(pointer.to_owned())
}
