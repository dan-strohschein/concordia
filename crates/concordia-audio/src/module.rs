//! `AudioModule`: the `[audio]` engine module (components, the presentation processor, the
//! `audio.*` methods and the `audio` debug layer).

use concordia_api::{
    ApiError, ApiErrorKind, DebugGeometry, Engine, EngineModule, MethodInfo, PlaySetup, WorldSel,
};
use concordia_assets::AssetDb;
use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_terra::World;

use crate::collect::Collector;
use crate::core::{AudioCore, AudioSettings, AudioShared};
use crate::library::AudioLibrary;

/// The debug layer's name.
pub const AUDIO_LAYER: &str = "audio";

/// Registers the audio types and methods; at play start loads every cue and adds `audio.collect`.
#[derive(Debug, Clone)]
pub struct AudioModule {
    shared: AudioShared,
}

impl AudioModule {
    /// Opens the output `settings` asks for (a failing device falls back to offline).
    pub fn new(settings: AudioSettings) -> Result<Self, crate::AudioError> {
        let shared = AudioShared::new(AudioCore::new(&settings)?);
        if shared.lock().backend_name() == "device" {
            crate::device::spawn_pump(shared.downgrade());
        }
        Ok(Self { shared })
    }

    /// The shared audio state (also `Engine::state::<AudioShared>()`).
    pub fn shared(&self) -> &AudioShared {
        &self.shared
    }
}

/// Stops every voice when the play world is discarded.
struct PlayGuard(AudioShared);

impl Drop for PlayGuard {
    fn drop(&mut self) {
        self.0.lock().stop_all();
    }
}

impl EngineModule for AudioModule {
    fn name(&self) -> &'static str {
        "audio"
    }

    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        crate::components::register_types(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        crate::methods::all()
    }

    fn engine_state(&self) -> Option<Box<dyn std::any::Any + Send>> {
        Some(Box::new(self.shared.clone()))
    }

    fn start_play_in(
        &self,
        engine: &mut Engine,
        _world: &mut World,
    ) -> Result<PlaySetup, ApiError> {
        let (registry, db) = engine.registry_and_state_mut::<AssetDb>();
        let anim_events = registry.get_by_name("AnimEvent").is_some();
        let library = match db {
            Some(db) => {
                let (library, problems) = AudioLibrary::load_all(db);
                for problem in &problems {
                    tracing::warn!(target: "concordia::audio", problem, "cue skipped; it won't sound");
                }
                library
            }
            None => {
                tracing::warn!(target: "concordia::audio", "no asset database ([assets]); no cues load");
                AudioLibrary::default()
            }
        };
        let rate =
            std::num::NonZeroU32::new(engine.sim_rate().hz()).unwrap_or(std::num::NonZeroU32::MIN);
        let mut core = self.shared.lock();
        let silent = core.handle().is_none();
        core.start_play(library)
            .map_err(|e| ApiError::new(ApiErrorKind::Internal, e.to_string()))?;
        drop(core);
        let mut setup = PlaySetup::default();
        if !silent {
            setup
                .schedule
                .add_processor(Collector::new(self.shared.clone(), anim_events, rate));
        }
        setup.state = Some(Box::new(PlayGuard(self.shared.clone())));
        Ok(setup)
    }

    fn debug_layers(&self) -> Vec<&'static str> {
        vec![AUDIO_LAYER]
    }

    fn debug_geometry(
        &self,
        engine: &Engine,
        world: WorldSel,
        layer: &str,
    ) -> Option<DebugGeometry> {
        if layer != AUDIO_LAYER {
            return None;
        }
        let voices = world == WorldSel::play;
        let world = engine.world(world).ok()?;
        let geometry = crate::debug::geometry(world, &self.shared.lock(), voices);
        (!geometry.lines.is_empty()).then_some(geometry)
    }
}
