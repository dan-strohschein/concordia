//! `AnimModule`: the `[anim]` engine module (components, and the play processors).

use std::sync::Arc;

use concordia_api::{ApiError, ApiErrorKind, Engine, EngineModule, MethodInfo, PlaySetup};
use concordia_assets::AssetDb;
use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_terra::World;

use crate::library::AnimLibrary;
use crate::processors::{add_processors, insert_rest_poses};

/// Registers the animation types and `anim.inspect`; at play start loads the library and adds the
/// tween system and the processors.
#[derive(Debug, Default)]
pub struct AnimModule;

impl AnimModule {
    /// The module.
    pub fn new() -> Self {
        Self
    }
}

impl EngineModule for AnimModule {
    fn name(&self) -> &'static str {
        "anim"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        crate::register_types(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![MethodInfo::of::<crate::methods::AnimInspect>()]
    }

    fn start_play_in(&self, engine: &mut Engine, world: &mut World) -> Result<PlaySetup, ApiError> {
        let (registry, db) = engine.registry_and_state_mut::<AssetDb>();
        let Some(db) = db else {
            tracing::warn!(target: "concordia::anim", "no asset database ([assets]); nothing animates");
            return Ok(PlaySetup::default());
        };
        let (library, report) = AnimLibrary::load(db, registry, world)
            .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))?;
        for problem in &report.problems {
            tracing::warn!(target: "concordia::anim", problem, "animation asset skipped; its players hold their pose");
        }
        insert_rest_poses(&library, world);
        let library = Arc::new(library);
        // The renderer finds skeletons here; resources other than the clock and RNG aren't hashed.
        world.insert_resource(library.clone());
        let mut setup = PlaySetup::default();
        // Tweens first, so a tweened graph parameter is seen by `anim.sample` the same tick.
        crate::tween::add_tweens(&mut setup.schedule, registry);
        add_processors(&mut setup.schedule, library);
        Ok(setup)
    }
}
