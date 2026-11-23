//! `UiModule`: the `[ui]` engine module (the components, `ui.inspect`, the `ui_bounds` debug
//! layer and the draw hook the renderer's UI pass reads).

use std::sync::{Arc, Mutex, MutexGuard};

use concordia_api::{
    ApiError, Engine, EngineModule, MethodInfo, PlaySetup, UiDrawList, UiFrame, WorldSel,
};
use concordia_assets::AssetDb;
use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_terra::World;

use crate::assets::{AssetCache, DbAssets};
use crate::context::UiContext;
use crate::scene::UiScene;

pub use concordia_api::UI_BOUNDS_LAYER;

/// The UI's presentation state: fonts, the glyph atlas and loaded assets.
#[derive(Debug, Default)]
pub struct UiCore {
    /// Fonts and the atlas.
    pub context: UiContext,
    /// Loaded images, themes and fonts.
    pub cache: AssetCache,
}

/// The shared UI state (also `Engine::state::<UiShared>()`).
#[derive(Debug, Clone, Default)]
pub struct UiShared(Arc<Mutex<UiCore>>);

impl UiShared {
    /// Locks the state (a poisoned lock is recovered: the state is a cache).
    pub fn lock(&self) -> MutexGuard<'_, UiCore> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Registers the UI types and `ui.inspect`; draws each world's UI for the renderer.
#[derive(Debug, Clone, Default)]
pub struct UiModule {
    shared: UiShared,
}

impl UiModule {
    /// A module with the built-in fonts loaded.
    pub fn new() -> Self {
        Self::default()
    }

    /// The shared state.
    pub fn shared(&self) -> &UiShared {
        &self.shared
    }
}

/// `world`'s UI scene and the asset database, borrowed together.
pub(crate) fn scene_and_db(
    engine: &mut Engine,
    world: WorldSel,
) -> Option<(UiScene, Option<&mut AssetDb>)> {
    let scene = UiScene::gather(engine.world(world).ok()?);
    Some((scene, engine.state_mut::<AssetDb>()))
}

impl EngineModule for UiModule {
    fn name(&self) -> &'static str {
        "ui"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        crate::components::register_types(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        crate::methods::all()
    }

    /// Snapshots the assets the play world's UI names, and adds `ui.interact`.
    fn start_play_in(&self, engine: &mut Engine, world: &mut World) -> Result<PlaySetup, ApiError> {
        let mut cache = AssetCache::default();
        let mut assets = DbAssets {
            db: engine.state_mut::<AssetDb>(),
            cache: &mut cache,
        };
        crate::play::preload(world, &mut assets);
        let mut setup = PlaySetup::default();
        crate::play::add_interact(&mut setup.schedule, cache);
        Ok(setup)
    }

    fn engine_state(&self) -> Option<Box<dyn std::any::Any + Send>> {
        Some(Box::new(self.shared.clone()))
    }

    fn debug_layers(&self) -> Vec<&'static str> {
        vec![UI_BOUNDS_LAYER]
    }

    fn ui_draw(&self, engine: &mut Engine, world: WorldSel, frame: &UiFrame) -> Option<UiDrawList> {
        let (scene, db) = scene_and_db(engine, world)?;
        if scene.is_empty() {
            return None;
        }
        let mut core = self.shared.lock();
        let UiCore { context, cache } = &mut *core;
        let mut assets = DbAssets { db, cache };
        let list = context.draw_scene(&scene, &mut assets, frame);
        (!list.is_empty()).then_some(list)
    }
}
