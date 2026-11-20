# concordia-script-wasm

The WASM script backend (ADR-0062): gameplay modules as WebAssembly components of `concordia:game/gameplay`, on Wasmtime (pinned `=51.0.0`). Precompiled ahead of time and cached in the DDC; deterministic (NaN canonicalization, no threads, deterministic relaxed SIMD, fuel per run, a memory cap, WASI virtualized to the simulation); a fresh instance for every run.

See `AGENTS.md` for invariants, tests and references, and `benches/BASELINE.md` for the guard costs.
