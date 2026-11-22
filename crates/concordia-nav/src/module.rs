//! The nav engine module (`[nav]`): components, `nav.*`, play-world navmeshes and the `navmesh`
//! debug layer.

use std::collections::BTreeMap;
use std::sync::Arc;

use std::any::Any;
use std::num::NonZeroU32;

use concordia_api::{
    ApiError, DebugGeometry, Engine, EngineModule, MethodInfo, PlaySetup, WorldSel,
};
use concordia_core::id::EntityId;
use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_terra::World;

use crate::agents::{AgentConfig, add_agent_processors, insert_agent_states};
use crate::methods::{NavBuild, NavPathMethod, navmeshes};
use crate::navmesh::NavMesh;
use crate::service::{Builder, NavState};

/// The debug layer that draws navmeshes.
pub const NAVMESH_LAYER: &str = "navmesh";
/// The debug layer that draws agents' remaining paths.
pub const NAV_PATHS_LAYER: &str = "nav_paths";

/// The play world's navmeshes by volume, built when play starts (a resource; not hashed, since
/// it is derived from hashed state).
#[derive(Debug, Clone, Default)]
pub struct NavMeshes(Arc<BTreeMap<EntityId, Arc<NavMesh>>>);

impl NavMeshes {
    /// Navmeshes by volume id.
    pub fn new(meshes: impl IntoIterator<Item = (EntityId, Arc<NavMesh>)>) -> Self {
        Self(Arc::new(meshes.into_iter().collect()))
    }

    /// Volumes with a navmesh.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// No navmeshes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `volume`'s navmesh.
    pub fn get(&self, volume: EntityId) -> Option<&Arc<NavMesh>> {
        self.0.get(&volume)
    }

    /// Every navmesh by volume id.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &Arc<NavMesh>)> {
        self.0.iter().map(|(id, m)| (*id, m))
    }
}

/// Registers navigation components and serves `nav.build` and `nav.path`.
#[derive(Debug, Clone, Default)]
pub struct NavModule {
    agents: AgentConfig,
}

impl NavModule {
    /// The module.
    pub fn new() -> Self {
        Self::default()
    }

    /// The module with at most `plans_per_tick` agent path plans per tick.
    pub fn with_plans_per_tick(plans_per_tick: NonZeroU32) -> Self {
        Self {
            agents: AgentConfig { plans_per_tick },
        }
    }

    /// The agent processors' settings.
    pub fn agent_config(&self) -> AgentConfig {
        self.agents
    }
}

impl EngineModule for NavModule {
    fn name(&self) -> &'static str {
        "nav"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        crate::components::register_types(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![
            MethodInfo::of::<NavBuild>(),
            MethodInfo::of::<NavPathMethod>(),
        ]
    }

    fn engine_state(&self) -> Option<Box<dyn Any + Send>> {
        Some(Box::new(NavState::default()))
    }

    /// Builds (or loads) every volume's navmesh as the `NavMeshes` resource (a volume that fails
    /// is logged and left out, so play still starts), and adds the agent processors.
    fn start_play_in(&self, engine: &mut Engine, world: &mut World) -> Result<PlaySetup, ApiError> {
        let mut meshes = BTreeMap::new();
        if let Some(state) = engine.state::<NavState>() {
            let builder = Builder {
                state,
                project: engine.project(),
                threads: engine.threads(),
            };
            for volume in crate::service::volumes(world) {
                match builder.build(world, WorldSel::play, volume) {
                    Ok(built) => {
                        meshes.insert(volume, built.navmesh);
                    }
                    Err(e) => {
                        tracing::warn!(target: "concordia::nav", %volume, error = %e, "navmesh not built")
                    }
                }
            }
        }
        world.insert_resource(NavMeshes(Arc::new(meshes)));
        insert_agent_states(world);
        let mut setup = PlaySetup::default();
        add_agent_processors(&mut setup.schedule, self.agents);
        crate::click::add_click_processor(&mut setup.schedule);
        Ok(setup)
    }

    fn debug_layers(&self) -> Vec<&'static str> {
        vec![NAVMESH_LAYER, NAV_PATHS_LAYER]
    }

    fn debug_geometry(
        &self,
        engine: &Engine,
        world: WorldSel,
        layer: &str,
    ) -> Option<DebugGeometry> {
        if layer == NAV_PATHS_LAYER {
            let geometry = crate::debug::path_geometry(engine.world(world).ok()?);
            return (!geometry.lines.is_empty()).then_some(geometry);
        }
        let meshes = (layer == NAVMESH_LAYER)
            .then(|| navmeshes(engine, world, None).ok())
            .flatten()?;
        let geometry = crate::debug::navmesh_geometry(meshes.iter().map(|m| m.as_ref()));
        (!geometry.triangles.is_empty()).then_some(geometry)
    }
}
