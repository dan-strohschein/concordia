//! A loaded graph: its processors over the batch's wire columns, compiled or interpreted.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use concordia_core::id::EntityId;
use concordia_graph::runtime::JobTrace;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use concordia_graph::runtime::{Entry, EventModule, Image, Program, RuntimeError};
use concordia_script::{ModuleStats, RunReport, ScriptError, ScriptFault, ScriptModule};
use concordia_script_protocol::{
    Batch, Column, Command, ModuleDesc, Phase, ProcessorDesc, RunOutput, Target, TickContext, Value,
};
use concordia_script_wasm::Fuel;

use crate::Mode;

/// How one processor runs.
enum Runner {
    Wasm {
        engine: wasmtime::Engine,
        module: wasmtime::Module,
        fuel: Option<Fuel>,
    },
    Interpret {
        budget: Option<u64>,
    },
}

/// A compiled Event graph as a script module.
pub struct GraphModule {
    desc: ModuleDesc,
    processors: HashMap<String, (Program, Runner)>,
    calls: AtomicU64,
    trace: Mutex<Option<Tracing>>,
}

/// An entity being traced and what was recorded so far.
struct Tracing {
    entity: EntityId,
    records: Vec<TickTrace>,
}

/// One run's trace for the traced entity (M4 4.10c, `graph.trace`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct TickTrace {
    /// The tick.
    pub tick: u64,
    /// The processor (one per event node).
    pub processor: String,
    /// Per job on the entity (one for a tick; one per delivered signal): what ran and the values.
    pub jobs: Vec<JobTrace>,
}

impl std::fmt::Debug for GraphModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphModule")
            .field("name", &self.desc.name)
            .finish()
    }
}

impl GraphModule {
    /// The module for `module`, its processors prepared for `mode`.
    pub(crate) fn new(
        module: EventModule,
        mode: &Mode,
        engine: Option<&wasmtime::Engine>,
    ) -> Result<Self, ScriptError> {
        let desc = describe(&module);
        let mut processors = HashMap::new();
        for program in module.processors {
            let runner = match (mode, engine) {
                (Mode::Wasm(config), Some(engine)) => {
                    let compiled = wasmtime::Module::new(engine, program.wasm())
                        .map_err(|e| ScriptError::Load(format!("{}: {e}", program.name)))?;
                    Runner::Wasm {
                        engine: engine.clone(),
                        module: compiled,
                        fuel: config.fuel,
                    }
                }
                (Mode::Interpret { budget }, _) => Runner::Interpret { budget: *budget },
                (Mode::Wasm(_), None) => {
                    return Err(ScriptError::Load("WASM mode without an engine".into()));
                }
            };
            processors.insert(program.name.clone(), (program, runner));
        }
        Ok(Self {
            desc,
            processors,
            calls: AtomicU64::new(0),
            trace: Mutex::new(None),
        })
    }

    /// Starts recording what this module's processors do for `entity` (replacing any trace).
    pub fn start_trace(&self, entity: EntityId) {
        *self.trace.lock().unwrap_or_else(PoisonError::into_inner) = Some(Tracing {
            entity,
            records: Vec::new(),
        });
    }

    /// Stops recording and returns what was recorded, in run order.
    pub fn take_trace(&self) -> Vec<TickTrace> {
        self.trace
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .map(|t| t.records)
            .unwrap_or_default()
    }

    /// Records the traced entity's jobs in `image` (before the run changes it).
    fn record(&self, p: &Program, tick: u64, ids: &[EntityId], image: &Image) {
        let mut guard = self.trace.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(tracing) = guard.as_mut() else {
            return;
        };
        let Some(row) = ids.iter().position(|id| *id == tracing.entity) else {
            return;
        };
        let jobs: Vec<JobTrace> = (0..p.jobs(image).unwrap_or(0))
            .filter(|&j| p.job_row(image, j).ok() == Some(row))
            .filter_map(|j| p.trace_job(image, j).ok())
            .collect();
        if !jobs.is_empty() {
            tracing.records.push(TickTrace {
                tick,
                processor: p.name.clone(),
                jobs,
            });
        }
    }
}

/// The module's description: the `Vars` component and one processor per program.
fn describe(module: &EventModule) -> ModuleDesc {
    let processors = module
        .processors
        .iter()
        .map(|p| ProcessorDesc {
            name: p.name.clone(),
            phase: Phase::default(),
            reads: p.reads.clone(),
            writes: p.writes.clone(),
            with: p.with.clone(),
            receives: match &p.entry {
                Entry::Signal(s) => vec![s.clone()],
                Entry::Tick => vec![],
            },
            ..ProcessorDesc::default()
        })
        .collect();
    ModuleDesc {
        name: module.name.clone(),
        version: 1,
        components: vec![module.vars.clone()],
        processors,
        ..ModuleDesc::default()
    }
}

impl ScriptModule for GraphModule {
    fn describe(&self) -> Result<ModuleDesc, ScriptFault> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(self.desc.clone())
    }

    fn run(&self, processor: &str, ctx: &TickContext, batch: &Batch) -> RunReport {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let result = match self.processors.get(processor) {
            Some((program, runner)) => run(program, runner, ctx, batch, |image| {
                self.record(program, ctx.tick, &batch.ids, image);
            }),
            None => Err(ScriptFault::Returned(format!("no processor {processor:?}"))),
        };
        RunReport {
            result,
            logs: Vec::new(),
        }
    }

    fn stats(&self) -> ModuleStats {
        ModuleStats {
            calls_in: self.calls.load(Ordering::Relaxed),
            calls_out: 0,
        }
    }
}

/// One run: lay out the image, run the jobs, read back the writes and the raises.
fn run(
    p: &Program,
    runner: &Runner,
    ctx: &TickContext,
    batch: &Batch,
    record: impl FnOnce(&Image),
) -> Result<RunOutput, ScriptFault> {
    let rows = batch.ids.len();
    let mut arenas = HashMap::new();
    let mut columns = Vec::new();
    for slot in p.columns() {
        let data = batch
            .column(&slot.name)
            .map_or(&[][..], |c| c.data.as_slice());
        let fixed = rows * slot.stride;
        if data.len() < fixed {
            return Err(ScriptFault::Protocol(format!(
                "column {} is shorter than {rows} rows",
                slot.name
            )));
        }
        columns.push(data[..fixed].to_vec());
        arenas.insert(slot.name.clone(), data[fixed..].to_vec());
    }
    let (mut image, job_rows) = match &p.entry {
        Entry::Tick => (
            Image::tick(p, rows, ctx.dt, &columns).map_err(protocol)?,
            (0..rows).collect(),
        ),
        Entry::Signal(name) => {
            let index: HashMap<_, _> = batch
                .ids
                .iter()
                .enumerate()
                .map(|(k, id)| (*id, k))
                .collect();
            let (targets, payloads) = batch.signal(name).map_or((vec![], vec![]), |s| {
                (
                    s.targets.iter().map(|t| index.get(t).copied()).collect(),
                    s.data.clone(),
                )
            });
            let job_rows: Vec<usize> = targets.iter().flatten().copied().collect();
            (
                Image::signals(p, &targets, &payloads, &columns).map_err(protocol)?,
                job_rows,
            )
        }
    };
    record(&image);
    execute(p, runner, &mut image, rows)?;
    let writes = p
        .writes
        .iter()
        .map(|name| {
            let mut data = image.column(p, name).unwrap_or_default().to_vec();
            data.extend(arenas.get(name).cloned().unwrap_or_default());
            Column {
                name: name.clone(),
                data,
            }
        })
        .collect();
    let commands = image
        .raises(p)
        .map_err(protocol)?
        .into_iter()
        .filter_map(|r| {
            let row = *job_rows.get(r.job as usize)?;
            Some(Command::Signal(
                Target::Entity(batch.ids[row]),
                Value {
                    name: r.signal,
                    data: r.payload,
                },
            ))
        })
        .collect();
    Ok(RunOutput { writes, commands })
}

fn protocol(e: RuntimeError) -> ScriptFault {
    ScriptFault::Protocol(e.to_string())
}

/// Runs the image's jobs in place.
fn execute(
    p: &Program,
    runner: &Runner,
    image: &mut Image,
    rows: usize,
) -> Result<(), ScriptFault> {
    match runner {
        Runner::Interpret { budget } => p.interpret_with(image, *budget).map_err(|e| match e {
            RuntimeError::Budget => {
                ScriptFault::OutOfFuel(format!("{}: step budget exhausted", p.name))
            }
            other => protocol(other),
        }),
        Runner::Wasm {
            engine,
            module,
            fuel,
        } => run_wasm(engine, module, *fuel, image, rows),
    }
}

/// A fresh instance per run (no state survives), fuel as scripts get it.
fn run_wasm(
    engine: &wasmtime::Engine,
    module: &wasmtime::Module,
    fuel: Option<Fuel>,
    image: &mut Image,
    rows: usize,
) -> Result<(), ScriptFault> {
    let trap = |e: wasmtime::Error| match e.downcast_ref::<wasmtime::Trap>() {
        Some(wasmtime::Trap::OutOfFuel) => ScriptFault::OutOfFuel(e.to_string()),
        _ => ScriptFault::Trap(e.to_string()),
    };
    let mut store = wasmtime::Store::new(engine, ());
    if let Some(fuel) = fuel {
        store.set_fuel(fuel.budget(rows)).map_err(trap)?;
    }
    let instance = wasmtime::Instance::new(&mut store, module, &[]).map_err(trap)?;
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| ScriptFault::Protocol("no memory export".into()))?;
    let pages = image.bytes.len().div_ceil(65536) as u64;
    let have = memory.size(&store);
    if pages > have {
        memory
            .grow(&mut store, pages - have)
            .map_err(|e| ScriptFault::Trap(format!("memory: {e}")))?;
    }
    memory
        .write(&mut store, 0, &image.bytes)
        .map_err(|e| ScriptFault::Protocol(e.to_string()))?;
    let run = instance
        .get_typed_func::<(), ()>(&mut store, "run")
        .map_err(trap)?;
    run.call(&mut store, ()).map_err(trap)?;
    let len = image.bytes.len();
    image.bytes.copy_from_slice(&memory.data(&store)[..len]);
    Ok(())
}
