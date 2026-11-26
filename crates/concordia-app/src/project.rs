//! A loaded project and the engine modules it uses.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use concordia_anim::AnimModule;
use concordia_api::EngineModule;
use concordia_assets::{AssetsModule, Importer};
use concordia_audio::{AudioModule, AudioSettings, Backend, CpalDevice, read_buses};
use concordia_core::math::Vec3;
use concordia_import_fbx::FbxImporter;
use concordia_import_gltf::GltfImporter;
use concordia_input::{InputMap, InputModule};
use concordia_nav::NavModule;
use concordia_physics::{PhysicsModule, PhysicsSettings};
use concordia_render::{ClosureBudget, RenderModule};
use concordia_replay::ReplayModule;
use concordia_scripting::{ScriptEntry, ScriptKind, ScriptsConfig, ScriptsModule};
use concordia_studio::StudioModule;
use concordia_terra_crowd::CrowdModule;

use crate::error::AppError;
use crate::manifest::{
    AudioBackendChoice, Manifest, RenderSection, ScriptModuleKind, ScriptsSection,
};

/// A project root and its parsed manifest.
#[derive(Debug, Clone)]
pub struct Project {
    /// Project root.
    pub root: PathBuf,
    /// `concordia.toml` (defaults if the file is absent).
    pub manifest: Manifest,
}

/// The model importers every project registers (glTF, FBX), beside the built-in image and material ones.
pub fn model_importers() -> Vec<Arc<dyn Importer>> {
    vec![Arc::new(GltfImporter), Arc::new(FbxImporter)]
}

/// Every importer `[assets]` registers beside the built-in ones: models, audio and UI.
pub fn asset_importers() -> Vec<Arc<dyn Importer>> {
    model_importers()
        .into_iter()
        .chain(concordia_audio::importers())
        .chain(concordia_ui::importers())
        .collect()
}

/// Where the engine runs: decides the audio output for `[audio] backend = "auto"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Playback {
    /// Headless (MCP, CLI, tests, CI, the editor): audio mixes offline.
    #[default]
    Headless,
    /// `concordia run`: audio plays on the default device.
    Windowed,
}

impl Project {
    /// Loads `root/concordia.toml`; a missing file means every default.
    pub fn load(root: &Path) -> Result<Self, AppError> {
        let path = root.join("concordia.toml");
        let manifest = match std::fs::read_to_string(&path) {
            Ok(text) => parse(&text, &path)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Manifest::default(),
            Err(e) => return Err(manifest_error(&path, e.to_string())),
        };
        if let Some(map) = &manifest.input.map {
            inside(map).map_err(|m| manifest_error(&path, format!("[input] map: {m}")))?;
        }
        if let Some(studio) = &manifest.studio {
            inside(&studio.dir).map_err(|m| manifest_error(&path, format!("[studio] dir: {m}")))?;
        }
        if let Some(scripts) = &manifest.scripts {
            check_scripts(scripts).map_err(|m| manifest_error(&path, m))?;
        }
        if let Some(render) = &manifest.render {
            closure_budget(render).map_err(|m| manifest_error(&path, m))?;
        }
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
        })
    }

    /// Engine settings for this project: its modules, `[sim]` rate and seed, `[journal]` sync.
    pub fn engine_config(&self) -> Result<concordia_api::EngineConfig, AppError> {
        self.engine_config_for(Playback::Headless)
    }

    /// [`Project::engine_config`] for `playback` (`concordia run` passes `Windowed`).
    pub fn engine_config_for(
        &self,
        playback: Playback,
    ) -> Result<concordia_api::EngineConfig, AppError> {
        let sim = &self.manifest.sim;
        let mut config = concordia_api::EngineConfig::new(&self.root);
        config.sim_rate = concordia_core::time::TickRate::from_hz(sim.rate_hz).map_err(|e| {
            manifest_error(
                &self.root.join("concordia.toml"),
                format!("[sim] rate_hz: {e}"),
            )
        })?;
        config.sim_seed = sim.seed;
        config.journal.sync = self.manifest.journal.sync;
        config.modules = self.modules_for(playback)?;
        Ok(config)
    }

    /// The input map the manifest names (empty if none).
    pub fn input_map(&self) -> Result<InputMap, AppError> {
        let Some(rel) = &self.manifest.input.map else {
            return Ok(InputMap::default());
        };
        let path = self.root.join(rel);
        let file_error = |message: String| AppError::File {
            path: path.clone(),
            message,
        };
        let text = std::fs::read_to_string(&path).map_err(|e| file_error(e.to_string()))?;
        InputMap::parse(&text).map_err(|e| file_error(e.to_string()))
    }

    /// The main scene camera's EV100: the default reference exposure for imported emission.
    pub fn main_camera_ev100(&self) -> Option<f32> {
        let main = self.manifest.scene.main.as_ref()?;
        let path = self.root.join(main);
        let text = std::fs::read_to_string(&path).ok()?;
        let cameras: Vec<(_, concordia_render::Camera)> = match concordia_scene::read_components(
            &text, "Camera",
        ) {
            Ok(cameras) => cameras,
            Err(e) => {
                tracing::warn!(target: "concordia::app", path = %path.display(), error = %e, "can't read the main scene's cameras");
                return None;
            }
        };
        let chosen = concordia_render_types::pick_camera(
            cameras.iter().map(|(id, c)| (*id, c.priority)),
            None,
        )?;
        cameras
            .iter()
            .find(|(id, _)| *id == chosen)
            .map(|(_, c)| c.exposure_ev100)
    }

    /// The project's closure budget (`[render] closure_budget`; None: the platform's).
    pub fn closure_budget(&self) -> Option<ClosureBudget> {
        self.manifest
            .render
            .as_ref()
            .and_then(|r| closure_budget(r).ok().flatten())
    }

    /// The project's `[render] backend` (`auto` without one).
    pub fn backend_choice(&self) -> concordia_rhi_select::BackendChoice {
        self.manifest
            .render
            .as_ref()
            .map(|r| r.backend)
            .unwrap_or_default()
    }

    /// The GPU backend: `CONCORDIA_BACKEND`, then `[render] backend`, then the platform's.
    pub fn render_backend(&self) -> Result<concordia_rhi::Backend, AppError> {
        let env = std::env::var(concordia_rhi_select::BACKEND_ENV).ok();
        self.render_backend_with(env.as_deref())
    }

    /// [`Project::render_backend`] for a given `CONCORDIA_BACKEND` value.
    pub fn render_backend_with(
        &self,
        env: Option<&str>,
    ) -> Result<concordia_rhi::Backend, AppError> {
        Ok(concordia_rhi_select::choose(env, self.backend_choice())?)
    }

    /// Engine modules after the core module, in order (only those the manifest configures).
    pub fn modules(&self) -> Result<Vec<Arc<dyn EngineModule>>, AppError> {
        self.modules_for(Playback::Headless)
    }

    /// [`Project::modules`] for `playback`.
    pub fn modules_for(&self, playback: Playback) -> Result<Vec<Arc<dyn EngineModule>>, AppError> {
        let mut modules: Vec<Arc<dyn EngineModule>> = Vec::new();
        if self.manifest.input.map.is_some() {
            modules.push(Arc::new(InputModule::new(self.input_map()?)));
        }
        if let Some(physics) = &self.manifest.physics {
            let mut settings = PhysicsSettings::default();
            settings.gravity = Vec3::from_array(physics.gravity);
            settings.max_bodies = physics.max_bodies;
            modules.push(Arc::new(PhysicsModule::new(settings)));
        }
        if let Some(nav) = &self.manifest.nav {
            modules.push(Arc::new(NavModule::with_plans_per_tick(nav.plans_per_tick)));
            modules.push(Arc::new(CrowdModule::new()));
        }
        if self.manifest.assets.is_some() {
            let assets = asset_importers()
                .into_iter()
                .fold(AssetsModule::new(&self.root), AssetsModule::with_importer);
            // Imported emission converts at the main camera's exposure unless a sidecar says
            // otherwise (ADR-0102).
            let assets = match self.main_camera_ev100() {
                Some(ev) => {
                    assets.with_setting_default("emissive_reference_ev100", serde_json::json!(ev))
                }
                None => assets,
            };
            modules.push(Arc::new(assets));
        }
        if self.manifest.anim.is_some() {
            modules.push(Arc::new(AnimModule::new()));
        }
        if self.manifest.audio.is_some() {
            modules.push(Arc::new(self.audio_module(playback)?));
        }
        if self.manifest.ui.is_some() {
            let ui = concordia_ui::UiModule::new();
            // With a renderer, world roots are seen through its camera (they take input).
            let ui = match self.manifest.render {
                Some(_) => ui.with_lens(Arc::new(concordia_render::world_ui_lens)),
                None => ui,
            };
            modules.push(Arc::new(ui));
        }
        if self.manifest.render.is_some() {
            let backend = self.render_backend()?;
            let device = move || concordia_rhi_select::new_headless(backend);
            modules.push(Arc::new(
                RenderModule::new(Arc::new(device))
                    .with_closure_budget(self.closure_budget())
                    .with_perf_mode(self.manifest.render.as_ref().is_some_and(|r| r.perf_mode))
                    .with_meshlets(self.manifest.render.as_ref().is_none_or(|r| r.meshlets))
                    .with_culling(self.manifest.render.as_ref().is_none_or(|r| r.culling))
                    .with_temporal(self.manifest.render.as_ref().map_or_else(
                        Default::default,
                        |r| concordia_render::temporal::Temporal {
                            backend: r.temporal,
                            upscale: r.upscale,
                        },
                    )),
            ));
            // Reference renders with the path tracer (ADR-0090, ADR-0101): GPU by default.
            modules.push(Arc::new(concordia_pathtrace::PathTraceModule::new(Some(
                Arc::new(device),
            ))));
        }
        if let Some(studio) = &self.manifest.studio {
            modules.push(Arc::new(StudioModule::new(&self.root, &studio.dir)));
        }
        if let Some(gen_config) = &self.manifest.generators {
            modules.push(Arc::new(concordia_gen::GenModule::new(gen_config)));
        }
        if self.manifest.replay.is_some() {
            modules.push(Arc::new(
                ReplayModule::new().with_factory(Arc::new(other_engine)),
            ));
        }
        if let Some(scripts) = &self.manifest.scripts {
            // Last, so script components may refer to every other module's types.
            modules.push(Arc::new(ScriptsModule::new(self.scripts_config(scripts))));
        }
        Ok(modules)
    }

    /// `[audio]`'s settings for `playback` (none without the section): the voice pool, the bus
    /// layout file (read and validated) and the backend.
    pub fn audio_settings(&self, playback: Playback) -> Result<Option<AudioSettings>, AppError> {
        let Some(audio) = &self.manifest.audio else {
            return Ok(None);
        };
        let manifest = self.root.join("concordia.toml");
        let voices = usize::from(audio.voices);
        let voices = std::num::NonZeroUsize::new(voices)
            .filter(|v| v.get() <= concordia_audio::MAX_VOICES)
            .ok_or_else(|| {
                manifest_error(
                    &manifest,
                    format!("[audio] voices: {voices} must be 1..=256"),
                )
            })?;
        let buses = match &audio.buses {
            None => concordia_audio::BusLayout::default(),
            Some(rel) => {
                inside(rel)
                    .map_err(|m| manifest_error(&manifest, format!("[audio] buses: {m}")))?;
                let path = self.root.join(rel);
                let file_error = |message: String| AppError::File {
                    path: path.clone(),
                    message,
                };
                let text = std::fs::read_to_string(&path).map_err(|e| file_error(e.to_string()))?;
                read_buses(&text).map_err(|e| file_error(e.to_string()))?
            }
        };
        let backend = match (audio.backend, playback) {
            (AudioBackendChoice::None, _) => Backend::None,
            (AudioBackendChoice::Offline, _) | (AudioBackendChoice::Auto, Playback::Headless) => {
                Backend::Offline
            }
            (AudioBackendChoice::Auto, Playback::Windowed) => {
                Backend::Device(std::sync::Arc::new(CpalDevice))
            }
        };
        Ok(Some(AudioSettings {
            voices,
            buses,
            backend,
        }))
    }

    /// The `audio` module for `playback`.
    fn audio_module(&self, playback: Playback) -> Result<AudioModule, AppError> {
        let manifest = self.root.join("concordia.toml");
        let settings = self.audio_settings(playback)?.unwrap_or_default();
        AudioModule::new(settings).map_err(|e| manifest_error(&manifest, format!("[audio]: {e}")))
    }

    /// `[scripts]` resolved against the project root.
    fn scripts_config(&self, scripts: &ScriptsSection) -> ScriptsConfig {
        let mut config = ScriptsConfig::for_project(&self.root, self.manifest.sim.seed);
        config.modules = scripts
            .module
            .iter()
            .map(|m| ScriptEntry {
                kind: match m.kind {
                    ScriptModuleKind::Wasm => ScriptKind::Wasm,
                    ScriptModuleKind::Native => ScriptKind::Native,
                },
                path: self.root.join(&m.path),
                source: m.source.as_ref().map(|s| self.root.join(s)),
            })
            .collect();
        config
    }
}

/// `[render] closure_budget`, checked: 1..=8, and it can only lower the platform's budget.
fn closure_budget(render: &RenderSection) -> Result<Option<ClosureBudget>, String> {
    render
        .closure_budget
        .map(|n| {
            ClosureBudget::new(n).map_err(|_| {
                format!(
                    "[render] closure_budget: {n} is outside 1..=8; a project can only lower the platform's budget (8 on Metal)"
                )
            })
        })
        .transpose()
}

/// Every `[[scripts.module]]` path stays inside the project.
fn check_scripts(scripts: &ScriptsSection) -> Result<(), String> {
    for (i, module) in scripts.module.iter().enumerate() {
        inside(&module.path).map_err(|m| format!("[[scripts.module]] {i} path: {m}"))?;
        if let Some(source) = &module.source {
            inside(source).map_err(|m| format!("[[scripts.module]] {i} source: {m}"))?;
        }
    }
    Ok(())
}

fn parse(text: &str, path: &Path) -> Result<Manifest, AppError> {
    toml::from_str(text).map_err(|e| manifest_error(path, e.to_string()))
}

fn manifest_error(path: &Path, message: String) -> AppError {
    AppError::Manifest {
        path: path.to_path_buf(),
        message,
    }
}

/// A project-relative path must not be absolute or climb out with `..`.
fn inside(rel: &str) -> Result<(), String> {
    let path = Path::new(rel);
    let escapes = path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)));
    match escapes {
        true => Err(format!(
            "{rel:?} must be a relative path inside the project"
        )),
        false => Ok(()),
    }
}

/// The engine for another project directory (`replay.run`'s `ab` mode): its own manifest's
/// modules and settings, with `threads` job threads.
fn other_engine(
    dir: &Path,
    threads: std::num::NonZeroUsize,
) -> Result<concordia_api::Engine, concordia_api::ApiError> {
    let invalid = |e: AppError| {
        concordia_api::ApiError::new(
            concordia_api::ApiErrorKind::InvalidParams,
            format!("other project {}: {e}", dir.display()),
        )
    };
    let mut config = Project::load(dir)
        .map_err(invalid)?
        .engine_config()
        .map_err(invalid)?;
    config.threads = threads;
    concordia_api::Engine::new(config)
}
