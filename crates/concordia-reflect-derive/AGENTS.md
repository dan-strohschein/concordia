# concordia-reflect-derive: agent guide

## Purpose
The `#[derive(Reflect)]` procedural macro. It turns a struct or enum's shape, doc comments and `#[reflect]` attributes into a `concordia_reflect::TypeInfo`, and rejects serde attributes that would make serde's output disagree with that shape.

## Public entry points
- `#[derive(Reflect)]` (re-exported as `concordia_reflect::Reflect`; use it from there).

## Invariants
- Supported: named, tuple, newtype and unit structs; enums with unit, newtype, tuple and struct variants.
- Rejected with a clear error: generics, unions, disallowed serde attributes (the list lives in `src/attrs.rs` and `concordia-reflect/AGENTS.md`), `range` with `min > max` or non-literal bounds, `#[reflect]` on variants.
- `#[serde(skip)]` fields are omitted; `default`, `skip_serializing_if` and `Option<..>` fields are optional.
- Generated code only names `::concordia_reflect` and `::std` paths.

## How to test
Through `concordia-reflect`: `cargo nextest run -p concordia-reflect` (derive tests plus `tests/ui` trybuild cases).

## Common tasks
- **Add an attribute:** parse it in `src/attrs.rs`, emit it in `src/fields.rs` or `src/expand_*.rs`, add a derive test and a trybuild case for its misuse.
- **Add a compile-fail case:** see `concordia-reflect/AGENTS.md`.

## Don'ts
- Don't emit code that depends on crates the user may not have (go through `concordia_reflect::__private`).
- Don't silently ignore an unknown attribute; return a `syn::Error` with a fix.

## References
- The Rust Reference (procedural macros); `syn` 3, `quote` and `proc-macro2` documentation.

## Status
M0 phase 0.2 complete.

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
