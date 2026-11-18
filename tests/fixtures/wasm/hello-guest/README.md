# hello-guest

WASM component fixture for the M-1 gate (`tests/gates/m_minus1_scaffold.rs`). Built by `just wasm` with plain `cargo build --target wasm32-wasip2` and `wit-bindgen`; no `cargo-component` needed.

**WASI imports.** A `std` guest built for `wasm32-wasip2` imports these even when unused: `wasi:io/{poll,error,streams}`, `wasi:clocks/{monotonic-clock,wall-clock}`, `wasi:cli/{stdout,stderr,stdin,environment,exit,terminal-*}` (all `@0.2.12`). The host links them with a closed `WasiCtx` (no stdio, env or filesystem). M2 decides whether gameplay guests go `no_std` to avoid them.

References: component-model.bytecodealliance.org, docs.wasmtime.dev, the wit-bindgen README.
