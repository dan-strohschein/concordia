# concordia-rhi: agent guide

## Purpose
Backend-neutral GPU vocabulary: device capabilities and errors now, plus devices, resources, barriers and pipelines from phase 1.2b (ADR-0040). Nothing in it depends on a graphics API.

## Public entry points
- `DeviceCaps` (what a device can do) and `DeviceCaps::missing(Requirement)`
- `Backend`, `SampleCount` (1/2/4/8 only), `Requirement::M1`
- `RhiError`

## Invariants
- No graphics-API types or crates here; backends translate.
- `DeviceCaps::feature_sets` is sorted, so reports diff cleanly.
- `SampleCount` can only hold 1, 2, 4 or 8, including when deserialized.

## How to test
`cargo nextest run -p concordia-rhi`

## Common tasks
- **Add a capability:** add the field to `DeviceCaps`, fill it in every backend's probe, and add it to `missing_for_m1` if M1 needs it, with a test in `tests/caps.rs`.

## Don'ts
- Don't add backend-specific enums; express them as capabilities or `feature_sets` names.

## References
- Apple *Metal Feature Set Tables*; Vulkan specification (capability vocabulary).

## Status
M1 phase 1.2a: capabilities and probe types.

## Engineering rules
These apply to every change, by humans and agents alike.
1. **TDD on every phase.** Write the tests first, see them fail, implement, then refactor.
2. **SOLID.** Depend on abstractions at crate boundaries; extend through traits rather than by editing callers.
3. **No duplicate functionality.** Before writing any implementation, search for existing functionality (grep, `cargo doc`, crate `AGENTS.md` files). Extend or refactor what exists; never write a parallel version.
4. **Single responsibility at every level:** function, type, module and crate.
5. **Small functions** that do one thing.
6. **Validate parameters.** Every function checks its inputs:
   - Rust: encode invariants in types (newtypes, enums, `NonZero*`); validate at boundaries and return `Result` with a descriptive error; use `debug_assert!` for internal invariants. Public APIs never panic on bad input.
   - TypeScript: strict types, plus runtime validation at API and IPC boundaries.
7. **Short comments.** One-line doc comments on public items; no essays, and no comments that restate the code.
8. **Idiomatic code.**
   - Rust: follow the Rust API Guidelines and keep it clippy-clean; `thiserror` in libraries, `anyhow` only in binaries and tests.
   - Editor UI: idiomatic React and TypeScript, with function components, hooks, strict mode, ESLint and Prettier.
