//! The crowd engine module: types, the `Crowd` trait, the processors at play start and the
//! `crowd` debug layer. `concordia-app` enables it with `[nav]`.

use concordia_api::{ApiError, DebugGeometry, Engine, EngineModule, PlaySetup, WorldSel};
use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_terra::World;

use crate::processors::{add_crowd_processors, insert_crowd_states};

/// The debug layer that draws crowd agents.
pub const CROWD_LAYER: &str = "crowd";

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
        let mut setup = PlaySetup::default();
        add_crowd_processors(&mut setup.schedule);
        Ok(setup)
    }

    fn debug_layers(&self) -> Vec<&'static str> {
        vec![CROWD_LAYER]
    }

    fn debug_geometry(
        &self,
        engine: &Engine,
        world: WorldSel,
        layer: &str,
    ) -> Option<DebugGeometry> {
        if layer != CROWD_LAYER {
            return None;
        }
        let geometry = crate::debug::crowd_geometry(engine.world(world).ok()?);
        (!geometry.lines.is_empty()).then_some(geometry)
    }
}
