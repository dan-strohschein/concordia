//! A loaded module in the engine: its description, processors, faults and logs.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, RwLock};

use concordia_reflect::{TypeInfo, TypeRegistry};
use concordia_terra::{NamedQuery, ReflectComponent, SimClock, processor};
use concordia_terra_sim::CompiledTemplate;

use crate::processor::{ScriptProcessor, Types};
use crate::protocol::Phase as WirePhase;
use crate::{ModuleDesc, ModuleStats, ProcessorDesc, ScriptError, ScriptFault, ScriptModule};

/// Compiled templates by name, for `spawn-template` commands.
pub type Templates = BTreeMap<String, Arc<CompiledTemplate>>;

/// Host settings for one play session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HostConfig {
    /// The session's seed; processor seeds derive from it (see [`crate::processor_seed`]).
    pub session_seed: u64,
}

/// A run that faulted: nothing it produced was applied, and its processor is disabled.
#[derive(Debug, Clone, PartialEq)]
pub struct FaultReport {
    /// `script:<module>`.
    pub actor: String,
    /// The processor.
    pub processor: String,
    /// The tick it ran in.
    pub tick: u64,
    /// What went wrong.
    pub fault: ScriptFault,
}

/// A line a module logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// `script:<module>`.
    pub actor: String,
    /// The processor that was running.
    pub processor: String,
    /// The tick.
    pub tick: u64,
    /// The text.
    pub message: String,
}

/// State shared by a host and its processors.
pub(crate) struct Shared {
    pub(crate) actor: String,
    pub(crate) module: RwLock<Arc<dyn ScriptModule>>,
    pub(crate) desc: RwLock<ModuleDesc>,
    pub(crate) config: HostConfig,
    pub(crate) faults: Mutex<Vec<FaultReport>>,
    pub(crate) logs: Mutex<Vec<LogLine>>,
    pub(crate) disabled: Mutex<BTreeSet<String>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    /// The current build of the module.
    pub(crate) fn module(&self) -> Arc<dyn ScriptModule> {
        let guard = self
            .module
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(&guard)
    }

    pub(crate) fn is_disabled(&self, processor: &str) -> bool {
        lock(&self.disabled).contains(processor)
    }

    /// Records a fault and disables the processor.
    pub(crate) fn fault(&self, processor: &str, tick: u64, fault: ScriptFault) {
        tracing::warn!(actor = %self.actor, processor, tick, %fault, "script processor faulted and was disabled");
        lock(&self.disabled).insert(processor.to_owned());
        lock(&self.faults).push(FaultReport {
            actor: self.actor.clone(),
            processor: processor.to_owned(),
            tick,
            fault,
        });
    }

    /// Records a run's log lines.
    pub(crate) fn log(&self, processor: &str, tick: u64, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        let mut logs = lock(&self.logs);
        for message in lines {
            tracing::info!(actor = %self.actor, processor, tick, "{message}");
            logs.push(LogLine {
                actor: self.actor.clone(),
                processor: processor.to_owned(),
                tick,
                message,
            });
        }
    }
}

/// Terra's phase for a protocol phase.
fn phase(p: WirePhase) -> concordia_terra::Phase {
    match p {
        WirePhase::PrePhysics => concordia_terra::Phase::PrePhysics,
        WirePhase::Physics => concordia_terra::Phase::Physics,
        WirePhase::PostPhysics => concordia_terra::Phase::PostPhysics,
        WirePhase::FrameEnd => concordia_terra::Phase::FrameEnd,
        WirePhase::Presentation => concordia_terra::Phase::Presentation,
    }
}

/// Every registered component and signal, for decoding commands by name.
fn component_types(registry: &TypeRegistry) -> Types {
    registry
        .names()
        .filter_map(|name| {
            let reg = registry.get_by_name(name)?;
            let ops = *reg.data::<ReflectComponent>()?;
            Some((reg.info().name, (reg.info(), ops)))
        })
        .collect()
}

/// The registered (static-lifetime) names of `names`.
fn resolve(
    types: &Types,
    p: &ProcessorDesc,
    names: &[String],
) -> Result<Vec<&'static TypeInfo>, ScriptError> {
    names
        .iter()
        .map(|n| {
            types
                .get(n.as_str())
                .map(|(info, _)| *info)
                .ok_or_else(|| ScriptError::UnknownType {
                    processor: p.name.clone(),
                    name: n.clone(),
                })
        })
        .collect()
}

fn names(infos: &[&'static TypeInfo]) -> Vec<&'static str> {
    infos.iter().map(|i| i.name).collect()
}

/// One loaded module. Cloning shares it.
#[derive(Clone)]
pub struct ScriptHost {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for ScriptHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptHost")
            .field("actor", &self.shared.actor)
            .finish()
    }
}

impl ScriptHost {
    /// Describes `module`, validates the description and registers its types and traits.
    pub fn load(
        module: Arc<dyn ScriptModule>,
        registry: &mut TypeRegistry,
        config: HostConfig,
    ) -> Result<Self, ScriptError> {
        let desc = module.describe().map_err(ScriptError::Describe)?;
        crate::register_module(registry, &desc)?;
        let shared = Shared {
            actor: format!("script:{}", desc.name),
            module: RwLock::new(module),
            desc: RwLock::new(desc),
            config,
            faults: Mutex::default(),
            logs: Mutex::default(),
            disabled: Mutex::default(),
        };
        Ok(Self {
            shared: Arc::new(shared),
        })
    }

    /// The `script:<module>` actor its faults and logs are attributed to.
    pub fn actor(&self) -> &str {
        &self.shared.actor
    }

    /// The module's description.
    pub fn desc(&self) -> ModuleDesc {
        self.shared
            .desc
            .read()
            .map(|d| d.clone())
            .unwrap_or_default()
    }

    /// One Terra processor per declared processor; names must all be registered.
    pub fn processors(
        &self,
        registry: &TypeRegistry,
        templates: &Templates,
    ) -> Result<Vec<ScriptProcessor>, ScriptError> {
        let types = Arc::new(component_types(registry));
        let templates = Arc::new(templates.clone());
        self.desc()
            .processors
            .iter()
            .map(|p| self.processor(p, &types, &templates))
            .collect()
    }

    fn processor(
        &self,
        p: &ProcessorDesc,
        types: &Arc<Types>,
        templates: &Arc<Templates>,
    ) -> Result<ScriptProcessor, ScriptError> {
        let reads = names(&resolve(types, p, &p.reads)?);
        let writes = names(&resolve(types, p, &p.writes)?);
        let with = names(&resolve(types, p, &p.with)?);
        let without = names(&resolve(types, p, &p.without)?);
        let receives = resolve(types, p, &p.receives)?;
        let mut decl = processor(&p.name, phase(p.phase)).reads_resource::<SimClock>();
        for &name in reads.iter().chain(&with).chain(&without) {
            decl = decl.reads_named(name);
        }
        for &name in &writes {
            decl = decl.writes_named(name);
        }
        for info in &receives {
            decl = decl.receives_named(info.name);
        }
        for name in &p.after {
            decl = decl.after(name);
        }
        for name in &p.before {
            decl = decl.before(name);
        }
        let query = NamedQuery::fetch(reads.iter().chain(&writes).copied())
            .with(with)
            .without(without);
        Ok(ScriptProcessor {
            decl,
            shared: Arc::clone(&self.shared),
            query,
            writes,
            receives,
            types: Arc::clone(types),
            templates: Arc::clone(templates),
        })
    }

    /// Faults so far, in order.
    pub fn faults(&self) -> Vec<FaultReport> {
        lock(&self.shared.faults).clone()
    }

    /// Log lines so far, in order.
    pub fn logs(&self) -> Vec<LogLine> {
        lock(&self.shared.logs).clone()
    }

    /// True once `processor` has faulted (until a reload).
    pub fn is_disabled(&self, processor: &str) -> bool {
        self.shared.is_disabled(processor)
    }

    /// Boundary crossings of the current module.
    pub fn stats(&self) -> ModuleStats {
        self.shared.module().stats()
    }

    /// Swaps in a new build of the module between ticks, keeping the world. Its types are
    /// re-registered; its processors must keep their names, phases and access. Disabled
    /// processors are enabled again.
    pub fn reload(
        &self,
        module: Arc<dyn ScriptModule>,
        registry: &mut TypeRegistry,
    ) -> Result<(), ScriptError> {
        let desc = module.describe().map_err(ScriptError::Describe)?;
        let old = self.desc();
        check_reload(&old, &desc)?;
        crate::register_module(registry, &desc)?;
        *self
            .shared
            .module
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = module;
        *self
            .shared
            .desc
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = desc;
        lock(&self.shared.disabled).clear();
        Ok(())
    }
}

/// A reload may change behavior, not the module's name, processors or types (changing those
/// needs a schedule rebuild and value migration: 2.6c and 2.6d).
fn check_reload(old: &ModuleDesc, new: &ModuleDesc) -> Result<(), ScriptError> {
    let refuse = |reason: String| ScriptError::ReloadChangesProcessors {
        module: old.name.clone(),
        reason,
    };
    if old.name != new.name {
        return Err(refuse(format!("the module is now named {}", new.name)));
    }
    if old.processors != new.processors {
        return Err(refuse("its processors or their access changed".into()));
    }
    let old_types: Vec<_> = crate::register::declared_types(old).collect();
    let new_types: Vec<_> = crate::register::declared_types(new).collect();
    if old_types != new_types {
        return Err(refuse("its types changed".into()));
    }
    Ok(())
}
