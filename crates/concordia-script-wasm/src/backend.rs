//! Loading components (through the DDC) and running them, one fresh instance per run.

use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use concordia_core::ddc::{Ddc, DdcKey};
use concordia_script::{
    Batch, ModuleDesc, ModuleSource, ModuleStats, RunReport, ScriptBackend, ScriptError,
    ScriptFault, ScriptModule, TickContext,
};
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{
    Config, Engine, InstanceAllocationStrategy, PoolingAllocationConfig, Store, Strategy, Trap,
};
use wasmtime_wasi::{WasiCtxView, WasiView};

use crate::bindings::{Gameplay, GameplayPre};
use crate::wasi::RunWasi;
use crate::{WasmConfig, convert};

/// The Wasmtime version components are compiled with (pinned in the workspace manifest).
pub const WASMTIME_VERSION: &str = "51.0.0";

/// The DDC namespace of precompiled components (bump to invalidate every entry).
const CACHE_NAMESPACE: &[u8] = b"concordia-script-wasm/component/v1";

/// The WASM backend: one Wasmtime engine with the determinism settings, and an optional DDC.
pub struct WasmBackend {
    config: WasmConfig,
    engine: Engine,
    compat: [u8; 32],
    ddc: Option<Ddc>,
}

impl std::fmt::Debug for WasmBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmBackend")
            .field("config", &self.config)
            .field("ddc", &self.ddc)
            .finish()
    }
}

/// A `Hasher` that feeds blake3, to fingerprint Wasmtime's compatibility hash stably.
struct Blake3Hasher(blake3::Hasher);

impl std::hash::Hasher for Blake3Hasher {
    fn finish(&self) -> u64 {
        let mut first = [0u8; 8];
        first.copy_from_slice(&self.0.finalize().as_bytes()[..8]);
        u64::from_le_bytes(first)
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
}

/// The engine configuration: AOT Cranelift, deterministic, metered, pooled. Wasm threads are
/// unavailable: Wasmtime is built without its `threads` feature. Compiled graphs share it (M4
/// 4.10b).
pub fn engine_config(config: &WasmConfig) -> Config {
    let mut c = Config::new();
    c.wasm_component_model(true)
        .strategy(Strategy::Cranelift)
        .cranelift_nan_canonicalization(config.nan_canonicalization)
        .relaxed_simd_deterministic(true)
        .consume_fuel(config.fuel.is_some())
        .epoch_interruption(false);
    let instances = config.max_instances.max(1);
    let mut pool = PoolingAllocationConfig::default();
    pool.total_component_instances(instances)
        .total_core_instances(instances.saturating_mul(8))
        .total_memories(instances.saturating_mul(2))
        .total_tables(instances.saturating_mul(8))
        .total_stacks(instances)
        .max_memory_size(config.memory_cap);
    c.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));
    c
}

impl WasmBackend {
    /// A backend with `config`.
    pub fn new(config: WasmConfig) -> Result<Self, ScriptError> {
        if config.memory_cap == 0 || config.max_instances == 0 {
            return Err(ScriptError::Load(
                "memory_cap and max_instances must be positive".into(),
            ));
        }
        let engine = Engine::new(&engine_config(&config))
            .map_err(|e| ScriptError::Load(format!("engine: {e}")))?;
        let mut hasher = Blake3Hasher(blake3::Hasher::new());
        engine.precompile_compatibility_hash().hash(&mut hasher);
        let compat = *hasher.0.finalize().as_bytes();
        Ok(Self {
            config,
            engine,
            compat,
            ddc: None,
        })
    }

    /// Caches precompiled components in `ddc`.
    #[must_use]
    pub fn with_cache(mut self, ddc: Ddc) -> Self {
        self.ddc = Some(ddc);
        self
    }

    /// The DDC key of `wasm`'s precompiled form under this backend's engine: the wasm, the
    /// Wasmtime version and the engine's compatibility hash (which covers its config).
    pub fn cache_key(&self, wasm: &[u8]) -> DdcKey {
        DdcKey::of(&[
            CACHE_NAMESPACE,
            wasm,
            WASMTIME_VERSION.as_bytes(),
            &self.compat,
        ])
    }

    /// Loads a component from its bytes, from the DDC when it holds a precompiled copy.
    pub fn load_wasm(&self, wasm: &[u8]) -> Result<Arc<WasmModule>, ScriptError> {
        let (component, from_cache) = self.component(wasm)?;
        let mut linker = Linker::<RunState>::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .map_err(|e| ScriptError::Load(format!("link wasi: {e}")))?;
        Gameplay::add_to_linker::<RunState, HasSelf<RunState>>(&mut linker, |s| s)
            .map_err(|e| ScriptError::Load(format!("link host: {e}")))?;
        let pre = linker
            .instantiate_pre(&component)
            .and_then(GameplayPre::new)
            .map_err(|e| ScriptError::Load(format!("instantiate: {e}")))?;
        Ok(Arc::new(WasmModule {
            engine: self.engine.clone(),
            pre,
            config: self.config,
            from_cache,
            calls_in: AtomicU64::new(0),
            calls_out: Arc::new(AtomicU64::new(0)),
        }))
    }

    /// The compiled component: deserialized from the DDC, or compiled (and then cached).
    fn component(&self, wasm: &[u8]) -> Result<(Component, bool), ScriptError> {
        let key = self.cache_key(wasm);
        if let Some(ddc) = &self.ddc
            && let Ok(Some(blob)) = ddc.get(&key)
            // SAFETY: the DDC is local build output, trusted like a compiled library. The key
            // binds the blob to these exact wasm bytes, this Wasmtime version and this engine's
            // compatibility hash, and Wasmtime checks the blob's header before using it; a blob
            // that fails those checks is recompiled below.
            && let Ok(component) = unsafe { Component::deserialize(&self.engine, &blob) }
        {
            return Ok((component, true));
        }
        let component = Component::new(&self.engine, wasm)
            .map_err(|e| ScriptError::Load(format!("compile: {e}")))?;
        if let Some(ddc) = &self.ddc {
            let blob = component
                .serialize()
                .map_err(|e| ScriptError::Cache(e.to_string()))?;
            ddc.put(&key, &blob)
                .map_err(|e| ScriptError::Cache(e.to_string()))?;
        }
        Ok((component, false))
    }
}

impl ScriptBackend for WasmBackend {
    fn name(&self) -> &'static str {
        "wasm"
    }

    fn load(&self, source: &ModuleSource) -> Result<Arc<dyn ScriptModule>, ScriptError> {
        let bytes = match source {
            ModuleSource::Bytes(bytes) => bytes.clone(),
            ModuleSource::File(path) => std::fs::read(path)
                .map_err(|e| ScriptError::Load(format!("{}: {e}", path.display())))?,
            ModuleSource::Linked(_) => {
                return Err(ScriptError::Unsupported {
                    backend: self.name(),
                    source_kind: format!("{source:?}"),
                });
            }
        };
        Ok(self.load_wasm(&bytes)?)
    }
}

/// One run's store data: virtual WASI and the log.
struct RunState {
    wasi: RunWasi,
    table: ResourceTable,
    logs: Vec<String>,
    calls_out: Arc<AtomicU64>,
}

impl WasiView for RunState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi.ctx,
            table: &mut self.table,
        }
    }
}

impl crate::bindings::concordia::game::host::Host for RunState {
    fn log(&mut self, message: String) {
        self.calls_out.fetch_add(1, Ordering::Relaxed);
        self.logs.push(message);
    }
}

impl crate::bindings::concordia::game::types::Host for RunState {}

/// A loaded component.
pub struct WasmModule {
    engine: Engine,
    pre: GameplayPre<RunState>,
    config: WasmConfig,
    from_cache: bool,
    calls_in: AtomicU64,
    calls_out: Arc<AtomicU64>,
}

impl WasmModule {
    /// True if the precompiled component came from the DDC.
    pub fn from_cache(&self) -> bool {
        self.from_cache
    }

    /// A fresh store for one call, fueled for `rows` rows.
    fn store(&self, time: f64, seed: u64, rows: usize) -> Result<Store<RunState>, ScriptFault> {
        let state = RunState {
            wasi: RunWasi::new(time, seed),
            table: ResourceTable::new(),
            logs: Vec::new(),
            calls_out: Arc::clone(&self.calls_out),
        };
        let mut store = Store::new(&self.engine, state);
        if let Some(fuel) = self.config.fuel {
            store
                .set_fuel(fuel.budget(rows))
                .map_err(|e| ScriptFault::Trap(format!("fuel: {e}")))?;
        }
        Ok(store)
    }

    /// The fault a failed call becomes (deterministic: the trap kind, never a backtrace).
    fn fault(&self, error: &wasmtime::Error) -> ScriptFault {
        match error.downcast_ref::<Trap>() {
            Some(Trap::OutOfFuel) => {
                ScriptFault::OutOfFuel("the run used its whole fuel budget".into())
            }
            Some(trap) => ScriptFault::Trap(trap.to_string()),
            None => ScriptFault::Trap(error.to_string()),
        }
    }
}

impl ScriptModule for WasmModule {
    fn describe(&self) -> Result<ModuleDesc, ScriptFault> {
        self.calls_in.fetch_add(1, Ordering::Relaxed);
        let mut store = self.store(0.0, 0, 0)?;
        let desc = self
            .pre
            .instantiate(&mut store)
            .and_then(|g| g.call_describe(&mut store))
            .map_err(|e| self.fault(&e))?;
        convert::module_desc(desc).map_err(ScriptFault::Protocol)
    }

    fn run(&self, processor: &str, ctx: &TickContext, batch: &Batch) -> RunReport {
        self.calls_in.fetch_add(1, Ordering::Relaxed);
        let rows = batch.ids.len() + batch.signals.iter().map(|s| s.targets.len()).sum::<usize>();
        let mut store = match self.store(ctx.time, ctx.seed, rows) {
            Ok(store) => store,
            Err(fault) => {
                return RunReport {
                    result: Err(fault),
                    logs: Vec::new(),
                };
            }
        };
        let called = self.pre.instantiate(&mut store).and_then(|g| {
            g.call_run(
                &mut store,
                processor,
                convert::tick(ctx),
                &convert::batch(batch),
            )
        });
        let result = match called {
            Ok(Ok(output)) => convert::run_output(output).map_err(ScriptFault::Protocol),
            Ok(Err(message)) => Err(ScriptFault::Returned(message)),
            Err(e) => Err(self.fault(&e)),
        };
        let state = store.into_data();
        let mut logs = state.logs;
        logs.extend(state.wasi.lines());
        RunReport { result, logs }
    }

    fn stats(&self) -> ModuleStats {
        ModuleStats {
            calls_in: self.calls_in.load(Ordering::Relaxed),
            calls_out: self.calls_out.load(Ordering::Relaxed),
        }
    }
}
