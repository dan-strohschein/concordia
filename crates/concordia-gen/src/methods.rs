//! `gen.list` and `gen.run`.

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method, MethodInfo};
use concordia_command::Actor;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::provider::GenKind;

/// List the content providers (procedural generators and any configured plugins) with what they make, their file extensions and the JSON Schema of their requests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GenList;

/// `gen.list` params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GenListParams {}

/// One provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ProviderInfo {
    /// Its id (gen.run's provider).
    pub id: String,
    /// What it makes.
    pub kind: GenKind,
    /// Its version.
    pub version: u32,
    /// File extensions it writes.
    pub extensions: Vec<String>,
    /// Runs in-process from code (not a plugin or external tool).
    pub procedural: bool,
    /// JSON Schema of its request.
    pub request: Value,
}

/// `gen.list` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GenListResult {
    /// The providers.
    pub providers: Vec<ProviderInfo>,
}

/// Generate a file with a provider (e.g. {"provider": "mesh", "request": {"Box": {"size": [1, 2, 1]}}, "seed": 1, "path": "assets/models/crate.glb"}), import it, and record its provenance in the sidecar so it can be made again. Writes a file, so it needs an actor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GenRun;

/// `gen.run` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct GenRunParams {
    /// The provider (gen.list).
    pub provider: String,
    /// Its request (gen.list shows the schema).
    pub request: Value,
    /// Seed (default 0).
    pub seed: Option<u64>,
    /// Project-relative output path with one of the provider's extensions.
    pub path: String,
    /// Who is generating (required; logged).
    pub actor: Option<Actor>,
}

/// `gen.run` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct GenRunResult {
    /// The file written.
    pub path: String,
    /// Its asset id (refer to it as "asset:<id>").
    pub id: String,
    /// Sub-asset paths ("asset:<id>#<path>").
    pub sub_assets: Vec<String>,
    /// BLAKE3 of provider, version, request and seed (also in the sidecar's provenance).
    pub inputs_hash: String,
    /// What the provider reports, e.g. triangles and bounds.
    pub info: Value,
}

impl Method for GenList {
    const NAME: &'static str = "gen.list";
    const MUTATES: bool = false;
    type Params = GenListParams;
    type Result = GenListResult;

    fn call(engine: &mut Engine, _: &CallCtx, _: GenListParams) -> Result<GenListResult, ApiError> {
        let providers = registry(engine)?
            .iter()
            .map(|p| ProviderInfo {
                id: p.id().to_owned(),
                kind: p.kind(),
                version: p.version(),
                extensions: p.extensions().iter().map(|e| (*e).to_owned()).collect(),
                procedural: p.procedural(),
                request: p.request_schema(),
            })
            .collect();
        Ok(GenListResult { providers })
    }
}

impl Method for GenRun {
    const NAME: &'static str = "gen.run";
    const MUTATES: bool = false;
    type Params = GenRunParams;
    type Result = GenRunResult;

    fn call(engine: &mut Engine, ctx: &CallCtx, p: GenRunParams) -> Result<GenRunResult, ApiError> {
        let actor = p
            .actor
            .clone()
            .or_else(|| ctx.actor.clone())
            .ok_or_else(|| {
                ApiError::new(
                    ApiErrorKind::InvalidParams,
                    "gen.run writes a file, so it needs an actor",
                )
                .at("/actor")
            })?;
        let registry = registry(engine)?;
        let provider = registry.get(&p.provider).ok_or_else(|| {
            let ids: Vec<&str> = registry.iter().map(|p| p.id()).collect();
            ApiError::new(
                ApiErrorKind::InvalidParams,
                format!(
                    "no provider {:?}; gen.list has {}",
                    p.provider,
                    ids.join(", ")
                ),
            )
            .at("/provider")
        })?;
        let extensions = provider.extensions();
        let extension = extensions
            .iter()
            .find(|e| p.path.ends_with(&format!(".{e}")))
            .ok_or_else(|| {
                ApiError::new(
                    ApiErrorKind::InvalidParams,
                    format!("{} writes .{} files", provider.id(), extensions.join(", .")),
                )
                .at("/path")
            })?;
        let file = engine.project_file(&p.path, &format!(".{extension}"))?;
        let seed = p.seed.unwrap_or(0);
        let ctx_gen = crate::provider::GenContext {
            project: engine.project(),
        };
        let generated = provider.generate(&p.request, seed, &ctx_gen).map_err(|e| {
            ApiError::new(
                ApiErrorKind::InvalidParams,
                format!("{}: {e}", provider.id()),
            )
            .at("/request")
        })?;
        write(&file, &generated.bytes)?;
        let import = |engine: &mut Engine| {
            concordia_assets::AssetImport::call(
                engine,
                ctx,
                concordia_assets::AssetImportParams {
                    path: p.path.clone(),
                    actor: Some(actor.clone()),
                },
            )
        };
        let mut imported = import(engine)?;
        if let Some(settings) = &generated.settings {
            merge_settings(&file, settings)?;
            imported = import(engine)?;
        }
        let hash = crate::provider::inputs_hash(
            provider.id(),
            provider.version(),
            &p.request,
            seed,
            &generated.inputs,
        );
        record_provenance(&file, provider.as_ref(), &p.request, seed, &hash)?;
        tracing::info!(target: "concordia::gen", actor = %actor, provider = provider.id(), path = %p.path, "content generated");
        Ok(GenRunResult {
            path: p.path,
            id: imported.id.to_string(),
            sub_assets: imported.sub_assets,
            inputs_hash: hash,
            info: generated.info,
        })
    }
}

/// The engine's providers (`[gen]`).
fn registry(engine: &Engine) -> Result<crate::registry::ProviderRegistry, ApiError> {
    engine
        .state::<crate::registry::ProviderRegistry>()
        .cloned()
        .ok_or_else(|| ApiError::new(ApiErrorKind::Internal, "the gen module's state is missing"))
}

/// Writes `bytes` to `file` (its directory first), replacing it atomically.
fn write(file: &std::path::Path, bytes: &[u8]) -> Result<(), ApiError> {
    let io = |e: std::io::Error| {
        ApiError::new(ApiErrorKind::SceneIo, format!("{}: {e}", file.display()))
    };
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = file.with_extension("gen-tmp");
    std::fs::write(&tmp, bytes).map_err(io)?;
    std::fs::rename(&tmp, file).map_err(io)
}

/// Merges a provider's import settings (e.g. loop points) into `file`'s sidecar.
fn merge_settings(file: &std::path::Path, settings: &Value) -> Result<(), ApiError> {
    use concordia_assets::Sidecar;
    let path = Sidecar::path_for(file);
    let fail =
        |e: concordia_assets::AssetError| ApiError::new(ApiErrorKind::SceneIo, e.to_string());
    let mut sidecar = Sidecar::read(&path).map_err(fail)?;
    if let (Value::Object(into), Value::Object(from)) = (&mut sidecar.settings, settings) {
        into.extend(from.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    sidecar.write(&path).map_err(fail)
}

/// Records how `file` was made in its sidecar (which the import just wrote).
fn record_provenance(
    file: &std::path::Path,
    provider: &dyn crate::provider::ContentProvider,
    request: &Value,
    seed: u64,
    hash: &str,
) -> Result<(), ApiError> {
    use concordia_assets::{Provenance, Sidecar};
    let path = Sidecar::path_for(file);
    let fail =
        |e: concordia_assets::AssetError| ApiError::new(ApiErrorKind::SceneIo, e.to_string());
    let mut sidecar = Sidecar::read(&path).map_err(fail)?;
    sidecar.provenance = Some(Provenance {
        provider: provider.id().to_owned(),
        provider_version: provider.version(),
        request: request.clone(),
        seed,
        inputs_hash: hash.to_owned(),
    });
    sidecar.write(&path).map_err(fail)
}

/// The module's methods.
pub(crate) fn all() -> Vec<MethodInfo> {
    vec![MethodInfo::of::<GenList>(), MethodInfo::of::<GenRun>()]
}
