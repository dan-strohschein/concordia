//! Engine modules: how crates add types, systems, methods and play-time state (ADR-0046).

use std::any::Any;
use std::fmt;

use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_scene::builtin;
use concordia_terra::{Schedule, World};

use crate::error::ApiError;
use crate::method::MethodInfo;

/// A unit of engine functionality (input, physics, render, ...); every hook has a no-op default.
pub trait EngineModule: Send + Sync + fmt::Debug {
    /// Unique module name, e.g. `"physics"`.
    fn name(&self) -> &'static str;

    /// Registers the module's reflected types and components.
    fn register_types(&self, _registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        Ok(())
    }

    /// Engine API methods the module adds (names must be unique across modules).
    fn methods(&self) -> Vec<MethodInfo> {
        Vec::new()
    }

    /// State that lives as long as the engine (e.g. an asset database), reachable through `Engine::state`.
    fn engine_state(&self) -> Option<Box<dyn Any + Send>> {
        None
    }

    /// Called on the engine thread after a playing simulation advanced at least one tick.
    fn after_sim(&self, _engine: &mut crate::Engine) {}

    /// Called once per new play world (which it may prepare, e.g. insert resources): systems to run after earlier modules', and state kept until the play world is discarded.
    fn start_play(&self, _world: &mut World) -> Result<PlaySetup, ApiError> {
        Ok(PlaySetup::default())
    }
}

/// What a module contributes to one play session.
#[derive(Default)]
pub struct PlaySetup {
    /// Systems appended to the play schedule.
    pub schedule: Schedule,
    /// Runtime state (e.g. a physics system), reachable through [`crate::Engine::play_state`].
    pub state: Option<Box<dyn Any + Send>>,
}

impl fmt::Debug for PlaySetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlaySetup")
            .field("systems", &self.schedule.names())
            .field("state", &self.state.is_some())
            .finish()
    }
}

/// The M0 built-ins: `Name`, `Transform`, `Parent`, `Velocity`, `integrate_velocity` and the core methods.
#[derive(Debug)]
pub(crate) struct CoreModule;

impl EngineModule for CoreModule {
    fn name(&self) -> &'static str {
        "core"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        builtin::register(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        crate::methods::all()
    }

    fn start_play(&self, _: &mut World) -> Result<PlaySetup, ApiError> {
        Ok(PlaySetup {
            schedule: builtin::headless_schedule(),
            state: None,
        })
    }
}
