# concordia-reflect: agent guide

## Purpose
Every engine type describes itself: wire shape, doc comments, defaults and ranges. From that one description the crate builds the type registry, JSON Schema (2020-12), agent-friendly JSON validation and canonical hashes. Values travel through serde; reflection adds the shape and docs.

## Public entry points
- `Reflect` (trait + `#[derive(Reflect)]`), `TypeInfo`, `Shape`, `FieldInfo`, `VariantInfo`, `TypeRef`
- `TypeRegistry` (`register`, `get`, `get_by_name`, `from_json`, `schema_for`, `schema_index`) and `TypeRegistration` (type-data slots, `deserialize`)
- `DynReflect` (`to_json`, `clone_dyn`, `eq_dyn`, downcasting)
- `schema_for(&TypeInfo)`, `canonical_hash(&dyn DynReflect, &mut blake3::Hasher)`
- `ReflectError` (JSON Pointer errors), `RegistryError`

## Invariants
- **serde shape == reflected shape.** The derive accepts only these serde attributes: container `transparent`, `deny_unknown_fields`, `default`, `bound`, `crate`, `expecting`; field `default`, `skip`, `skip_serializing_if`, `bound`; none on variants. Everything else (`rename`, `rename_all`, `tag`, `untagged`, `content`, `flatten`, `with`, `alias`, `other`, ...) is a compile error. `schema_matches_serde` proves the agreement.
- Every public reflected type and field has a doc comment: it becomes agent-facing schema text.
- Stable names (`TypeInfo.name`) are unique per registry; `rust_path` is diagnostics only and never stored.
- Schemas are a pure function of `TypeInfo`; structs and enums go in `$defs`, everything else is inlined.
- `from_json` validates shape and declared ranges before serde runs; serde deserialization from files (`deserialize`) does not enforce ranges.
- The canonical hash encoding is frozen by `canonical_hash_stable_snapshot`; changing it changes every world hash.
- M0 does not support generic user types.

## How to test
`cargo nextest run -p concordia-reflect` (includes trybuild compile-fail cases) and `cargo test --doc -p concordia-reflect`. Review snapshot changes with `cargo insta review`.

## Common tasks
- **Make a type reflectable:** derive `Clone, PartialEq, Serialize, Deserialize, Reflect`; doc-comment the type, every field and every variant; register it with `TypeRegistry::register::<T>()` (dependencies register automatically).
- **Add a default or range:** `#[reflect(default = 100.0, range(min = 0.0, max = 1000.0))]` on the field. Range bounds are numeric literals with `min <= max`.
- **Rename a type's stable name:** `#[reflect(name = "game.Health")]` (never `#[serde(rename)]`).
- **Support a new foreign type:** add an impl under `src/impls/` and a case to `tests/schema_matches_serde.rs`.
- **Add a compile-fail case:** add `tests/ui/<case>.rs`, run the tests, review `wip/<case>.stderr`, then move it next to the case.

## Don'ts
- Don't hand-write JSON Schemas anywhere in the engine; generate them from `TypeInfo`.
- Don't add serde attributes to the allow-list without a proptest showing serde's JSON still matches the schema.
- Don't change `src/hash.rs` encoding without a deliberate snapshot update.

## References
- JSON Schema draft 2020-12 specification; RFC 6901 (JSON Pointer).
- serde data model documentation; `erased-serde` documentation.
- The Rust Reference on procedural macros and `#[doc]` attributes; `syn` 3 documentation.
- Bevy reflection *documentation* read for the problem space only; no code copied (design is our own).

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
