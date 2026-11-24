# concordia-assert: agent guide

## Purpose
The playtest assertion language as text and as a tree: parse with column spans, print canonically, type-check against the reflection registry (names, field paths, units, quantifiers) and build from Rust. It doesn't evaluate: `concordia-replay` runs assertions over a live, replayed or scripted world (M3 phase 3.2b).

## Public entry points
- `parse(&str) -> Result<Assertion, ParseError>` (columns 1-based, `expected` lists the accepted tokens)
- `Assertion`, `Temporal`, `Dur`, `Pred`, `Cmp`, `Expr`, `Quant`, `Sel`, `Unit`; `Display` prints the canonical text
- `check(&Assertion, &TypeRegistry, rate_hz) -> Result<Assertion, TypeError>` (durations become ticks), `Dim`
- `builder`: `within`, `always`, `always_for`, `never`, `never_for`, `eventually`, `at`, `after`; `tag`, `name`, `id`, `with`; `distance`, `count`, `exists`, `event`, `field`, `any`, `all`; `600.ticks()`, `2.0.m()`, `3.0.s()`, `90.0.deg()`, `1.5.secs()`; `Expr::{lt, le, gt, ge, equals, differs}`, `Pred::{and, or}`, `!pred`

## Invariants
- `parse(a.to_string()) == a` for every tree (`prop_pretty_print_then_parse_is_identity`); `and` binds tighter than `or`, `not` tighter than both.
- Field paths are checked through each component's JSON Schema (`TypeRegistry::schema_for`), so script-defined components work; vectors index as `x`, `y`, `z`, `w` (bounded by `maxItems`) or digits.
- Units: a plain number (fields, counts) compares with anything; Length, Time and Angle only with themselves; booleans stand alone. `tag(..)` and `with(..)` may match several entities, so a field read needs `any` or `all`.
- Seconds become whole ticks at the project rate, or the check fails (no silent rounding).
- No `unsafe`; public functions never panic.

## How to test
`cargo nextest run -p concordia-assert`

## Common tasks
- **Add an expression:** a variant in `ast::Expr`, its text in `print.rs`, its parse in `parse.rs` (and the `expected` list), its `Dim` in `check.rs`, a builder function, and the proptest strategy; evaluation goes in `concordia-replay`.

## Don'ts
- Don't add a parser generator: the hand-written parser keeps the messages ours.
- Don't evaluate here; this crate knows no world.

## References
- Runtime verification of bounded temporal properties (bounded LTL monitoring literature, e.g. Bauer, Leucker and Schallhart, "Runtime Verification for LTL and TLTL", ACM TOSEM 2011).
- Dimensional analysis as type checking (units of measure in type systems, e.g. Kennedy, "Types for Units-of-Measure", 2009).
- Levenshtein edit distance for suggestions.

## Status
M3 phase 3.2a (ADR-0084): grammar v1, parser, printer, type checker, builder.

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
