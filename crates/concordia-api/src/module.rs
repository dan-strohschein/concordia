//! Engine modules: how crates add types, systems, methods and play-time state (ADR-0046).

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use concordia_reflect::{Reflect, RegistryError, TypeRegistry};
use concordia_scene::builtin;
use concordia_terra::{Schedule, World};
use serde::{Deserialize, Serialize};

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

    /// Ready-made entities the module offers to `entity.templates` / `entity.create` (e.g. lights).
    fn templates(&self) -> Vec<crate::Template> {
        Vec::new()
    }

    /// State that lives as long as the engine (e.g. an asset database), reachable through `Engine::state`.
    fn engine_state(&self) -> Option<Box<dyn Any + Send>> {
        None
    }

    /// The code this module runs (e.g. loaded script builds), so a replay can tell whether it
    /// was recorded with the same code (ADR-0072).
    fn fingerprints(&self) -> Vec<Fingerprint> {
        Vec::new()
    }

    /// Called on the engine thread after a playing simulation advanced at least one tick.
    fn after_sim(&self, _engine: &mut crate::Engine) {}

    /// Called once per new play world (which it may prepare, e.g. insert resources): systems to run after earlier modules', and state kept until the play world is discarded.
    fn start_play(&self, _world: &mut World) -> Result<PlaySetup, ApiError> {
        Ok(PlaySetup::default())
    }

    /// Debug layers this module draws (names unique across modules), e.g. `"navmesh"`.
    fn debug_layers(&self) -> Vec<&'static str> {
        Vec::new()
    }

    /// `layer`'s geometry in `world`, or `None` when there is nothing to draw.
    fn debug_geometry(
        &self,
        _engine: &crate::Engine,
        _world: crate::WorldSel,
        _layer: &str,
    ) -> Option<crate::DebugGeometry> {
        None
    }

    /// The in-game UI `world` draws into `frame` (ADR-0060), or `None` when it draws none.
    fn ui_draw(
        &self,
        _engine: &mut crate::Engine,
        _world: crate::WorldSel,
        _frame: &crate::UiFrame,
    ) -> Option<crate::UiDrawList> {
        None
    }

    /// [`EngineModule::start_play`] with the engine in view (its registry, templates, seed and
    /// module states, e.g. to load assets); the engine calls this, and by default it calls `start_play`.
    fn start_play_in(
        &self,
        _engine: &mut crate::Engine,
        world: &mut World,
    ) -> Result<PlaySetup, ApiError> {
        self.start_play(world)
    }
}

/// One piece of code a module runs, identified by a content hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Fingerprint {
    /// The engine module, e.g. `"scripts"`.
    pub module: String,
    /// What it runs, e.g. a script module's name.
    pub name: String,
    /// How it runs, e.g. `"wasm"`, `"native"` or `"linked"`.
    pub kind: String,
    /// BLAKE3 of the code (or of its description), as hex.
    pub hash: String,
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

/// The core module followed by `modules`: every engine's module list.
pub(crate) fn with_core(modules: &[Arc<dyn EngineModule>]) -> Vec<Arc<dyn EngineModule>> {
    let core: Arc<dyn EngineModule> = Arc::new(CoreModule);
    std::iter::once(core)
        .chain(modules.iter().cloned())
        .collect()
}

/// The M0 built-ins: `Name`, `Transform`, `Parent`, `Velocity`, `integrate_velocity` and the core methods.
#[derive(Debug)]
pub(crate) struct CoreModule;

impl EngineModule for CoreModule {
    fn name(&self) -> &'static str {
        "core"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        builtin::register(registry)?;
        concordia_terra_sim::register(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        crate::methods::all()
    }

    fn templates(&self) -> Vec<crate::Template> {
        vec![crate::Template::new(
            "Empty",
            "An entity with only a name and a transform: a group or pivot for other entities.",
            [
                ("Name", serde_json::json!("Empty")),
                (
                    "Transform",
                    serde_json::to_value(builtin::Transform::default()).unwrap_or_default(),
                ),
            ],
        )]
    }

    fn start_play(&self, _: &mut World) -> Result<PlaySetup, ApiError> {
        Ok(PlaySetup {
            schedule: builtin::headless_schedule(),
            state: None,
        })
    }
}
