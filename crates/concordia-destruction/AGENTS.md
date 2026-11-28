# concordia-destruction: agent guide

## Purpose
Destruction at runtime: a fractured breakable (`concordia-fracture`) is one Jolt body in play until impacts strain its bonds past their strength. Then it splits into its connected parts, deterministically. Play only; the edit world is never touched (ADR-0032).

## Public entry points
- Components: `Breakable { recipe, anchors, impact_radius }` (authored, with a `Transform` and a `RigidBody`), `Fragment { recipe, anchors, impact_radius, pieces, damage, broken }` (runtime)
- `destruction_system(Arc<dyn BreakableSource>, DestructionSettings { breaks_per_tick })`: the `destruction.break` system for `PostPhysics`
- `BreakableSource` (`load(recipe) -> Arc<Loaded { fracture, structure }>`), `MemorySource` (`with(recipe, loaded)`)
- `DestructionModule` (`[destruction] breaks_per_tick`): registers the components; at play start loads every recipe the play world's breakables name (`concordia_fracture::load`, then `structure`), and schedules the system

## Invariants
- A breakable becomes a fragment on its first play tick: a `ColliderShape::Compound` of its pieces' hulls (mesh space = body space), `RigidBody.mass` from the structure, `ContactReports { min_strength: 0 }`; static if it holds an anchored cell.
- Strain: each `ContactImpact` on a fragment adds strength × (1 − d / `impact_radius`) to each of its unbroken bonds, where d is the distance from the impact to the midpoint of the bond's two piece centres (mesh space).
- Breaks: bonds whose damage reaches their strength break, sorted by fragment id then bond, at most `breaks_per_tick` per tick; the rest stay due and break on later ticks.
- Splits: union-find over the unbroken internal bonds; parts are ordered by their lowest piece. Each part:
  - gets an id derived from its parent and its lowest piece (UUIDv5), so ids don't depend on thread count or order;
  - keeps the parent's transform;
  - moves as it did inside the whole (v + ω × r at its centre of mass, same ω), so momentum is kept;
  - is static if it holds an anchor, otherwise dynamic.
- Fragment state is a component, so it is hashed, saved and restored with the play world. Nothing iterates a hash map.
- Cluster levels and their damage thresholds aren't used at runtime yet (fields, 8.4).

## How to test
`cargo nextest run -p concordia-destruction`

## Common tasks
- **A new strain source** (fields, 8.4): add damage to `Fragment::damage` before `break_bonds` runs; never break bonds directly.

## Don'ts
- Don't run destruction in the edit world or from a method; it is a play system only.
- Don't spawn parts with fresh random ids.

## References
- Müller, Chentanez and Kim, "Real Time Dynamic Fracture with Volumetric Approximate Convex Decompositions" (SIGGRAPH 2013); Jolt Physics documentation (compound shapes, contact listeners).

## Status
M8 8.3: breakables, strain, the break budget, splits with momentum, anchors, the `[destruction]` module.

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
