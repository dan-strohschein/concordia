//! A loaded project and the engine modules it uses.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use concordia_api::EngineModule;
use concordia_assets::AssetsModule;
use concordia_core::math::Vec3;
use concordia_import_gltf::GltfImporter;
use concordia_input::{InputMap, InputModule};
use concordia_physics::{PhysicsModule, PhysicsSettings};
use concordia_render::RenderModule;
use concordia_replay::ReplayModule;
use concordia_scripting::{ScriptEntry, ScriptKind, ScriptsConfig, ScriptsModule};
use concordia_studio::StudioModule;

use crate::error::AppError;
use crate::manifest::{Manifest, ScriptModuleKind, ScriptsSection};

/// A project root and its parsed manifest.
#[derive(Debug, Clone)]
pub struct Project {
    /// Project root.
    pub root: PathBuf,
    /// `concordia.toml` (defaults if the file is absent).
    pub manifest: Manifest,
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
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
        })
    }

    /// Engine settings for this project: its modules, `[sim]` rate and seed, `[journal]` sync.
    pub fn engine_config(&self) -> Result<concordia_api::EngineConfig, AppError> {
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
        config.modules = self.modules()?;
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

    /// Engine modules after the core module, in order (only those the manifest configures).
    pub fn modules(&self) -> Result<Vec<Arc<dyn EngineModule>>, AppError> {
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
        if self.manifest.assets.is_some() {
            modules.push(Arc::new(
                AssetsModule::new(&self.root).with_importer(Arc::new(GltfImporter)),
            ));
        }
        if self.manifest.render.is_some() {
            modules.push(Arc::new(RenderModule::new(Arc::new(
                concordia_rhi_metal::new_headless,
            ))));
        }
        if let Some(studio) = &self.manifest.studio {
            modules.push(Arc::new(StudioModule::new(&self.root, &studio.dir)));
        }
        if self.manifest.replay.is_some() {
            modules.push(Arc::new(ReplayModule::new()));
        }
        if let Some(scripts) = &self.manifest.scripts {
            // Last, so script components may refer to every other module's types.
            modules.push(Arc::new(ScriptsModule::new(self.scripts_config(scripts))));
        }
        Ok(modules)
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
