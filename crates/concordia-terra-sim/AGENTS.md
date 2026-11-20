# concordia-terra-sim: agent guide

## Purpose
Terra's templates, traits and spawners (M2 phase 2.0c, ADR-0070), on top of `concordia-terra`'s storage (ADR-0069). A **trait** is a Rust type with reflected parameters that adds components (dense, shared or tag) and names the processors its entities need. A **template** composes traits and plain component values; project templates live in `templates/**/*.template.ron`, module built-ins in code. A **spawner** is an edit-world component that places a template's entities with a seeded distribution when play starts.

## Public entry points
- `EntityTrait` (`build`, `requires`), `register_trait::<T>(&mut TypeRegistry)`, `ReflectTrait`, `TemplateBuilder` (`add`, `add_shared`), `trait_names`, `trait_schema`
- `Template` (`new`, `from_doc`, `to_doc`, `compile`) → `CompiledTemplate` (`component_names`, `requires`, `missing_processors`, `components_json`, `spawn`, `spawn_batch`)
- `TemplateCatalog::load(project, builtins, registry)` (`get`, `iter`), `TEMPLATE_DIR`
- `Spawner` (`points`, `MAX_COUNT`), `Distribution` (`Grid`, `Box`, `Disc`), `expand_spawners`
- `Movement` (the built-in trait), `register` (registers `Spawner` and `Movement`), `TemplateError`
- The text format itself is `concordia_scene::{read_template, write_template, TemplateDoc}`.

## Invariants
- Traits apply in name order, then component values. Two traits adding one component is `TraitConflict` naming both; a component value replaces what a trait added.
- Every trait parameter and component value is validated through the `TypeRegistry` (ranges included); unknown names come with a did-you-mean.
- `add_shared` is a storage hint: a world that already stores the type densely keeps it dense. Hashes are identical either way (ADR-0069).
- Spawner points use only `SimRng` and arithmetic (no trigonometry), so they're the same on every platform; ids come from the spawner's seed and its entity id, so every play session spawns the same entities.
- Template files are named by file stem and must be unique across built-ins and files (`Duplicate`).
- No `unsafe`.

## How to test
`cargo nextest run -p concordia-terra-sim` (templates, traits, spawners, catalog; includes a 100k batch spawn). API and MCP coverage: `concordia-api` `tests/terra.rs`, `concordia-mcp` `tests/terra_tools.rs`.

## Common tasks
- **Add a trait:** derive `Clone, PartialEq, Serialize, Deserialize, Reflect` (with `#[serde(deny_unknown_fields)]` and `range` attributes for parameters), implement `EntityTrait` (add components with `template.add`/`add_shared`, return processor names from `requires`), and call `register_trait::<T>(registry)` in your module's `register_types`.
- **Add a distribution:** extend `Distribution` and `Spawner::points`; keep it seed-only and trig-free, and test same-seed equality and bounds.

## Don'ts
- Don't read wall-clock time or OS randomness when placing entities.
- Don't journal batch spawns: `terra.spawn_batch` acts on the play world; levels use `Spawner`.
- Don't let two traits silently share a component; conflicts are errors by design.

## References
- Unreal Engine's public Mass documentation on entity configs and traits (traits add fragments and require processors), spawners and spawn data generators; concepts only (clean room, ADR-0067). No Unreal source.
- Bevy documentation on bundles and required components; flecs documentation on prefabs; concepts only, no code copied.

## Status
M2 phase 2.0c complete (2026-11-20).

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
