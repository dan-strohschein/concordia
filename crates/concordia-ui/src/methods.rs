//! `ui.inspect`: computed layout as numbers, so agents check UI without an image.

use std::collections::BTreeSet;

use concordia_api::{
    ApiError, ApiErrorKind, CallCtx, Engine, EntityRef, Method, MethodInfo, WorldSel,
};
use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::assets::DbAssets;
use crate::geom::Rect;
use crate::layout::{UNBOUNDED, UiLayout, compute_layout};
use crate::module::{UiCore, UiShared, scene_and_db};
use crate::root::CanvasMap;
use crate::scene::{UiElement, UiScene};
use crate::style::ResolvedStyle;

/// Every UI method.
pub fn all() -> Vec<MethodInfo> {
    vec![MethodInfo::of::<UiInspect>()]
}

/// Lay out a world's UI and report every node's rects, resolved style and text lines (read
/// only; layout is presentation and never changes the world).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct UiInspect;

/// `ui.inspect` params.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct UiInspectParams {
    /// Which world (default edit).
    pub world: Option<WorldSel>,
    /// Only this entity and its descendants (default: every root).
    pub entity: Option<EntityRef>,
    /// Viewport width in logical pixels (1..=16384; default: the first root's canvas).
    pub width: Option<u32>,
    /// Viewport height in logical pixels (1..=16384; default: the first root's canvas).
    pub height: Option<u32>,
}

/// A root's canvas mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RootReport {
    /// The root entity.
    pub entity: EntityId,
    /// UI space → viewport.
    pub map: CanvasMap,
}

/// One text line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct LineReport {
    /// Its text.
    pub text: String,
    /// Left edge after alignment, from the content box.
    pub x: f32,
    /// Top, from the content box.
    pub top: f32,
    /// Baseline, from the content box.
    pub baseline: f32,
    /// Width without trailing spaces.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// One node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct NodeReport {
    /// The entity.
    pub entity: EntityId,
    /// Its root.
    pub root: EntityId,
    /// Its box (UI space).
    pub rect: Rect,
    /// Its box less padding.
    pub content: Rect,
    /// What clips it, if anything does.
    pub clip: Option<Rect>,
    /// Opacity with its ancestors'.
    pub opacity: f32,
    /// Its resolved style.
    pub style: ResolvedStyle,
    /// Its text lines, if it is a label.
    pub lines: Vec<LineReport>,
}

/// A component that failed its checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ProblemReport {
    /// The entity.
    pub entity: EntityId,
    /// What is wrong (component.field: reason).
    pub error: String,
}

/// `ui.inspect` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct UiInspectResult {
    /// The viewport laid out for.
    pub viewport: [f32; 2],
    /// Roots, back to front.
    pub roots: Vec<RootReport>,
    /// Nodes, back to front.
    pub nodes: Vec<NodeReport>,
    /// Components dropped for failing their checks.
    pub problems: Vec<ProblemReport>,
}

impl Method for UiInspect {
    const NAME: &'static str = "ui.inspect";
    const MUTATES: bool = false;
    type Params = UiInspectParams;
    type Result = UiInspectResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: UiInspectParams,
    ) -> Result<UiInspectResult, ApiError> {
        let world = p.world.unwrap_or_default();
        let only = match &p.entity {
            Some(e) => Some(e.resolve(engine.world(world)?)?),
            None => None,
        };
        let shared = engine.state::<UiShared>().cloned().ok_or_else(|| {
            ApiError::new(ApiErrorKind::Internal, "the UI module's state is missing")
        })?;
        let (scene, db) = scene_and_db(engine, world)
            .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the world is missing"))?;
        let viewport = viewport(&p, &scene)?;
        let mut core = shared.lock();
        let UiCore { context, cache } = &mut *core;
        let mut assets = DbAssets { db, cache };
        let layout = compute_layout(&scene, &mut context.fonts, &mut assets, viewport);
        let keep = only.map(|id| subtree(&scene, id));
        Ok(report(&layout, viewport, keep.as_ref()))
    }
}

/// The viewport: both sides given, or the first root's canvas (1280 × 720 with no roots).
fn viewport(p: &UiInspectParams, scene: &UiScene) -> Result<[f32; 2], ApiError> {
    let canvas = scene
        .trees
        .first()
        .map_or([1280.0, 720.0], |t| t.root.canvas);
    let side = |v: Option<u32>, fallback: f32, at: &str| -> Result<f32, ApiError> {
        match v {
            None => Ok(fallback),
            Some(n) if (1..=16_384).contains(&n) => Ok(n as f32),
            Some(n) => Err(ApiError::new(
                ApiErrorKind::InvalidParams,
                format!("{n} must be in 1..=16384"),
            )
            .at(at)),
        }
    };
    Ok([
        side(p.width, canvas[0], "/width")?,
        side(p.height, canvas[1], "/height")?,
    ])
}

/// `id` and every descendant, as the scene holds them.
fn subtree(scene: &UiScene, id: EntityId) -> BTreeSet<EntityId> {
    fn find(el: &UiElement, id: EntityId) -> Option<&UiElement> {
        if el.entity == id {
            return Some(el);
        }
        el.children.iter().find_map(|c| find(c, id))
    }
    fn all(el: &UiElement, out: &mut BTreeSet<EntityId>) {
        out.insert(el.entity);
        el.children.iter().for_each(|c| all(c, out));
    }
    let mut out = BTreeSet::new();
    if let Some(el) = scene.trees.iter().find_map(|t| find(&t.element, id)) {
        all(el, &mut out);
    }
    out
}

fn report(
    layout: &UiLayout,
    viewport: [f32; 2],
    keep: Option<&BTreeSet<EntityId>>,
) -> UiInspectResult {
    let nodes = layout
        .order
        .iter()
        .filter(|id| keep.is_none_or(|k| k.contains(id)))
        .map(|id| {
            let n = &layout.nodes[id];
            let lines = layout.texts.get(id).map_or_else(Vec::new, |b| {
                b.lines
                    .iter()
                    .map(|l| LineReport {
                        text: l.text.clone(),
                        x: l.x,
                        top: l.top,
                        baseline: l.baseline,
                        width: l.width,
                        height: l.height,
                    })
                    .collect()
            });
            NodeReport {
                entity: *id,
                root: n.root,
                rect: n.rect,
                content: n.content,
                clip: (n.clip != UNBOUNDED).then_some(n.clip),
                opacity: n.opacity,
                style: n.style.clone(),
                lines,
            }
        })
        .collect();
    UiInspectResult {
        viewport,
        roots: layout
            .roots
            .iter()
            .map(|r| RootReport {
                entity: r.entity,
                map: r.map,
            })
            .collect(),
        nodes,
        problems: layout
            .problems
            .iter()
            .map(|(entity, e)| ProblemReport {
                entity: *entity,
                error: e.to_string(),
            })
            .collect(),
    }
}
