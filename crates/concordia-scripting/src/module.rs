//! The `scripts` engine module.

use std::any::Any;

use std::sync::Arc;

use concordia_api::{
    ApiError, ApiErrorKind, Engine, EngineModule, Fingerprint, MethodInfo, PlaySetup,
};
use concordia_reflect::{RegistryError, TypeRegistry};
use concordia_script::{HostConfig, ScriptHost, Templates, TestBackend};
use concordia_script_guest::Module;
use concordia_terra::{Phase, SystemCtx, World, system};

use crate::config::ScriptsConfig;
use crate::state::{Handle, Loaded, ScriptsState};

/// The system that logs what scripts reported during a tick. It is exclusive, so it runs on the
/// engine thread after every script processor, and emits in a fixed order.
const EMIT_LOGS: &str = "scripts.emit_logs";

/// Loads a project's gameplay modules, runs their processors in play and serves
/// `script.reload`.
pub struct ScriptsModule {
    handle: Handle,
}

impl std::fmt::Debug for ScriptsModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptsModule")
            .field("modules", &self.handle.lock().sources())
            .finish()
    }
}

impl ScriptsModule {
    /// A module loading `config`'s files.
    pub fn new(config: ScriptsConfig) -> Self {
        Self {
            handle: Handle::new(config, TestBackend::new()),
        }
    }

    /// Also runs module `M`, linked into the engine (the static path).
    #[must_use]
    pub fn link<M: Module>(self) -> Self {
        {
            let mut scripts = self.handle.lock();
            scripts.linked = std::mem::take(&mut scripts.linked).link::<M>();
            scripts.linked_names.push(M::describe().name);
        }
        self
    }
}

impl EngineModule for ScriptsModule {
    fn name(&self) -> &'static str {
        "scripts"
    }

    /// Loads every module and registers its types; a module that fails is logged and skipped
    /// (`script.reload` reports it), so a broken script never stops the editor from opening.
    fn register_types(&self, registry: &mut TypeRegistry) -> Result<(), RegistryError> {
        let mut scripts = self.handle.lock();
        let config = HostConfig {
            session_seed: scripts.config.session_seed,
        };
        for source in scripts.sources() {
            let loaded = scripts.load(&source, registry).and_then(|build| {
                let host =
                    ScriptHost::load(build.module, registry, config).map_err(|e| e.to_string())?;
                Ok((host, build.hash, build.graph))
            });
            match loaded {
                Ok((host, hash, graph)) => {
                    scripts.adopt(host.desc().name, graph);
                    scripts.loaded.push(Loaded { source, host, hash });
                }
                Err(reason) => {
                    tracing::error!(target: "concordia::script", %reason, "script module failed to load");
                    scripts.failed.push((source, reason));
                }
            }
        }
        Ok(())
    }

    fn methods(&self) -> Vec<MethodInfo> {
        use crate::graph_api::{
            GraphApply, GraphCheck, GraphCompile, GraphListNodes, GraphRead, GraphTrace,
        };
        vec![
            MethodInfo::of::<crate::reload::ScriptReload>(),
            MethodInfo::of::<GraphListNodes>(),
            MethodInfo::of::<GraphRead>(),
            MethodInfo::of::<GraphCheck>(),
            MethodInfo::of::<GraphCompile>(),
            MethodInfo::of::<GraphApply>(),
            MethodInfo::of::<GraphTrace>(),
        ]
    }

    /// Each loaded module's build, by content hash (ADR-0072).
    fn fingerprints(&self) -> Vec<Fingerprint> {
        self.handle
            .lock()
            .loaded
            .iter()
            .map(|loaded| Fingerprint {
                module: "scripts".into(),
                name: loaded.host.desc().name,
                kind: loaded.source.backend().into(),
                hash: loaded.hash.clone(),
            })
            .collect()
    }

    fn engine_state(&self) -> Option<Box<dyn Any + Send>> {
        Some(Box::new(ScriptsState(self.handle.clone())))
    }

    /// Every module's processors, in load order; faulted processors get a fresh start.
    fn start_play_in(
        &self,
        engine: &mut Engine,
        _world: &mut World,
    ) -> Result<PlaySetup, ApiError> {
        let scripts = self.handle.lock();
        let mut setup = PlaySetup::default();
        if scripts.loaded.is_empty() {
            return Ok(setup);
        }
        let templates = compiled_templates(engine)?;
        for loaded in &scripts.loaded {
            loaded.host.enable_all();
            let processors = loaded
                .host
                .processors(engine.registry(), &templates)
                .map_err(|e| ApiError::new(ApiErrorKind::ScriptFailed, e.to_string()))?;
            for processor in processors {
                setup.schedule.add_processor(processor);
            }
        }
        let hosts: Vec<ScriptHost> = scripts.loaded.iter().map(|l| l.host.clone()).collect();
        let emit = system(EMIT_LOGS, move |_: &mut World, _: &SystemCtx<'_>| {
            hosts.iter().for_each(ScriptHost::emit_logs);
        });
        setup.schedule.add_in(Phase::Presentation, emit);
        Ok(setup)
    }
}

/// The project's templates, compiled; ones that don't compile are skipped (`entity.templates`
/// reports their problems).
fn compiled_templates(engine: &Engine) -> Result<Templates, ApiError> {
    let catalog = engine.template_catalog()?;
    Ok(catalog
        .iter()
        .filter_map(|t| {
            let compiled = t.compile(engine.registry()).ok()?;
            Some((t.name.clone(), Arc::new(compiled)))
        })
        .collect())
}
