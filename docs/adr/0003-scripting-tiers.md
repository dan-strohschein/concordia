# ADR-0003: Three gameplay scripting tiers
- Status: Accepted
- Date: 2026-11-17

## Decision
1. **Rust native** game crates, with hot reload (comparable to Unreal's C++).
2. **WASM components** on Wasmtime. The gameplay API is defined once in WIT (`wit/`, package `concordia:game`) and generates bindings for Rust, C/C++, Zig, AssemblyScript, Swift and Go. The API is batch/query-oriented to keep the cost of crossing the host/guest boundary low. Sandboxed, so mods and agent code can't crash the engine.
3. **Visual graphs stored as text** (`.graph.ron`), comparable to Blueprints, compiled to fast code. Arrives in M4.

## Consequences
WASM computation runs at roughly 1.1–1.5× native time; frequent fine-grained calls across the boundary are the main cost. WASM threads and SIMD (beyond 128-bit) are limited, so hot loops stay native.
