//! The assets engine module: `asset.import` and `asset.list` (ADR-0046).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, EngineModule, Method, MethodInfo};
use concordia_command::Actor;
use concordia_core::id::{AssetId, SystemIdGen};
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::db::{AssetDb, AssetInfo};
use crate::error::AssetError;
use crate::importer::Importer;

/// Adds the project's asset database, asset components and the `asset.*` methods.
#[derive(Clone)]
pub struct AssetsModule {
    root: PathBuf,
    importers: Vec<Arc<dyn Importer>>,
}

impl std::fmt::Debug for AssetsModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.importers.iter().map(|i| i.name()).collect();
        f.debug_struct("AssetsModule")
            .field("root", &self.root)
            .field("importers", &names)
            .finish()
    }
}

impl AssetsModule {
    /// A module for the project at `root` (the image importer is built in).
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            importers: Vec::new(),
        }
    }

    /// Adds an importer (e.g. glTF) to the module's database.
    pub fn with_importer(mut self, importer: Arc<dyn Importer>) -> Self {
        self.importers.push(importer);
        self
    }
}

impl EngineModule for AssetsModule {
    fn name(&self) -> &'static str {
        "assets"
    }

    fn register_types(
        &self,
        registry: &mut concordia_reflect::TypeRegistry,
    ) -> Result<(), concordia_reflect::RegistryError> {
        crate::register_types(registry)
    }

    fn methods(&self) -> Vec<MethodInfo> {
        vec![
            MethodInfo::of::<AssetImport>(),
            MethodInfo::of::<AssetList>(),
            MethodInfo::of::<crate::prefab_method::PrefabInstantiate>(),
            MethodInfo::of::<crate::files::ProjectFiles>(),
        ]
    }

    fn engine_state(&self) -> Option<Box<dyn std::any::Any + Send>> {
        match AssetDb::new(&self.root, Box::new(SystemIdGen)) {
            Ok(mut db) => {
                self.importers
                    .iter()
                    .for_each(|i| db.register(Arc::clone(i)));
                Some(Box::new(db))
            }
            Err(e) => {
                tracing::error!(target: "concordia::assets", error = %e, "asset database unavailable");
                None
            }
        }
    }
}

pub(crate) fn db(engine: &mut Engine) -> Result<&mut AssetDb, ApiError> {
    engine.state_mut::<AssetDb>().ok_or_else(|| {
        ApiError::new(
            ApiErrorKind::Internal,
            "the asset database failed to load; see log.tail",
        )
    })
}

pub(crate) fn api_error(e: AssetError) -> ApiError {
    let kind = match e {
        AssetError::Path(_)
        | AssetError::NoImporter { .. }
        | AssetError::Io(_)
        | AssetError::Reference { .. }
        | AssetError::NotFound(_) => ApiErrorKind::InvalidParams,
        _ => ApiErrorKind::Internal,
    };
    ApiError::new(kind, e.to_string())
}

/// Import (or re-import) a source file under the project: writes its `.import.ron` sidecar and fills the derived-data cache. Idempotent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AssetImport;

/// `asset.import` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct AssetImportParams {
    /// Project-relative source path, e.g. "textures/crate.png".
    pub path: String,
    /// Who is importing (attribution in the log).
    pub actor: Option<Actor>,
}

/// `asset.import` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AssetImportResult {
    /// Asset id (refer to it as "asset:<id>").
    pub id: AssetId,
    /// True if nothing changed since the last import.
    pub cached: bool,
    /// Sub-asset paths ("asset:<id>#<path>").
    pub sub_assets: Vec<String>,
}

impl Method for AssetImport {
    const NAME: &'static str = "asset.import";
    const MUTATES: bool = false;
    type Params = AssetImportParams;
    type Result = AssetImportResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        p: AssetImportParams,
    ) -> Result<AssetImportResult, ApiError> {
        let report = db(engine)?.import(Path::new(&p.path)).map_err(api_error)?;
        let actor = ctx
            .actor
            .as_ref()
            .map_or_else(|| "none".into(), Actor::to_string);
        tracing::info!(target: "concordia::assets", actor, path = %p.path, id = %report.id, cached = report.cached, "asset imported");
        Ok(AssetImportResult {
            id: report.id,
            cached: report.cached,
            sub_assets: report.sub_assets.into_keys().collect(),
        })
    }
}

/// List every imported asset with its id, path, importer and sub-assets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AssetList;

/// `asset.list` params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct AssetListParams {}

/// `asset.list` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AssetListResult {
    /// Assets by id.
    pub assets: Vec<AssetInfo>,
}

impl Method for AssetList {
    const NAME: &'static str = "asset.list";
    const MUTATES: bool = false;
    type Params = AssetListParams;
    type Result = AssetListResult;

    fn call(
        engine: &mut Engine,
        _: &CallCtx,
        _: AssetListParams,
    ) -> Result<AssetListResult, ApiError> {
        Ok(AssetListResult {
            assets: db(engine)?.list(),
        })
    }
}
