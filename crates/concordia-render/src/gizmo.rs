//! `viewport.gizmo`: the Move gizmo's drag, engine-side so native input and the page share it.
//! A drag is one transaction (one undo step) on the caller's session, as the caller's actor.

use concordia_api::{ApiError, ApiErrorKind, Audience, CallCtx, Engine, Method};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_scene::builtin::Transform;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::GizmoAxis;
use crate::module::{RenderState, redraw_viewport};

/// What a `viewport.gizmo` call does.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum GizmoAction {
    /// Press on an axis handle (needs `entity` and `axis`).
    begin,
    /// The pointer moved.
    r#move,
    /// Release: commit the move.
    end,
    /// Abandon the move.
    cancel,
}

/// A drag in progress.
#[derive(Debug, Clone)]
pub(crate) struct GizmoDrag {
    entity: EntityId,
    axis: Vec3,
    start: Transform,
    pointer: [f32; 2],
    txn: Value,
}

/// Drag the Move gizmo: begin on an axis handle, move with the pointer, end (one undo step) or cancel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportGizmo;

/// `viewport.gizmo` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ViewportGizmoParams {
    /// What to do.
    pub action: GizmoAction,
    /// The entity to move (begin; default: the shown gizmo's entity).
    #[serde(default)]
    pub entity: Option<EntityId>,
    /// The handle's axis (begin; default: the shown gizmo's handle under the pointer).
    #[serde(default)]
    pub axis: Option<GizmoAxis>,
    /// Pointer x in viewport pixels (begin, move).
    #[serde(default)]
    pub x: Option<f32>,
    /// Pointer y in viewport pixels (begin, move).
    #[serde(default)]
    pub y: Option<f32>,
    /// Viewport width in pixels (move).
    #[serde(default)]
    pub width: Option<f32>,
    /// Viewport height in pixels (move).
    #[serde(default)]
    pub height: Option<f32>,
    /// Caller's input tag, echoed in `viewport.stats` for the redrawn frame.
    #[serde(default)]
    pub seq: Option<u64>,
    /// When the input happened (Unix ms).
    #[serde(default)]
    pub input_time_ms: Option<f64>,
    /// Who is dragging (defaults to the session's actor).
    #[serde(default)]
    pub actor: Option<concordia_command::Actor>,
}

/// `viewport.gizmo` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ViewportGizmoResult {
    /// The entity's translation now (null after end or cancel, or a press that missed).
    pub translation: Option<Vec3>,
    /// The handle a begin grabbed (null if the press missed every handle).
    pub axis: Option<GizmoAxis>,
}

impl Method for ViewportGizmo {
    const NAME: &'static str = "viewport.gizmo";
    const MUTATES: bool = true;
    const AUDIENCE: Audience = Audience::editor;
    type Params = ViewportGizmoParams;
    type Result = ViewportGizmoResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: ViewportGizmoParams,
    ) -> Result<ViewportGizmoResult, ApiError> {
        let (translation, axis) = match p.action {
            GizmoAction::begin => match grabbed(engine, &p)? {
                Some((entity, axis)) => (Some(begin(engine, ctx, &p, entity, axis)?), Some(axis)),
                None => (None, None),
            },
            GizmoAction::r#move => (Some(drag(engine, ctx, &p)?), None),
            GizmoAction::end => (finish(engine, ctx, "txn.commit").map(|_| None)?, None),
            GizmoAction::cancel => (finish(engine, ctx, "txn.abort").map(|_| None)?, None),
        };
        if p.action != GizmoAction::begin {
            redraw_viewport(engine, p.seq, p.input_time_ms)?;
        }
        Ok(ViewportGizmoResult { translation, axis })
    }
}

fn invalid(message: &str, at: &str) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidParams, message).at(at)
}

fn state(engine: &mut Engine) -> Result<&mut RenderState, ApiError> {
    engine
        .state_mut::<RenderState>()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the render module has no state"))
}

fn pointer(p: &ViewportGizmoParams) -> Result<[f32; 2], ApiError> {
    match (p.x, p.y) {
        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok([x, y]),
        _ => Err(invalid("x and y must be finite pixel coordinates", "/x")),
    }
}

/// The entity and handle a begin grabs: as given, or the handle of the shown gizmo under the
/// pointer (`None` when the press misses every handle).
fn grabbed(
    engine: &mut Engine,
    p: &ViewportGizmoParams,
) -> Result<Option<(EntityId, GizmoAxis)>, ApiError> {
    if let (Some(entity), Some(axis)) = (p.entity, p.axis) {
        return Ok(Some((entity, axis)));
    }
    let [x, y] = pointer(p)?;
    let size = match (p.width, p.height) {
        (Some(w), Some(h)) if w >= 1.0 && h >= 1.0 => [w, h],
        _ => {
            return Err(invalid(
                "finding the handle needs the viewport width and height (≥ 1)",
                "/width",
            ));
        }
    };
    crate::module::refresh_anchor(engine);
    let state = state(engine)?;
    let anchor = *state
        .anchor
        .lock()
        .map_err(|_| ApiError::new(ApiErrorKind::Internal, "gizmo anchor poisoned"))?;
    let Some((entity, at)) = anchor.filter(|(e, _)| p.entity.is_none_or(|want| want == *e)) else {
        return Ok(None);
    };
    let handles = crate::gizmo_handles(&state.camera.view(), at, size[0], size[1]);
    Ok(crate::gizmo_hit(&handles, x, y).map(|axis| (entity, axis)))
}

fn begin(
    engine: &mut Engine,
    ctx: &CallCtx,
    p: &ViewportGizmoParams,
    entity: EntityId,
    axis: GizmoAxis,
) -> Result<Vec3, ApiError> {
    let pointer = pointer(p)?;
    if state(engine)?.gizmo.is_some() {
        finish(engine, ctx, "txn.abort")?;
    }
    let value = engine.call(
        ctx.session,
        "component.get",
        json!({"entity": entity, "component": "Transform"}),
    )?;
    let start: Transform = serde_json::from_value(value["value"].clone()).map_err(|e| {
        ApiError::new(
            ApiErrorKind::ValueInvalid,
            format!("entity {entity} has no usable Transform: {e}"),
        )
    })?;
    let txn = engine.call(
        ctx.session,
        "txn.begin",
        json!({"label": "Move", "actor": actor(ctx)}),
    )?["txn"]
        .clone();
    state(engine)?.gizmo = Some(GizmoDrag {
        entity,
        axis: axis.vector(),
        start,
        pointer,
        txn,
    });
    Ok(start.translation)
}

fn drag(engine: &mut Engine, ctx: &CallCtx, p: &ViewportGizmoParams) -> Result<Vec3, ApiError> {
    let [x, y] = pointer(p)?;
    let size = match (p.width, p.height) {
        (Some(w), Some(h)) if w >= 1.0 && h >= 1.0 => [w, h],
        _ => {
            return Err(invalid(
                "move needs the viewport width and height (≥ 1)",
                "/width",
            ));
        }
    };
    let state = state(engine)?;
    let drag = state.gizmo.clone().ok_or_else(|| {
        ApiError::new(ApiErrorKind::InvalidRequest, "no gizmo drag in progress")
            .suggest(["call viewport.gizmo with action begin first".to_owned()])
    })?;
    let view = state.camera.view();
    let delta = [x - drag.pointer[0], y - drag.pointer[1]];
    let t = crate::axis_delta(
        &view,
        drag.start.translation,
        drag.axis,
        delta,
        size[0],
        size[1],
    );
    let moved = Transform {
        translation: drag.start.translation + drag.axis * t,
        ..drag.start
    };
    engine.call(
        ctx.session,
        "component.set",
        json!({"entity": drag.entity, "component": "Transform", "value": moved, "actor": actor(ctx)}),
    )?;
    Ok(moved.translation)
}

/// Commits or aborts the drag's transaction.
fn finish(engine: &mut Engine, ctx: &CallCtx, method: &str) -> Result<(), ApiError> {
    let drag = state(engine)?
        .gizmo
        .take()
        .ok_or_else(|| ApiError::new(ApiErrorKind::InvalidRequest, "no gizmo drag in progress"))?;
    engine.call(
        ctx.session,
        method,
        json!({"txn": drag.txn, "actor": actor(ctx)}),
    )?;
    Ok(())
}

/// The caller's actor, for the edits made on its behalf.
fn actor(ctx: &CallCtx) -> Value {
    ctx.actor
        .as_ref()
        .map_or(Value::Null, |a| json!(a.to_string()))
}
