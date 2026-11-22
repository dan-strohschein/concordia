# concordia-nav: agent guide

## Purpose
Navigation (M2 phase 2.5, ADR-0077): navmeshes built with Recast from the static colliders inside a `NavMeshVolume`, marked by `NavArea` and `NavAreaGrid`, queried with Detour (nearest point, path, surface raycast), cached in the DDC, served as `nav.build` / `nav.path`, and drawn as the `navmesh` debug layer; agents (`NavAgent`) that walk character controllers along paths (2.5b.1), drawn as the `nav_paths` layer. `#![forbid(unsafe_code)]`: all FFI is `recast-sys`'.

## Public entry points
- Components: `NavMeshVolume` (box, agent size, `cell_size`, `cell_height`, `tile_size`, `edge_max_error`), `NavArea` / `NavAreaKind` (`Blocked`, `Cost { cost }`), `NavAreaGrid` (`cell_size`, `rows` of `.` `#` `o`); `register_types`
- `NavInput::gather(&World, volume)` (settings, triangles, marks, costs in entity order; `key()` for the cache), `NavSettings`
- `build_tiles(&NavInput, threads)` → `NavMeshData` (`to_bytes`, `from_bytes`: the `CNAV` v1 format)
- `NavMesh` (`from_data`, `data`, `stats` → `NavStats`, `polygons` → `NavPoly` with islands, `nearest_point` → `NavPoint`, `island_at`, `path` → `NavPath`, `raycast` → `NavRay`)
- `NavCache` (`new(Ddc)`, `load_or_build` → `Loaded`), `NavError`
- `NavModule` (`[nav]`; `with_plans_per_tick`): methods `NavBuild` (`nav.build`) and `NavPathMethod` (`nav.path`) with their params and results; the `NavMeshes` play-world resource (`new`, `get`, `iter`); the `NAVMESH_LAYER` and `NAV_PATHS_LAYER` debug layers
- Click-to-move (2.5b.2a, ADR-0078): `ClickToMove` and `add_click_processor` (`nav.click`, in `PrePhysics` before `nav.plan`)
- Agents (2.5b.1): `NavAgent` (gameplay's request; `validate`), `NavAgentState` and `NavStatus` (the engine's, hashed), the signals `NavArrived` and `NavPathFailed` (`NavFailure`); `add_agent_processors(schedule, AgentConfig)` (`nav.plan`, `nav.follow` in `PrePhysics`) and `insert_agent_states` (play start)

## Invariants
- Sources: `concordia_physics::static_solids` (static, non-sensor bodies physics accepts), tessellated from fixed tables (`tessellate.rs`, no trigonometry) at `translation + rotation × (local + offset)`; scale never applies, as in physics. Triangles are kept if they touch the volume.
- Marks: `NavArea`s by entity id, then `NavAreaGrid`s by entity id; upper bounds exclusive (half-open cells); blocked becomes null before erosion (in the shim). Distinct costs get area ids 2..=62 in ascending order; more than 61 is `TooManyCosts`.
- The input, its key and the built bytes never depend on entity insertion order or thread count: everything is gathered by entity id, tiles build independently and are kept in (z, x) order (`navmesh_build_is_deterministic_byte_identical`).
- Tiles get the triangles that touch them plus Recast's border, each with its own copy of its vertices, so no tile validates the whole level.
- The module memoizes per (world, volume) and key, then reads the DDC; `nav.build` / `nav.path` re-gather every call, so they follow edits. Play worlds get `NavMeshes` at play start (not hashed: derived from hashed state); a volume that fails is logged and left out.
- Queries borrow pooled Detour query objects (`Mutex<Vec<DtQuery>>`), so a `NavMesh` is `Sync` and gives the same answer on any thread.
- Agents: `NavAgent` is written only by gameplay and `NavAgentState` only by `nav.plan` / `nav.follow`; an entity's `NavAgent` owns its `CharacterMotion` (nav writes it every tick). Agents need a `CharacterController` and use its rule: the position is `Transform.translation`.
- `nav.plan` takes at most `plans_per_tick` agents a tick in `(waiting_since, id)` order (hashed state only), so results never depend on threads; `nav.follow` runs in parallel pieces of 256 agents and raises signals through command buffers. Neither opts into LOD (ADR-0077's update).
- Distances for corners, arrival, target moves and off-path checks are sideways, with a vertical tolerance of half the agent's height. Replans: a new target (moved past `target_desired_distance`), or off the current segment by more than `path_max_distance`; a failed target is retried only when it moves. `NavPathFailed` is raised once per target; unreachable targets walk to the closest point and arrive with `partial`.
- The last stretch is slowed to `gap / dt`, so no tick carries an agent past its path's end; a `Waiting` agent walks its old path but never arrives on it.
- Size checks (N3) warn once per (agent, volume) and never refuse; the controller collides.
- `nav.click` reads `ActionStates`, `PhysicsQueries` and `NavMeshes`. When the click fires (`just_pressed`, or `pressed` with `hold`), it takes the ray's nearest physics hit, snaps it onto the agent's navmesh (`nearest_point`) and writes `NavAgent.target`; a miss changes nothing. Bad action names or kinds, layers or reach warn once per agent and the component is ignored. It runs after input sampling by registration order (input's module comes first).
- Recast rules the tests rely on: the walkable surface of a floor is up to one `cell_height` above it (a span is at least one cell tall); a face exactly on a cell boundary belongs to one side only (fixtures put floor edges inside a cell); erosion keeps the cell diagonal to a blocked corner; contours are exact to the cell when `edge_max_error` < 0.7. Detour's A* runs over portal midpoints, so path shapes are pinned on single-tile fixtures.

## How to test
`cargo nextest run -p concordia-nav` (`tests/build.rs`, `tests/query.rs`, `tests/api.rs`; `tests/agents.rs` on a rig without physics, where a test system moves each `Transform` by its velocity so ticks are hand-computable; `tests/agents_physics.rs` with Jolt characters; every expected value is derived in the files' headers). The guest test is `concordia-app`'s `tests/nav_guest.rs` (needs `just wasm`). The GPU golden of the debug layer is `concordia-app`'s `tests/nav.rs`; the MCP tools are `concordia-mcp`'s `tests/nav_tools.rs`.

## Common tasks
- **A new mark:** add a component, gather it into `BoxMark`s in `gather.rs` (in entity order, half-open), and test it against a hand-computed area or path.
- **A new query:** add it to `DtQuery` in `recast-sys` first (with a smoke test), then wrap it on `NavMesh`.
- **A new agent behavior:** decide in `plan.rs` (who needs a plan) or `follow.rs` (one tick of walking), both pure functions; keep processor glue in `agents.rs`. Test it on `tests/agents.rs`' rig with hand-computed ticks.

## Don'ts
- Don't build from render meshes (ADR-0077: what characters can't stand on isn't walkable).
- Don't store navmesh bytes anywhere but the DDC.
- Don't copy the reference port's names or grid semantics here; M5 maps them onto `NavAreaGrid`.

## References
- Recast & Detour documentation and source (zlib) and Mikko Mononen's talks and posts (implementation); Godot's navigation docs and source (MIT) for semantics only (agent radius and height, target and path desired distances, path max distance, region and agent roles; `NavigationAgent3D`); Greg Snook, "Simplified 3D Movement and Pathfinding Using Navigation Meshes", *Game Programming Gems* (2000). No Unreal or other non-open source was consulted.

## Status
M2 phase 2.5a (navmesh build, queries, cache, `nav.*`, debug layer); 2.5b.1 (agents, signals, `nav_paths`); 2.5b.2a (click-to-move). Windowed play is 2.5b.2b; crowds (`concordia-terra-crowd`) 2.5c; traffic 2.5d.

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
