//! `viewport.gizmo`: the Move gizmo's drag, engine-side so native input and the page share it.
//! A drag is one transaction (one undo step) on the caller's session, as the caller's actor.

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method};
use concordia_core::id::EntityId;
use concordia_core::math::Vec3;
use concordia_reflect::Reflect;
use concordia_scene::builtin::Transform;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::module::{RenderState, redraw_viewport};

/// A gizmo axis.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum GizmoAxis {
    /// World +X.
    x,
    /// World +Y.
    y,
    /// World +Z.
    z,
}

impl GizmoAxis {
    fn vector(self) -> Vec3 {
        match self {
            Self::x => Vec3::X,
            Self::y => Vec3::Y,
            Self::z => Vec3::Z,
        }
    }
}

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
    /// The entity to move (begin).
    #[serde(default)]
    pub entity: Option<EntityId>,
    /// The handle's axis (begin).
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
    /// The entity's translation now (null after end or cancel).
    pub translation: Option<Vec3>,
}

impl Method for ViewportGizmo {
    const NAME: &'static str = "viewport.gizmo";
    const MUTATES: bool = true;
    type Params = ViewportGizmoParams;
    type Result = ViewportGizmoResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: ViewportGizmoParams,
    ) -> Result<ViewportGizmoResult, ApiError> {
        let translation = match p.action {
            GizmoAction::begin => Some(begin(engine, ctx, &p)?),
            GizmoAction::r#move => Some(drag(engine, ctx, &p)?),
            GizmoAction::end => finish(engine, ctx, "txn.commit").map(|_| None)?,
            GizmoAction::cancel => finish(engine, ctx, "txn.abort").map(|_| None)?,
        };
        if p.action != GizmoAction::begin {
            redraw_viewport(engine, p.seq, p.input_time_ms)?;
        }
        Ok(ViewportGizmoResult { translation })
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

fn begin(engine: &mut Engine, ctx: &CallCtx, p: &ViewportGizmoParams) -> Result<Vec3, ApiError> {
    let entity = p
        .entity
        .ok_or_else(|| invalid("begin needs the entity", "/entity"))?;
    let axis = p
        .axis
        .ok_or_else(|| invalid("begin needs the axis", "/axis"))?;
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
