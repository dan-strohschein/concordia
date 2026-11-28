# concordia-destruction: agent guide

## Purpose
Destruction at runtime: a fractured breakable (`concordia-fracture`) is one Jolt body in play until impacts strain its bonds past their strength. Then it splits into its connected parts, deterministically. Play only; the edit world is never touched (ADR-0032).

## Public entry points
- Components: `Breakable { recipe, anchors, impact_radius }` (authored, with a `Transform` and a `RigidBody`), `Fragment { recipe, anchors, impact_radius, pieces, damage, broken }` (runtime)
- `destruction_system(Arc<dyn BreakableSource>, DestructionSettings { breaks_per_tick, max_debris, debris_lifetime, min_debris_volume })`: the `destruction.break` system for `PostPhysics`
- `BreakableSource` (`load(recipe) -> Arc<Loaded { fracture, structure }>`), `MemorySource` (`with(recipe, loaded)`)
- `Field { shape: FieldShape { Sphere, Box, Plane, Everywhere }, falloff: Falloff { None, Linear, InverseSquare }, effect: FieldEffect { Strain, Crumble, Anchor, Kill, Force, Impulse, Sleep, Disable }, magnitude, duration }` (an ordinary component, placed by its `Transform`), `FieldAge` (play-only), `Field::weight`
- Methods (play only): `destruction.crumble {entity}` and `destruction.apply_strain {center, radius, amount}` → `{bonds}`
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
- Fields act at one point each tick: after setup, before impacts, fields in id order over fragments in id order.
  - `Strain` adds magnitude × weight × dt to each bond inside it.
  - `Crumble` makes every bond of a fragment due once any of its pieces is inside.
  - `Anchor` adds the covered cells to the fragment's anchors and makes it static.
  - `Kill` despawns debris (fragments without a `Breakable`) whose centre of mass is inside.

  - `Force` adds magnitude × weight × dt as an `Impulse` at the fragment's centre of mass, and `Impulse` adds magnitude × weight once, on the field's first tick. Both push away from the field's centre, or along its normal for a plane.
  - `Sleep` sends `Sleep`.
  - `Disable` holds `PhysicsDisabled` while the fragment is covered.

  Every field ages in `FieldAge`; one with a duration leaves the play world when it ends.
- Debris is every fragment without a `Breakable` (parts split off). After breaking, each tick:
  - it ages in `DebrisAge`;
  - past `debris_lifetime` (when that is set) it is removed;
  - below `min_debris_volume` it loses its `Collider`, so it has no body;
  - past `max_debris`, the oldest go first (ties: lower id).
- Fields and the methods only add damage; bonds break in `break_bonds`, under the budget.
- Cluster levels and their damage thresholds aren't used at runtime yet.

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
M8 8.3: breakables, strain, the break budget, splits with momentum, anchors, the `[destruction]` module. 8.4a: fields (strain, crumble, anchor, kill), `destruction.crumble` and `destruction.apply_strain`. 8.5a: the debris budget, lifetime and small-debris collision. 8.4b: force, impulse, sleep and disable through `concordia-physics`'s `Impulse`, `Sleep` and `PhysicsDisabled`.

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
