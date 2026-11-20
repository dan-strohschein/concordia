//! A script processor: a Terra processor whose run is one protocol call. Its output is staged
//! in full (decoded and checked) before anything is applied, so a bad run changes nothing.

use std::collections::BTreeMap;
use std::sync::Arc;

use concordia_core::id::EntityId;
use concordia_reflect::wire::WireColumn;
use concordia_reflect::{DynReflect, TypeInfo};
use concordia_terra::{
    CommandBuffer, NamedQuery, Pending, Phase, Processor, ProcessorDecl, ReflectComponent,
    SimClock, SystemCtx, View,
};

use crate::host::{Shared, Templates};
use crate::{Batch, Column, Command, RunOutput, ScriptFault, SignalColumn, TickContext, Value};

/// Every registered component and signal by name: its type and its operations.
pub(crate) type Types = BTreeMap<&'static str, (&'static TypeInfo, ReflectComponent)>;

/// One of a module's processors, scheduled in Terra like any other.
pub struct ScriptProcessor {
    pub(crate) decl: ProcessorDecl,
    pub(crate) shared: Arc<Shared>,
    pub(crate) query: NamedQuery,
    pub(crate) writes: Vec<&'static str>,
    pub(crate) receives: Vec<&'static TypeInfo>,
    pub(crate) types: Arc<Types>,
    pub(crate) templates: Arc<Templates>,
}

impl std::fmt::Debug for ScriptProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptProcessor")
            .field("name", &self.decl.name())
            .field("module", &self.shared.actor)
            .finish()
    }
}

/// A run's effects, checked and ready to apply.
struct Staged {
    writes: Vec<WireColumn>,
    commands: CommandBuffer,
}

impl Processor for ScriptProcessor {
    fn decl(&self) -> &ProcessorDecl {
        &self.decl
    }

    fn run(&mut self, view: &mut View<'_>, ctx: &SystemCtx<'_>) {
        let name = self.decl.name().to_owned();
        if self.shared.is_disabled(&name) {
            return;
        }
        let tick = ctx.tick.0;
        let outcome = self.run_once(view, ctx, &name);
        if let Err(fault) = outcome {
            self.shared.fault(&name, tick, fault);
        }
    }
}

impl ScriptProcessor {
    /// Collects the batch, calls the module, then stages and applies its output.
    fn run_once(
        &self,
        view: &mut View<'_>,
        ctx: &SystemCtx<'_>,
        name: &str,
    ) -> Result<(), ScriptFault> {
        let tick = ctx.tick.0;
        let (ids, batch) = self.collect(view).map_err(ScriptFault::Protocol)?;
        let time = view.resource::<SimClock>().map(|c| c.time).unwrap_or(0.0);
        let tick_ctx = TickContext {
            tick,
            dt: ctx.dt,
            time,
            seed: crate::processor_seed(self.shared.config.session_seed, tick, name),
        };
        let report = self.shared.module().run(name, &tick_ctx, &batch);
        self.shared.log(name, tick, report.logs);
        let output = report.result?;
        let staged = self
            .stage(&output, ids.len())
            .map_err(ScriptFault::Protocol)?;
        self.apply(view, &ids, staged)
            .map_err(ScriptFault::Protocol)
    }

    /// The declared columns of every match, and the delivered signals.
    fn collect(&self, view: &mut View<'_>) -> Result<(Vec<EntityId>, Batch), String> {
        let found = view.batch(&self.query).map_err(|e| e.to_string())?;
        let mut columns: Vec<Column> = found
            .columns
            .into_iter()
            .map(|c| Column {
                name: c.info().name.to_owned(),
                data: c.into_bytes(),
            })
            .collect();
        for name in &self.query.fetch {
            if !columns.iter().any(|c| c.name == *name) {
                // A type the world never stored: no rows, so no bytes.
                columns.push(Column {
                    name: (*name).to_owned(),
                    data: Vec::new(),
                });
            }
        }
        let mut signals = Vec::with_capacity(self.receives.len());
        for info in &self.receives {
            let (targets, column) = view.signal_batch(info).map_err(|e| e.to_string())?;
            signals.push(SignalColumn {
                name: info.name.to_owned(),
                targets,
                data: column.into_bytes(),
            });
        }
        let batch = Batch {
            ids: found.ids.clone(),
            columns,
            signals,
        };
        Ok((found.ids, batch))
    }

    /// Decodes and checks every write and command; nothing touches the world yet.
    fn stage(&self, output: &RunOutput, rows: usize) -> Result<Staged, String> {
        let mut writes: Vec<WireColumn> = Vec::with_capacity(output.writes.len());
        for column in &output.writes {
            let Some(&name) = self.writes.iter().find(|w| **w == column.name) else {
                return Err(format!(
                    "wrote {}, which it did not declare as a write",
                    column.name
                ));
            };
            if writes.iter().any(|w| w.info().name == name) {
                return Err(format!("wrote {name} twice"));
            }
            let (info, _) = self.type_of(name)?;
            let decoded = WireColumn::from_bytes(info, rows, column.data.clone())
                .map_err(|e| format!("{name}: {e}"))?;
            writes.push(decoded);
        }
        if self.decl.phase() == Phase::Presentation && !output.commands.is_empty() {
            return Err("presentation processors can't record commands".into());
        }
        let commands = self.stage_commands(&output.commands)?;
        Ok(Staged { writes, commands })
    }

    fn stage_commands(&self, commands: &[Command]) -> Result<CommandBuffer, String> {
        let mut buffer = CommandBuffer::new();
        let mut spawned: Vec<Pending> = Vec::new();
        for command in commands {
            match command {
                Command::Spawn(values) => {
                    let entity = buffer.spawn();
                    for value in values {
                        let (ops, decoded) = self.decode(value)?;
                        buffer.insert_reflect(entity, ops, decoded);
                    }
                    spawned.push(entity);
                }
                Command::SpawnTemplate(name) => {
                    let template = self
                        .templates
                        .get(name)
                        .ok_or_else(|| format!("spawn-template: unknown template {name:?}"))?;
                    spawned.push(template.record(&mut buffer));
                }
                Command::Insert(target, value) => {
                    let target = resolve(*target, &spawned)?;
                    let (ops, decoded) = self.decode(value)?;
                    buffer.insert_reflect(target, ops, decoded);
                }
                Command::Remove(target, name) => {
                    let target = resolve(*target, &spawned)?;
                    let (info, _) = self.type_of(name)?;
                    buffer.remove_named(target, info.name);
                }
                Command::Despawn(target) => buffer.despawn(resolve(*target, &spawned)?),
                Command::Signal(target, value) => {
                    let target = resolve(*target, &spawned)?;
                    let (ops, decoded) = self.decode(value)?;
                    buffer.signal_reflect(target, ops, decoded);
                }
            }
        }
        Ok(buffer)
    }

    fn type_of(&self, name: &str) -> Result<(&'static TypeInfo, ReflectComponent), String> {
        self.types
            .get(name)
            .copied()
            .ok_or_else(|| format!("{name} is not a registered component or signal"))
    }

    /// A one-row value as its component's value.
    fn decode(&self, value: &Value) -> Result<(ReflectComponent, Box<dyn DynReflect>), String> {
        let (info, ops) = self.type_of(&value.name)?;
        let fail = |e: concordia_reflect::wire::WireError| format!("{}: {e}", value.name);
        let column = WireColumn::from_bytes(info, 1, value.data.clone()).map_err(fail)?;
        Ok((ops, ops.from_wire(&column, 0).map_err(fail)?))
    }

    /// Writes the staged columns back to the batch rows, then queues the commands.
    fn apply(&self, view: &mut View<'_>, ids: &[EntityId], staged: Staged) -> Result<(), String> {
        for column in &staged.writes {
            view.write_batch(ids, column).map_err(|e| e.to_string())?;
        }
        if !staged.commands.is_empty() {
            view.commands()
                .map_err(|e| e.to_string())?
                .append(staged.commands);
        }
        Ok(())
    }
}

/// A protocol target as a command-buffer target; pending targets name earlier spawns.
fn resolve(target: crate::Target, spawned: &[Pending]) -> Result<concordia_terra::Target, String> {
    match target {
        crate::Target::Entity(id) => Ok(id.into()),
        crate::Target::Pending(n) => spawned
            .get(n as usize)
            .map(|p| (*p).into())
            .ok_or_else(|| format!("pending target {n} names no earlier spawn")),
    }
}
