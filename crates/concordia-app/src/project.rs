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
use concordia_studio::StudioModule;

use crate::error::AppError;
use crate::manifest::Manifest;

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
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
        })
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
        Ok(modules)
    }
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
