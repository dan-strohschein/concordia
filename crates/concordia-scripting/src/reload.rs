//! `script.reload`: load what's on disk again, migrating changed types in both worlds.

use concordia_api::{ApiError, ApiErrorKind, CallCtx, Engine, Method, WorldSel};
use concordia_command::{Actor, EditOp};
use concordia_core::id::TxnId;
use concordia_reflect::{Reflect, TypeDesc};
use concordia_script::{HostConfig, ModuleDesc, ScriptHost, TypeMigration, changed_types};
use concordia_terra::World;
use serde::{Deserialize, Serialize};

use crate::config::ScriptKind;
use crate::state::{Loaded, Scripts, ScriptsState, Source, graph_module_name};

/// Reload the project's gameplay modules from disk (WASM components and native libraries;
/// this never builds). Changed component types are migrated: in the edit world as one undoable
/// transaction by `script:<module>`, and in the play world, all or nothing. Processors keep
/// running with the new code from the next tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ScriptReload;

/// `script.reload` params.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ScriptReloadParams {
    /// Reload only this module (by its declared name); all of them when absent.
    pub module: Option<String>,
    /// Who is asking.
    pub actor: Option<Actor>,
}

/// One migrated type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct MigratedType {
    /// Component or signal type.
    pub name: String,
    /// Play-world values converted.
    pub values: u64,
    /// Fields added (JSON Pointer paths; stored values took their defaults).
    pub added: Vec<String>,
    /// Fields removed (JSON Pointer paths; their values are gone).
    pub removed: Vec<String>,
}

/// One reloaded module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ReloadedModule {
    /// The module's declared name.
    pub name: String,
    /// `wasm`, `native` or `linked`.
    pub backend: String,
    /// The new build's declared version.
    pub version: u32,
    /// Types whose layout changed.
    pub migrated: Vec<MigratedType>,
    /// The edit-world migration transaction, if the edit world held a changed type.
    pub edit_txn: Option<String>,
}

/// `script.reload` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ScriptReloadResult {
    /// Reloaded modules, in load order.
    pub modules: Vec<ReloadedModule>,
}

impl Method for ScriptReload {
    const NAME: &'static str = "script.reload";
    const MUTATES: bool = true;
    type Params = ScriptReloadParams;
    type Result = ScriptReloadResult;

    fn call(
        engine: &mut Engine,
        ctx: &CallCtx,
        params: ScriptReloadParams,
    ) -> Result<ScriptReloadResult, ApiError> {
        let actor = ctx.actor()?.to_string();
        let handle = engine
            .state::<ScriptsState>()
            .map(|s| s.0.clone())
            .ok_or_else(|| failed("the scripts module is not running".into()))?;
        let mut scripts = handle.lock();
        let mut modules = Vec::new();
        match &params.module {
            Some(name) => {
                let index = position(&scripts, name)?;
                modules.push(reload_one(engine, &mut scripts, index, &actor)?);
            }
            None => {
                modules.extend(retry_failed(engine, &mut scripts)?);
                for index in 0..scripts.loaded.len() {
                    modules.push(reload_one(engine, &mut scripts, index, &actor)?);
                }
            }
        }
        Ok(ScriptReloadResult { modules })
    }
}

fn failed(message: String) -> ApiError {
    ApiError::new(ApiErrorKind::ScriptFailed, message)
}

/// The loaded module named `name`.
fn position(scripts: &Scripts, name: &str) -> Result<usize, ApiError> {
    scripts
        .loaded
        .iter()
        .position(|l| l.host.desc().name == name)
        .ok_or_else(|| {
            let known: Vec<String> = scripts.loaded.iter().map(|l| l.host.desc().name).collect();
            failed(format!(
                "no loaded module is named {name:?}; loaded: {known:?}"
            ))
        })
}

/// Loads modules that failed earlier; they join play from the next session.
fn retry_failed(
    engine: &mut Engine,
    scripts: &mut Scripts,
) -> Result<Vec<ReloadedModule>, ApiError> {
    let mut joined = Vec::new();
    for (source, _) in std::mem::take(&mut scripts.failed) {
        joined.push(join(engine, scripts, source)?);
    }
    Ok(joined)
}

/// Loads one module that failed earlier (it stays failed if it fails again).
fn join(
    engine: &mut Engine,
    scripts: &mut Scripts,
    source: Source,
) -> Result<ReloadedModule, ApiError> {
    let config = HostConfig {
        session_seed: scripts.config.session_seed,
    };
    let (registry, _) = engine.registry_and_play_mut();
    let host = scripts.load(&source, registry).and_then(|build| {
        let host = ScriptHost::load(build.module, registry, config).map_err(|e| e.to_string())?;
        Ok((host, build.hash, build.graph))
    });
    let (host, hash, graph) = match host {
        Ok(loaded) => loaded,
        Err(reason) => {
            scripts.failed.push((source, reason.clone()));
            return Err(failed(reason));
        }
    };
    let desc = host.desc();
    scripts.adopt(desc.name.clone(), graph);
    let joined = ReloadedModule {
        name: desc.name,
        backend: source.backend().to_owned(),
        version: desc.version,
        migrated: Vec::new(),
        edit_txn: None,
    };
    scripts.loaded.push(Loaded { source, host, hash });
    Ok(joined)
}

/// Reloads (or, if it failed to load before, loads) the graph module `name`; false if no
/// configured module has that name.
pub(crate) fn reload_graph(
    engine: &mut Engine,
    scripts: &mut Scripts,
    name: &str,
    actor: &str,
) -> Result<bool, ApiError> {
    if let Ok(index) = position(scripts, name) {
        reload_one(engine, scripts, index, actor)?;
        return Ok(true);
    }
    let failed_graph = scripts.failed.iter().position(|(source, _)| match source {
        Source::File(entry) => {
            entry.kind == ScriptKind::Graph && graph_module_name(&entry.file()) == name
        }
        Source::Linked(_) => false,
    });
    match failed_graph {
        Some(k) => {
            let (source, _) = scripts.failed.remove(k);
            join(engine, scripts, source)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Reloads loaded module `index`: edit world first (as `script:<module>`), then the registry
/// and play world; if the second step fails, the first is undone.
fn reload_one(
    engine: &mut Engine,
    scripts: &mut Scripts,
    index: usize,
    actor: &str,
) -> Result<ReloadedModule, ApiError> {
    let source = scripts.loaded[index].source.clone();
    let host = scripts.loaded[index].host.clone();
    let crate::state::Build {
        module: build,
        hash,
        graph,
    } = scripts.load(&source, engine.registry()).map_err(failed)?;
    let desc = build
        .describe()
        .map_err(|e| failed(format!("describe: {e}")))?;
    let old = host.desc();
    let module_actor: Actor = format!("script:{}", old.name).parse().map_err(|_| {
        failed(format!(
            "{:?} is not a valid module name for an actor",
            old.name
        ))
    })?;
    let txn = migrate_edit_world(engine, &module_actor, &old, &desc)?;
    let (registry, play) = engine.registry_and_play_mut();
    let mut scratch = World::new();
    let reloaded = host.reload(build, registry, play.unwrap_or(&mut scratch));
    let report = match reloaded {
        Ok(report) => report,
        Err(e) => {
            if let Some(txn) = txn {
                engine.undo_as(module_actor, txn)?;
            }
            return Err(failed(e.to_string()));
        }
    };
    scripts.loaded[index].hash = hash;
    scripts.adopt(old.name.clone(), graph);
    let migrated: Vec<MigratedType> = report.migrated.into_iter().map(migrated_type).collect();
    tracing::info!(
        target: "concordia::script",
        actor,
        module = %old.name,
        version = desc.version,
        migrated = migrated.len(),
        "script reloaded"
    );
    Ok(ReloadedModule {
        name: old.name,
        backend: source.backend().to_owned(),
        version: desc.version,
        migrated,
        edit_txn: txn.map(|t| t.to_string()),
    })
}

/// One transaction migrating the changed types the edit world has stored.
fn migrate_edit_world(
    engine: &mut Engine,
    actor: &Actor,
    old: &ModuleDesc,
    new: &ModuleDesc,
) -> Result<Option<TxnId>, ApiError> {
    let Ok(world) = engine.world(WorldSel::edit) else {
        return Ok(None);
    };
    let changed: Vec<&TypeDesc> = changed_types(old, new)
        .into_iter()
        .filter(|t| world.stores(&t.name))
        .collect();
    let names: Vec<&str> = changed.iter().map(|t| t.name.as_str()).collect();
    let ops = changed
        .iter()
        .map(|t| {
            let to = serde_json::to_value(t).map_err(|e| failed(e.to_string()))?;
            Ok(EditOp::MigrateType {
                component: t.name.clone(),
                to,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let label = format!("script reload: migrate {}", names.join(", "));
    engine.commit_edits_as(actor.clone(), &label, ops)
}

fn migrated_type(m: TypeMigration) -> MigratedType {
    MigratedType {
        name: m.name,
        values: m.values as u64,
        added: m.added,
        removed: m.removed,
    }
}
