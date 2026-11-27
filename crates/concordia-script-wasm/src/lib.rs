//! The WASM script backend (ADR-0062): gameplay modules as WebAssembly components of the WIT
//! world `concordia:game/gameplay`, on Wasmtime.
//!
//! - **Ahead of time:** components are compiled once and cached in the DDC, keyed by the wasm
//!   hash, the Wasmtime version and the engine's compatibility hash; nothing is compiled or
//!   interpreted while the game runs.
//! - **Deterministic:** Cranelift with NaN canonicalization, no threads, deterministic relaxed
//!   SIMD, fuel per run (no epochs), a memory cap, and WASI virtualized to the simulation.
//! - **Stateless:** every run instantiates a fresh instance from a pre-linked `InstancePre`
//!   (pooling allocator), so guest statics never outlive a run.

mod backend;
mod config;
mod convert;
mod wasi;

/// Host bindings of the WIT world `concordia:game/gameplay`.
#[allow(missing_docs, clippy::all)]
mod bindings {
    wasmtime::component::bindgen!({ world: "gameplay", path: "../../wit/concordia-game" });
}

pub use backend::{WASMTIME_VERSION, WasmBackend, WasmModule, engine_config};
pub use config::{Fuel, WasmConfig};
pub use wasi::sim_nanos;
