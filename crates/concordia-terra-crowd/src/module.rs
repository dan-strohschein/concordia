//! The crowd engine module: crowds and traffic (types, the `Crowd` and `Traffic` traits, the
//! processors at play start) and the `crowd` and `lanes` debug layers. `concordia-app` enables it
//! with `[nav]`.

use concordia_api::{ApiError, DebugGeometry, Engine, EngineModule, PlaySetup, WorldSel};
use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_terra::World;

use crate::processors::{add_crowd_processors, insert_crowd_states};
use crate::traffic::steps::{add_traffic_processors, insert_traffic};

/// The debug layer that draws crowd agents.
pub const CROWD_LAYER: &str = "crowd";
/// The debug layer that draws lanes, junctions and vehicles.
pub const LANES_LAYER: &str = "lanes";

/// Crowds over `[nav]`'s navmeshes.
#[derive(Debug, Clone, Default)]
pub struct CrowdModule;

impl CrowdModule {
    /// The module.
    pub fn new() -> Self {
        Self
    }
}

impl EngineModule for CrowdModule {
    fn name(&self) -> &'static str {
        "crowd"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        crate::register_types(registry)
    }

    fn start_play(&self, world: &mut World) -> Result<PlaySetup, ApiError> {
        insert_crowd_states(world);
        insert_traffic(world);
        let mut setup = PlaySetup::default();
        add_crowd_processors(&mut setup.schedule);
        add_traffic_processors(&mut setup.schedule);
        Ok(setup)
    }

    fn debug_layers(&self) -> Vec<&'static str> {
        vec![CROWD_LAYER, LANES_LAYER]
    }

    fn debug_geometry(
        &self,
        engine: &Engine,
        world: WorldSel,
        layer: &str,
    ) -> Option<DebugGeometry> {
        let world = engine.world(world).ok()?;
        let geometry = match layer {
            CROWD_LAYER => crate::debug::crowd_geometry(world),
            LANES_LAYER => crate::traffic::debug::lanes_geometry(world),
            _ => return None,
        };
        (!geometry.lines.is_empty()).then_some(geometry)
    }
}
