//! The in-process test backend: runs SDK modules linked into the engine, natively. A panic is
//! treated like a WASM trap. Statics *do* survive between runs here (2.6c's source check
//! rejects them for native modules).

use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use concordia_script_guest::{Module, run_module};

use crate::{
    Batch, ModuleDesc, ModuleSource, ModuleStats, RunReport, ScriptBackend, ScriptError,
    ScriptFault, ScriptModule, TickContext,
};

type DescribeFn = fn() -> ModuleDesc;
type RunFn = fn(
    &str,
    TickContext,
    &Batch,
    &mut dyn FnMut(&str),
) -> Result<concordia_script_protocol::RunOutput, String>;

#[derive(Clone, Copy)]
struct Linked {
    describe: DescribeFn,
    run: RunFn,
}

/// Loads modules linked into this process ([`ModuleSource::Linked`] by module name).
#[derive(Default, Clone)]
pub struct TestBackend {
    modules: BTreeMap<String, Linked>,
}

impl std::fmt::Debug for TestBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestBackend")
            .field("modules", &self.modules.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl TestBackend {
    /// A backend with no modules.
    pub fn new() -> Self {
        Self::default()
    }

    /// Links module `M` under the name its description gives.
    #[must_use]
    pub fn link<M: Module>(mut self) -> Self {
        let linked = Linked {
            describe: M::describe,
            run: run_module::<M>,
        };
        self.modules.insert(M::describe().name, linked);
        self
    }
}

impl ScriptBackend for TestBackend {
    fn name(&self) -> &'static str {
        "test"
    }

    fn load(&self, source: &ModuleSource) -> Result<Arc<dyn ScriptModule>, ScriptError> {
        let ModuleSource::Linked(name) = source else {
            return Err(ScriptError::Unsupported {
                backend: self.name(),
                source_kind: format!("{source:?}"),
            });
        };
        let linked = *self
            .modules
            .get(name)
            .ok_or_else(|| ScriptError::UnknownModule(name.clone()))?;
        Ok(Arc::new(TestModule {
            linked,
            calls_in: AtomicU64::new(0),
            calls_out: AtomicU64::new(0),
        }))
    }
}

struct TestModule {
    linked: Linked,
    calls_in: AtomicU64,
    calls_out: AtomicU64,
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".to_owned())
}

impl ScriptModule for TestModule {
    fn describe(&self) -> Result<ModuleDesc, ScriptFault> {
        self.calls_in.fetch_add(1, Ordering::Relaxed);
        std::panic::catch_unwind(self.linked.describe)
            .map_err(|p| ScriptFault::Trap(panic_message(p.as_ref())))
    }

    fn run(&self, processor: &str, ctx: &TickContext, batch: &Batch) -> RunReport {
        self.calls_in.fetch_add(1, Ordering::Relaxed);
        let mut logs = Vec::new();
        let mut sink = |line: &str| {
            self.calls_out.fetch_add(1, Ordering::Relaxed);
            logs.push(line.to_owned());
        };
        let run = self.linked.run;
        let outcome =
            std::panic::catch_unwind(AssertUnwindSafe(|| run(processor, *ctx, batch, &mut sink)));
        let result = match outcome {
            Ok(Ok(out)) => Ok(out),
            Ok(Err(message)) => Err(ScriptFault::Returned(message)),
            Err(p) => Err(ScriptFault::Trap(panic_message(p.as_ref()))),
        };
        RunReport { result, logs }
    }

    fn stats(&self) -> ModuleStats {
        ModuleStats {
            calls_in: self.calls_in.load(Ordering::Relaxed),
            calls_out: self.calls_out.load(Ordering::Relaxed),
        }
    }
}
