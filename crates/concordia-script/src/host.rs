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
    /// Registered components and signals, as processors resolve them (refreshed on reload).
    pub(crate) types: RwLock<Arc<Types>>,
    /// Compiled templates for `spawn-template`.
    pub(crate) templates: RwLock<Arc<Templates>>,
    /// How many log lines and faults [`ScriptHost::emit_logs`] has sent.
    pub(crate) emitted: Mutex<(usize, usize)>,
}

fn read<T: Clone>(lock: &RwLock<T>) -> T {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn write<T>(lock: &RwLock<T>, value: T) {
    *lock
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = value;
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Shared {
    /// The current build of the module.
    pub(crate) fn module(&self) -> Arc<dyn ScriptModule> {
        read(&self.module)
    }

    /// The current component and signal types.
    pub(crate) fn types(&self) -> Arc<Types> {
        read(&self.types)
    }

    /// The current templates.
    pub(crate) fn templates(&self) -> Arc<Templates> {
        read(&self.templates)
    }

    pub(crate) fn is_disabled(&self, processor: &str) -> bool {
        lock(&self.disabled).contains(processor)
    }

    /// Records a fault and disables the processor.
    pub(crate) fn fault(&self, processor: &str, tick: u64, fault: ScriptFault) {
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

/// What a reload changed: the component and signal types it migrated.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReloadReport {
    /// One entry per changed type, in declaration order.
    pub migrated: Vec<TypeMigration>,
}

/// One type moved to its new version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMigration {
    /// The type.
    pub name: String,
    /// Stored values converted.
    pub values: usize,
    /// Fields the new version added, as JSON Pointer paths (stored values took their defaults).
    pub added: Vec<String>,
    /// Fields the new version dropped, as JSON Pointer paths (their values are gone).
    pub removed: Vec<String>,
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
            types: RwLock::new(Arc::new(component_types(registry))),
            templates: RwLock::default(),
            emitted: Mutex::default(),
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
        write(&self.shared.types, Arc::clone(&types));
        write(&self.shared.templates, Arc::clone(&templates));
        self.desc()
            .processors
            .iter()
            .map(|p| self.processor(p, &types, &templates))
            .collect()
    }

    /// Replaces the templates `spawn-template` uses (recompile them after a reload changes a
    /// type they contain).
    pub fn set_templates(&self, templates: Templates) {
        write(&self.shared.templates, Arc::new(templates));
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
            receive_names: names(&receives),
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

    /// Sends the log lines and faults recorded since the last call to `tracing`, attributed to
    /// `script:<module>`, ordered by tick, then by processor declaration order (each run's lines,
    /// then its fault), so the log is the same at any thread count. Call it from the engine
    /// thread.
    pub fn emit_logs(&self) {
        let order: Vec<String> = self.desc().processors.into_iter().map(|p| p.name).collect();
        let rank = |processor: &str| {
            order
                .iter()
                .position(|p| p == processor)
                .unwrap_or(usize::MAX)
        };
        let mut emitted = lock(&self.shared.emitted);
        let logs = lock(&self.shared.logs);
        let faults = lock(&self.shared.faults);
        let mut events: Vec<(u64, usize, u8, usize)> = Vec::new();
        for (i, line) in logs.iter().enumerate().skip(emitted.0) {
            events.push((line.tick, rank(&line.processor), 0, i));
        }
        for (i, fault) in faults.iter().enumerate().skip(emitted.1) {
            events.push((fault.tick, rank(&fault.processor), 1, i));
        }
        events.sort_unstable();
        for (_, _, kind, i) in events {
            match kind {
                0 => {
                    let l = &logs[i];
                    tracing::info!(target: "concordia::script", actor = %l.actor, processor = %l.processor, tick = l.tick, "{}", l.message);
                }
                _ => {
                    let f = &faults[i];
                    tracing::warn!(target: "concordia::script", actor = %f.actor, processor = %f.processor, tick = f.tick, fault = %f.fault, "script processor faulted and was disabled");
                }
            }
        }
        *emitted = (logs.len(), faults.len());
    }

    /// Enables every processor again (a new play session).
    pub fn enable_all(&self) {
        lock(&self.shared.disabled).clear();
    }

    /// True once `processor` has faulted (until a reload).
    pub fn is_disabled(&self, processor: &str) -> bool {
        self.shared.is_disabled(processor)
    }

    /// Boundary crossings of the current module.
    pub fn stats(&self) -> ModuleStats {
        self.shared.module().stats()
    }

    /// Swaps in a new build of the module between ticks, keeping the world. Changed component
    /// and signal types are re-registered and the world's values migrated through reflect (all
    /// or nothing; see [`ReloadReport`]); traits are re-registered. The module's name and
    /// processors (names, phases, access, order) must not change, nor may types be removed.
    /// Disabled processors are enabled again.
    pub fn reload(
        &self,
        module: Arc<dyn ScriptModule>,
        registry: &mut TypeRegistry,
        world: &mut concordia_terra::World,
    ) -> Result<ReloadReport, ScriptError> {
        let desc = module.describe().map_err(ScriptError::Describe)?;
        desc.validate()?;
        let old = self.desc();
        check_reload(&old, &desc)?;
        let migrated = crate::reload::migrate_types(registry, world, &old, &desc);
        write(&self.shared.types, Arc::new(component_types(registry)));
        let migrated = migrated?;
        write(&self.shared.module, module);
        write(&self.shared.desc, desc);
        lock(&self.shared.disabled).clear();
        Ok(ReloadReport { migrated })
    }
}

/// A reload may change behavior and types, not the module's name or processors (that needs a
/// schedule rebuild: 2.6d), and may not remove types.
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
    let kept: Vec<&str> = crate::register::declared_types(new)
        .map(|t| t.name.as_str())
        .collect();
    match crate::register::declared_types(old).find(|t| !kept.contains(&t.name.as_str())) {
        Some(gone) => Err(refuse(format!("it no longer declares {}", gone.name))),
        None => Ok(()),
    }
}
