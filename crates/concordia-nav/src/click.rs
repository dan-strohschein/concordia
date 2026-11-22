//! Click-to-move (M2 phase 2.5b.2a, ADR-0078): `ClickToMove` and the `nav.click` processor, which
//! turns a click's pointer ray into its agent's target.

use std::collections::BTreeSet;

use concordia_core::id::EntityId;
use concordia_core::math::{Ray, Vec3};
use concordia_input::{ActionStates, ActionValue};
use concordia_physics::{LayerMask, PhysicsLayer, PhysicsQueries};
use concordia_reflect::Reflect;
use concordia_scene::builtin::Transform;
use concordia_terra::{Phase, Schedule, View, processor};
use serde::{Deserialize, Serialize};

use crate::agent::NavAgent;
use crate::agents::{PLAN, volumes};
use crate::components::NavMeshVolume;
use crate::module::NavMeshes;
use crate::plan::{Volume, volume_for};

/// Sets its entity's `NavAgent.target` where the `click` action points its `pointer` ray: the
/// nearest physics hit, snapped onto the navmesh (gameplay the engine ships).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(category = "Navigation", default)]
#[serde(default, deny_unknown_fields)]
pub struct ClickToMove {
    /// The `Button` action that clicks.
    pub click: String,
    /// The `Ray` action that points.
    pub pointer: String,
    /// Retarget every tick while the button is held (false: only when it is pressed).
    pub hold: bool,
    /// Physics layers the ray hits (empty: all).
    pub layers: Vec<u8>,
    /// Longest hit distance in meters.
    #[reflect(range(min = 0.0, max = 1000000.0))]
    pub max_distance: f32,
}

impl Default for ClickToMove {
    fn default() -> Self {
        Self {
            click: "move".into(),
            pointer: "pointer_ray".into(),
            hold: false,
            layers: Vec::new(),
            max_distance: 1000.0,
        }
    }
}

/// `nav.click`: turns clicks into targets, before `nav.plan`.
pub const CLICK: &str = "nav.click";

/// Adds `nav.click` to `PrePhysics`, before `nav.plan` (after input sampling, which input's module
/// registers first).
pub fn add_click_processor(schedule: &mut Schedule) {
    let mut warned: BTreeSet<EntityId> = BTreeSet::new();
    schedule.add_processor(
        processor(CLICK, Phase::PrePhysics)
            .reads::<ClickToMove>()
            .reads::<Transform>()
            .reads::<NavMeshVolume>()
            .writes::<NavAgent>()
            .reads_resource::<ActionStates>()
            .reads_resource::<PhysicsQueries>()
            .reads_resource::<NavMeshes>()
            .before(PLAN)
            .run(move |view: &mut View<'_>, _| click_all(view, &mut warned)),
    );
}

/// What a click needs from the world.
struct Scene {
    states: ActionStates,
    physics: PhysicsQueries,
    meshes: NavMeshes,
    volumes: Vec<Volume>,
}

/// Retargets every agent whose click fired this tick.
fn click_all(view: &mut View<'_>, warned: &mut BTreeSet<EntityId>) {
    let (Ok(states), Ok(physics)) = (
        view.resource::<ActionStates>().cloned(),
        view.resource::<PhysicsQueries>().cloned(),
    ) else {
        return;
    };
    let meshes = view.resource::<NavMeshes>().cloned().unwrap_or_default();
    let scene = Scene {
        states,
        physics,
        meshes,
        volumes: volumes(view),
    };
    let Ok(query) = view.query::<(&ClickToMove, &Transform, &mut NavAgent)>() else {
        return;
    };
    query.for_each(|id, (click, t, agent)| match fired_ray(click, &scene.states) {
        Err(problem) => {
            if warned.insert(id) {
                tracing::warn!(target: "concordia::nav", entity = %id, "ClickToMove on {id} is ignored: {problem}");
            }
        }
        Ok(Some(ray)) => {
            if let Some(target) = clicked_point(click, &ray, agent, t.translation, &scene) {
                agent.target = Some(target);
            }
        }
        Ok(None) => {}
    });
}

/// The pointer ray if the click fired this tick; an error names a bad setting.
fn fired_ray(click: &ClickToMove, states: &ActionStates) -> Result<Option<Ray>, String> {
    check(click)?;
    let button = states
        .get(&click.click)
        .ok_or_else(|| format!("there is no action {:?}", click.click))?;
    if !matches!(button.value, ActionValue::Button(_)) {
        return Err(format!("click {:?} must be a Button action", click.click));
    }
    let pointer = states
        .get(&click.pointer)
        .ok_or_else(|| format!("there is no action {:?}", click.pointer))?;
    let ActionValue::Ray { origin, direction } = pointer.value else {
        return Err(format!("pointer {:?} must be a Ray action", click.pointer));
    };
    let fired = if click.hold {
        button.pressed
    } else {
        button.just_pressed
    };
    Ok(fired.then(|| Ray::new(origin, direction).ok()).flatten())
}

/// Checks the reach and the layers.
fn check(click: &ClickToMove) -> Result<(), String> {
    if !(click.max_distance.is_finite() && click.max_distance > 0.0) {
        return Err(format!(
            "max_distance must be positive, got {}",
            click.max_distance
        ));
    }
    match click
        .layers
        .iter()
        .find(|l| PhysicsLayer::new(**l).is_none())
    {
        Some(bad) => Err(format!("layer {bad} is not in 0..=15")),
        None => Ok(()),
    }
}

/// The nearest physics hit along `ray`, snapped onto the agent's navmesh (none: a miss).
fn clicked_point(
    click: &ClickToMove,
    ray: &Ray,
    agent: &NavAgent,
    position: Vec3,
    scene: &Scene,
) -> Option<Vec3> {
    let hit = scene
        .physics
        .raycast(ray, click.max_distance, mask(&click.layers))?;
    let (_, mesh) = volume_for(agent, position, &scene.volumes, &scene.meshes)?;
    Some(mesh.nearest_point(hit.point)?.position)
}

/// The layers as a mask (empty: all); `check` has refused bad ones.
fn mask(layers: &[u8]) -> LayerMask {
    match layers.is_empty() {
        true => LayerMask::ALL,
        false => layers
            .iter()
            .filter_map(|l| PhysicsLayer::new(*l))
            .fold(LayerMask::NONE, LayerMask::with),
    }
}
