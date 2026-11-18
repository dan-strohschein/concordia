# ADR-0001: Rust for the engine core
- Status: Accepted
- Date: 2026-11-17

## Context
Most of this engine will be written with AI agents. We need memory safety, good performance and fast, precise feedback on generated code.

## Decision
The engine core and tools are written in Rust (stable channel, pinned in `rust-toolchain.toml`). C/C++ libraries (Jolt, Recast) are used through `-sys` crates with safe wrappers.

## Consequences
- The compiler and clippy act as strict reviewers of agent-written code.
- The pool of veteran engine developers is smaller than for C++; we compensate with documentation and small crates.
- FFI boundaries must be kept small and well tested.

## Alternatives considered
C++ (industry standard, less safe), Zig (young ecosystem, less LLM fluency).
