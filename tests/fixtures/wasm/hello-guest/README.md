# hello-guest

WASM component fixture for the M-1 gate (`tests/gates/m_minus1_scaffold.rs`). Built by `just wasm` with plain `cargo build --target wasm32-wasip2` and `wit-bindgen`; no `cargo-component` needed.

**WASI imports.** A `std` guest built for `wasm32-wasip2` imports these even when unused: `wasi:io/{poll,error,streams}`, `wasi:clocks/{monotonic-clock,wall-clock}`, `wasi:cli/{stdout,stderr,stdin,environment,exit,terminal-*}` (all `@0.2.12`). The host links them with a closed `WasiCtx` (no stdio, env or filesystem). Gameplay guests (M2 phase 2.6b, ADR-0062) keep `std`; the script host virtualizes these imports to the simulation.

References: component-model.bytecodealliance.org, docs.wasmtime.dev, the wit-bindgen README.
