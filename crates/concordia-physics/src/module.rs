//! The physics engine module: components, a Jolt world per play session, and `physics.raycast` (ADR-0046).

use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use concordia_api::{
    ApiError, ApiErrorKind, CallCtx, Engine, EngineModule, Method, MethodInfo, PlaySetup, WorldSel,
};
use concordia_core::id::EntityId;
use concordia_core::math::{Ray, Vec3};
use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_terra::{Phase, Schedule, SystemCtx, World, register_component, system};
use serde::{Deserialize, Serialize};

use crate::components::{CharacterController, CharacterMotion, Collider, RigidBody};
use crate::events::{ContactBegan, ContactEnded, ContactImpact};
use crate::layers::{LayerMask, PhysicsLayer};
use crate::runtime::PhysicsRuntime;
use crate::settings::PhysicsSettings;

/// Most threads a physics step uses (results are identical for any count).
const MAX_THREADS: usize = 8;

/// The play session's physics world, shared by the step system and `physics.raycast`.
#[derive(Debug, Clone)]
struct SharedRuntime(Arc<Mutex<PhysicsRuntime>>);

/// Read-only physics queries for processors (a play resource sharing the play session's runtime;
/// not hashed). Queries see the bodies as of the last physics step.
#[derive(Debug, Clone)]
pub struct PhysicsQueries(SharedRuntime);

impl PhysicsQueries {
    /// The nearest body hit within `max_distance` on `layers`.
    pub fn raycast(
        &self,
        ray: &Ray,
        max_distance: f32,
        layers: LayerMask,
    ) -> Option<crate::runtime::RayHit> {
        self.0.0.lock().ok()?.raycast(ray, max_distance, layers)
    }
}

/// Registers physics components and steps Jolt in every play world.
#[derive(Debug, Clone, Default)]
pub struct PhysicsModule {
    settings: PhysicsSettings,
}

impl PhysicsModule {
    /// A module using `settings` for every play session.
    pub fn new(settings: PhysicsSettings) -> Self {
        Self { settings }
    }
}

impl EngineModule for PhysicsModule {
    fn name(&self) -> &'static str {
        "physics"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        register_component::<RigidBody>(registry)?;
        register_component::<Collider>(registry)?;
        register_component::<CharacterController>(registry)?;
        register_component::<CharacterMotion>(registry)?;
        // Signals, so gameplay modules can receive them by name (2.10c.1).
        register_component::<ContactBegan>(registry)?;
        register_component::<ContactImpact>(registry)?;
        register_component::<crate::ContactReports>(registry)?;
        register_component::<crate::Impulse>(registry)?;
        register_component::<crate::Sleep>(registry)?;
        register_component::<crate::PhysicsDisabled>(registry)?;
        register_component::<ContactEnded>(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![MethodInfo::of::<PhysicsRaycast>()]
    }

    fn start_play(&self, world: &mut World) -> Result<PlaySetup, ApiError> {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(MAX_THREADS));
        let threads = NonZeroUsize::new(threads).unwrap_or(NonZeroUsize::MIN);
        let runtime = PhysicsRuntime::new(self.settings.clone(), threads)
            .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))?;
        let shared = SharedRuntime(Arc::new(Mutex::new(runtime)));
        world.insert_resource(PhysicsQueries(shared.clone()));
        let mut schedule = Schedule::new();
        schedule.add_in(Phase::Physics, step_system(shared.clone()));
        Ok(PlaySetup {
            schedule,
            state: Some(Box::new(shared)),
        })
    }
}

/// The fixed-step system: sync, step, write back; invalid bodies are logged once each.
fn step_system(shared: SharedRuntime) -> impl concordia_terra::System {
    let mut warned: BTreeSet<EntityId> = BTreeSet::new();
    system(
        "physics.step",
        move |world: &mut World, ctx: &SystemCtx<'_>| {
            let Ok(mut runtime) = shared.0.lock() else {
                return;
            };
            match runtime.tick(world, ctx.dt) {
                Ok(report) => {
                    for (entity, reason) in report.rejected {
                        if warned.insert(entity) {
                            tracing::warn!(target: "concordia::physics", %entity, reason, "body skipped");
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(target: "concordia::physics", error = %e, "physics step failed")
                }
            }
        },
    )
}

/// Cast a ray against physics bodies (edit or play world) and return the nearest hit, if any.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct PhysicsRaycast;

/// `physics.raycast` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct RaycastParams {
    /// Ray start, world space.
    pub origin: Vec3,
    /// Ray direction (any non-zero length).
    pub direction: Vec3,
    /// Longest hit distance in meters.
    #[reflect(range(min = 0.0, max = 1000000.0))]
    pub max_distance: f32,
    /// Layers to hit (default: all).
    pub layers: Option<Vec<u8>>,
    /// Which world (default edit; play uses the running simulation).
    pub world: Option<WorldSel>,
}

/// One hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RaycastHit {
    /// Entity id hit.
    pub entity: String,
    /// World-space hit point.
    pub point: Vec3,
    /// Surface normal.
    pub normal: Vec3,
    /// Distance from the origin.
    pub distance: f32,
}

/// `physics.raycast` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct RaycastResult {
    /// The nearest hit, or null.
    pub hit: Option<RaycastHit>,
}

impl Method for PhysicsRaycast {
    const NAME: &'static str = "physics.raycast";
    const MUTATES: bool = false;
    type Params = RaycastParams;
    type Result = RaycastResult;

    fn call(engine: &mut Engine, _: &CallCtx, p: RaycastParams) -> Result<RaycastResult, ApiError> {
        let ray =
            Ray::new(p.origin, p.direction).map_err(|e| invalid(&e.to_string(), "/direction"))?;
        let mask = mask(p.layers.as_deref())?;
        if !(p.max_distance.is_finite() && p.max_distance > 0.0) {
            return Err(invalid("max_distance must be positive", "/max_distance"));
        }
        let hit = match p.world.unwrap_or_default() {
            WorldSel::play => play_raycast(engine, &ray, p.max_distance, mask)?,
            WorldSel::edit => edit_raycast(engine, &ray, p.max_distance, mask)?,
        };
        Ok(RaycastResult {
            hit: hit.map(|h| RaycastHit {
                entity: h.entity.to_string(),
                point: h.point,
                normal: h.normal,
                distance: h.distance,
            }),
        })
    }
}

fn play_raycast(
    engine: &Engine,
    ray: &Ray,
    max: f32,
    mask: LayerMask,
) -> Result<Option<crate::runtime::RayHit>, ApiError> {
    engine.world(WorldSel::play)?;
    let shared = engine
        .play_state::<SharedRuntime>()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "physics module not started"))?;
    let runtime = shared
        .0
        .lock()
        .map_err(|_| ApiError::new(ApiErrorKind::Internal, "physics runtime poisoned"))?;
    Ok(runtime.raycast(ray, max, mask))
}

fn edit_raycast(
    engine: &Engine,
    ray: &Ray,
    max: f32,
    mask: LayerMask,
) -> Result<Option<crate::runtime::RayHit>, ApiError> {
    let world = engine.world(WorldSel::edit)?;
    let mut settings = PhysicsSettings::default();
    settings.max_bodies = u32::try_from(world.len().max(1))
        .unwrap_or(u32::MAX)
        .min(65_536);
    let (runtime, _) = PhysicsRuntime::snapshot(world, settings)
        .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))?;
    Ok(runtime.raycast(ray, max, mask))
}

fn mask(layers: Option<&[u8]>) -> Result<LayerMask, ApiError> {
    let Some(layers) = layers else {
        return Ok(LayerMask::ALL);
    };
    layers
        .iter()
        .enumerate()
        .try_fold(LayerMask::NONE, |m, (i, l)| {
            PhysicsLayer::new(*l)
                .map(|layer| m.with(layer))
                .ok_or_else(|| {
                    invalid(
                        &format!("layer {l} is not in 0..=15"),
                        &format!("/layers/{i}"),
                    )
                })
        })
}

fn invalid(message: &str, pointer: &str) -> ApiError {
    ApiError::new(ApiErrorKind::InvalidParams, message.to_owned()).at(pointer.to_owned())
}
