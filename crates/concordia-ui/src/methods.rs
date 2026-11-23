//! `ui.inspect`: computed layout as numbers, so agents check UI without an image.

use std::collections::BTreeSet;

use concordia_api::{
    ApiError, ApiErrorKind, CallCtx, Engine, EntityRef, Method, MethodInfo, WorldSel,
};
use concordia_core::id::EntityId;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use concordia_input::ActionStates;

use crate::assets::DbAssets;
use crate::controls::UiState;
use crate::geom::Rect;
use crate::hit::{HitPart, Index, hit_with};
use crate::interact::UiInput;
use crate::layout::{UNBOUNDED, UiLayout, compute_layout};
use crate::module::{UiCore, UiShared, scene_and_db};
use crate::root::CanvasMap;
use crate::scene::{UiElement, UiScene};
use crate::style::Cursor;
use crate::style::ResolvedStyle;

/// Every UI method.
pub fn all() -> Vec<MethodInfo> {
    vec![MethodInfo::of::<UiInspect>(), MethodInfo::of::<UiHitTest>()]
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
    /// What input is doing to it, if it is a widget.
    pub state: Option<UiState>,
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
        let (scene, layout, viewport) = lay_out(engine, world, p.width, p.height)?;
        let keep = only.map(|id| subtree(&scene, id));
        Ok(report(&scene, &layout, viewport, keep.as_ref()))
    }
}

/// `world`'s UI laid out with the UI module's fonts and assets, for the given or default
/// viewport.
fn lay_out(
    engine: &mut Engine,
    world: WorldSel,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<(UiScene, UiLayout, [f32; 2]), ApiError> {
    let shared = engine
        .state::<UiShared>()
        .cloned()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the UI module's state is missing"))?;
    let input = engine
        .world(world)?
        .resource::<ActionStates>()
        .map(UiInput::read)
        .and_then(|i| i.viewport);
    let (scene, db) = scene_and_db(engine, world)
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the world is missing"))?;
    let viewport = viewport(width, height, input, &scene)?;
    let mut core = shared.lock();
    let UiCore { context, cache } = &mut *core;
    let mut assets = DbAssets { db, cache };
    let layout = compute_layout(&scene, &mut context.fonts, &mut assets, viewport);
    Ok((scene, layout, viewport))
}

/// Find the topmost UI node and widget under a viewport point, with its path from the root and
/// the cursor it asks for (read only). QA pairs it with `input.inject`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct UiHitTest;

/// `ui.hit_test` params.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(default, deny_unknown_fields)]
pub struct UiHitTestParams {
    /// X in viewport logical pixels.
    pub x: f32,
    /// Y in viewport logical pixels.
    pub y: f32,
    /// Which world (default edit).
    pub world: Option<WorldSel>,
    /// Viewport width in logical pixels (1..=16384; default: the play input's, else the first
    /// root's canvas).
    pub width: Option<u32>,
    /// Viewport height in logical pixels (1..=16384; default as `width`).
    pub height: Option<u32>,
}

/// `ui.hit_test` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct UiHitTestResult {
    /// The viewport laid out for.
    pub viewport: [f32; 2],
    /// The topmost node that takes the pointer (`None`: the pointer reaches the world).
    pub node: Option<EntityId>,
    /// The enabled widget that handles it.
    pub widget: Option<EntityId>,
    /// Which part of the widget.
    pub part: Option<HitPart>,
    /// Root to node.
    pub path: Vec<EntityId>,
    /// The point in the root's UI space.
    pub point: Option<[f32; 2]>,
    /// The cursor the widget (else the node) asks for.
    pub cursor: Cursor,
}

impl Method for UiHitTest {
    const NAME: &'static str = "ui.hit_test";
    const MUTATES: bool = false;
    type Params = UiHitTestParams;
    type Result = UiHitTestResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        p: UiHitTestParams,
    ) -> Result<UiHitTestResult, ApiError> {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return Err(
                ApiError::new(ApiErrorKind::InvalidParams, "x and y must be finite").at("/x"),
            );
        }
        let (scene, layout, viewport) =
            lay_out(engine, p.world.unwrap_or_default(), p.width, p.height)?;
        let index = Index::new(&scene);
        let hit = hit_with(&index, &layout, [p.x, p.y]);
        let cursor = hit
            .as_ref()
            .and_then(|h| layout.nodes.get(&h.widget.unwrap_or(h.node)))
            .map_or(Cursor::Arrow, |n| n.style.cursor);
        Ok(UiHitTestResult {
            viewport,
            node: hit.as_ref().map(|h| h.node),
            widget: hit.as_ref().and_then(|h| h.widget),
            part: hit.as_ref().map(|h| h.part),
            path: hit.as_ref().map(|h| h.path.clone()).unwrap_or_default(),
            point: hit.map(|h| h.point),
            cursor,
        })
    }
}

/// The viewport: the sides given, else the world's input viewport, else the first root's canvas
/// (1280 × 720 with no roots).
fn viewport(
    width: Option<u32>,
    height: Option<u32>,
    input: Option<[f32; 2]>,
    scene: &UiScene,
) -> Result<[f32; 2], ApiError> {
    let canvas = input.unwrap_or_else(|| {
        scene
            .trees
            .first()
            .map_or([1280.0, 720.0], |t| t.root.canvas)
    });
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
        side(width, canvas[0], "/width")?,
        side(height, canvas[1], "/height")?,
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
    scene: &UiScene,
    layout: &UiLayout,
    viewport: [f32; 2],
    keep: Option<&BTreeSet<EntityId>>,
) -> UiInspectResult {
    let index = Index::new(scene);
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
                state: index.elements.get(id).and_then(|e| e.state.clone()),
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
