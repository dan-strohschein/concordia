# ADR-0021: WASM guest build path
- Status: Accepted; amended by ADR-0062 (M2 phase 2.6b): Wasmtime is pinned at `=51.0.0`, and gameplay guests use `std` with virtualized WASI
- Date: 2026-11-17

## Decision
- Guests build with plain `cargo build --target wasm32-wasip2` plus `wit-bindgen`. That emits a component directly, so `cargo-component` isn't required.
- Wasmtime and `wasmtime-wasi` are pinned exactly (`=50.0.2`), because the embedding API changes between majors.
- Guests that link `std` import WASI interfaces (clocks, stdio, environment). Hosts satisfy them with a closed `WasiCtx`: no inherited stdio, env or filesystem.

## Consequences
One fewer tool to install. Wasmtime upgrades are deliberate changes that must keep the M-1 gate green. M2 decides whether gameplay guests should be `no_std` to shed the WASI imports.
