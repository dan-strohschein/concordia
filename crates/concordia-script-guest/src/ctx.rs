//! A run's context: the tick, the batch, a seeded RNG, the log and the output being built.

use concordia_core::id::EntityId;
use concordia_core::rng::SimRng;
use concordia_reflect::wire::{ColumnWriter, WireColumn};
use concordia_reflect::{Reflect, TypeInfo};
use concordia_script_protocol::{Batch, Column, Command, RunOutput, Target, TickContext, Value};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Module;

/// One run of one processor: read the batch, compute, record writes and commands.
pub struct RunCtx<'a> {
    tick: TickContext,
    batch: &'a Batch,
    rng: SimRng,
    log: &'a mut dyn FnMut(&str),
    out: RunOutput,
    spawns: u32,
}

impl std::fmt::Debug for RunCtx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunCtx")
            .field("tick", &self.tick)
            .field("rows", &self.batch.ids.len())
            .finish()
    }
}

/// Encodes `value` as a one-row wire value of its type.
pub fn value<T: Reflect + Serialize>(value: &T) -> Result<Value, String> {
    Ok(Value {
        name: T::type_info().name.to_owned(),
        data: encode(std::slice::from_ref(value))?,
    })
}

/// Encodes `values` as one wire column of `T`.
fn encode<T: Reflect + Serialize>(values: &[T]) -> Result<Vec<u8>, String> {
    let info = T::type_info();
    let fail = |e: concordia_reflect::wire::WireError| format!("{}: {e}", info.name);
    let mut writer = ColumnWriter::new(info, values.len()).map_err(fail)?;
    for (row, v) in values.iter().enumerate() {
        writer.write(row, v).map_err(fail)?;
    }
    Ok(writer.finish().map_err(fail)?.into_bytes())
}

/// Decodes all `rows` rows of a column of `info` into `T`s.
fn decode<T: DeserializeOwned>(
    info: &'static TypeInfo,
    rows: usize,
    data: &[u8],
) -> Result<Vec<T>, String> {
    let fail = |e: concordia_reflect::wire::WireError| format!("{}: {e}", info.name);
    let column = WireColumn::from_bytes(info, rows, data.to_vec()).map_err(fail)?;
    (0..rows)
        .map(|row| column.read(row).map_err(fail))
        .collect()
}

/// Runs `M`'s processor `processor` on `batch`; log lines go to `log`.
pub fn run_module<M: Module>(
    processor: &str,
    tick: TickContext,
    batch: &Batch,
    log: &mut dyn FnMut(&str),
) -> Result<RunOutput, String> {
    let mut ctx = RunCtx::new(tick, batch, log);
    M::run(processor, &mut ctx)?;
    Ok(ctx.finish())
}

impl<'a> RunCtx<'a> {
    /// A context for one run.
    pub fn new(tick: TickContext, batch: &'a Batch, log: &'a mut dyn FnMut(&str)) -> Self {
        Self {
            tick,
            batch,
            rng: SimRng::new(tick.seed),
            log,
            out: RunOutput::default(),
            spawns: 0,
        }
    }

    /// The tick being simulated.
    pub fn tick(&self) -> u64 {
        self.tick.tick
    }

    /// Simulated seconds this tick.
    pub fn dt(&self) -> f32 {
        self.tick.dt
    }

    /// Simulated seconds since play started.
    pub fn time(&self) -> f64 {
        self.tick.time
    }

    /// This processor's random stream for this tick (seeded by the engine).
    pub fn rng(&mut self) -> &mut SimRng {
        &mut self.rng
    }

    /// The matched entities; row `i` of every column belongs to `ids()[i]`.
    pub fn ids(&self) -> &[EntityId] {
        &self.batch.ids
    }

    /// Every row of component `T` (a declared read or write).
    pub fn read<T: Reflect + DeserializeOwned>(&self) -> Result<Vec<T>, String> {
        let info = T::type_info();
        let column = self
            .batch
            .column(info.name)
            .ok_or_else(|| format!("{} is not in this batch; declare it", info.name))?;
        decode(info, self.batch.ids.len(), &column.data)
    }

    /// Writes component `T` for every row (a declared write).
    pub fn write<T: Reflect + Serialize>(&mut self, values: &[T]) -> Result<(), String> {
        let name = T::type_info().name;
        if values.len() != self.batch.ids.len() {
            return Err(format!(
                "{name}: {} rows written for {} batch rows",
                values.len(),
                self.batch.ids.len()
            ));
        }
        let column = Column {
            name: name.to_owned(),
            data: encode(values)?,
        };
        match self.out.writes.iter_mut().find(|c| c.name == name) {
            Some(existing) => *existing = column,
            None => self.out.writes.push(column),
        }
        Ok(())
    }

    /// The delivered signals of type `T`, by entity then raise order.
    pub fn signals<T: Reflect + DeserializeOwned>(&self) -> Result<Vec<(EntityId, T)>, String> {
        let info = T::type_info();
        let Some(column) = self.batch.signal(info.name) else {
            return Ok(Vec::new());
        };
        let payloads = decode(info, column.targets.len(), &column.data)?;
        Ok(column.targets.iter().copied().zip(payloads).collect())
    }

    /// Spawns an entity with `components` (see [`value`]); returns its pending target.
    pub fn spawn(&mut self, components: Vec<Value>) -> Target {
        self.out.commands.push(Command::Spawn(components));
        self.next_pending()
    }

    /// Spawns an entity from template `name`; returns its pending target.
    pub fn spawn_template(&mut self, name: &str) -> Target {
        self.out
            .commands
            .push(Command::SpawnTemplate(name.to_owned()));
        self.next_pending()
    }

    fn next_pending(&mut self) -> Target {
        let target = Target::Pending(self.spawns);
        self.spawns += 1;
        target
    }

    /// Inserts or replaces component `T` on `target`.
    pub fn insert<T: Reflect + Serialize>(
        &mut self,
        target: impl Into<Target>,
        component: &T,
    ) -> Result<(), String> {
        let command = Command::Insert(target.into(), value(component)?);
        self.out.commands.push(command);
        Ok(())
    }

    /// Removes component `T` from `target`.
    pub fn remove<T: Reflect>(&mut self, target: impl Into<Target>) {
        let name = T::type_info().name.to_owned();
        self.out.commands.push(Command::Remove(target.into(), name));
    }

    /// Despawns `target`.
    pub fn despawn(&mut self, target: impl Into<Target>) {
        self.out.commands.push(Command::Despawn(target.into()));
    }

    /// Raises signal `T` on `target` (delivered next tick).
    pub fn signal<T: Reflect + Serialize>(
        &mut self,
        target: impl Into<Target>,
        signal: &T,
    ) -> Result<(), String> {
        let command = Command::Signal(target.into(), value(signal)?);
        self.out.commands.push(command);
        Ok(())
    }

    /// Logs a line, attributed to the module.
    pub fn log(&mut self, message: &str) {
        (self.log)(message);
    }

    /// The run's output.
    pub fn finish(self) -> RunOutput {
        self.out
    }
}

#[cfg(test)]
mod tests;
