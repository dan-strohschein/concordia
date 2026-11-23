# concordia-terra-crowd: agent guide

## Purpose
Crowds (M2 phase 2.5c, ADR-0077's 2.5c update): `NavAgent`s with a `CrowdAgent`, steered by per-agent Terra processors that rewrite DetourCrowd's `dtCrowd::update` (zlib source) over Detour queries (N1). Two bodies (N5): a crowd agent without a `CharacterController` is a **surface agent** (the crowd moves its `Transform` along the navmesh); with one it is a **physics agent** (the crowd's velocity goes to `CharacterMotion`). `#![forbid(unsafe_code)]`: all FFI is `recast-sys`'. **Lane traffic** (M2 phase 2.5d, ADR-0079) lives in `src/traffic/`: lanes and junctions as entities (N6), vehicles on the Intelligent Driver Model, junction grants, and braking for people from `CrowdGrid` (N7).

## Public entry points
- `CrowdState` (hashed; written only by `crowd.*`): `surface`, `velocity`, `desired_velocity`, `desired_speed`, `new_velocity`, `candidate`, `correction`, `corners` (≤ 4), `at_end`, `neighbours` (≤ 6, nearest first), `boundary_center` and `walls` (≤ 8 `Wall { from, to }`, nearest first; 2.5c.2); `register_types`
- `CrowdGrid` / `GridEntry`: the per-tick snapshot resource (`new`, `get`, `slot`, `entries`, `around`; its data is shared, so cloning it is cheap)
- `add_crowd_processors(schedule)` (`GRID`, `CORRIDOR`, `NEIGHBOURS`, `STEER`, `AVOID`, `INTEGRATE`, `COLLIDE`, `MOVE`, in `PrePhysics` after `nav.plan`) and `insert_crowd_states` (play start; also inserts the `CrowdGrid`)
- `CrowdModule` ("crowd"; `concordia-app` adds it with `[nav]`), the `CROWD_LAYER` debug layer
- The `Crowd` trait (`agent: NavAgent`, `crowd: CrowdAgent`; adds `Transform`, `NavAgent`, `CrowdAgent`; requires `crowd.move`)
- `CrowdAgent` and `CrowdAvoidance` (the request) live in `concordia-nav`, which must know which agents it leaves to the crowd.
- Traffic (2.5d):
  - **Components and signals:** `Lane`, `Junction`, `Vehicle` (`check`), `VehicleState` (hashed), `VehicleStatus`, `JunctionState` (hashed, with `Grant`s), `TrafficProblems`, `VehicleArrived` and `VehicleRouteFailed`.
  - **Resources:** `TrafficGraph` (`build` from `LaneInput`s and junctions; `lane` → `LaneInfo` with `at` and `project` → `Projection`; `junction` → `JunctionInfo`; `conflict`; `problems` → `TrafficProblem`) and `TrafficSnapshot` / `VehicleSnap`.
  - **Processors:** `insert_traffic` (play start) and `add_traffic_processors` (`PLACE`, `SNAPSHOT`, `JUNCTIONS`, `DRIVE`, after `crowd.move`).
  - **Template and debug:** the `Traffic` trait (`vehicle: Vehicle`; adds `Transform` and `Vehicle`; requires `traffic.drive`) and the `LANES_LAYER` debug layer.

## Invariants
- **Planning is nav's:** `nav.plan` plans crowd agents in the shared `(waiting_since, id)` queue and keeps their polygon corridor in `NavAgentState.corridor`; crowd agents need no controller, never replan for being off their path, and keep their first polygon (and volume) while idle. `nav.follow` and `nav_paths` skip them (`Without<CrowdAgent>`).
- **One writer per agent, snapshots for the rest:** every per-agent step writes only its own components. Others are read from `CrowdGrid` (built once a tick, in id order, from hashed state: last tick's positions and velocities, whatever an agent's LOD) or, in `crowd.collide`, from the previous Jacobi iteration's candidates. Parallel pieces are 256 rows; neighbours are sorted by (distance², id); signals go through deferred buffers joined in piece order. So the world hash is identical at any thread count (`crowd_is_deterministic_across_runs_and_threads`).
- **The chain** (DetourCrowd's order): grid → corridor (physics agents' corridors follow their bodies by `moveAlongSurface`; `findCorners`, at most 4, dropping those within 0.01 m; the visibility shortcut toward the second corner, stretched to `path_optimization_range`, kept when the ray reaches it and crosses more than one polygon) → neighbours (everything within `collision_query_range` sideways that overlaps vertically, nearest 6) → steer (smooth or straight direction; speed scaled by min(d, 2r) / 2r at the end; separation weighted 1 − (d / range)², averaged, clamped by DetourCrowd's squared ratio) → avoid (below) → integrate (velocity toward the chosen one by at most `max_acceleration × dt`; a step never passes the end) → collide (4 Jacobi iterations; each agent moves (p / 2) × 0.7 per contact, averaged; a surface agent takes the whole 0.7 p from a solid; physics agents count only other physics crowd agents and turn their push into velocity) → move (surface agents: `moveAlongSurface` from the corridor's first polygon, merged with `dtMergeCorridorStartMoved`, height from `getPolyHeight`; physics agents: `CharacterMotion = velocity + correction / dt`; arrival within `target_desired_distance` of the end raises `NavArrived`).
- **Characters are solid:** every `CharacterController` (the player, physics agents, plain nav characters) is in the grid; surface agents avoid and yield to them and never push them. Physics never sees surface agents.
- A lost corridor (a surface move that fails) asks `nav.plan` for a new path by setting `Waiting`.
- **Avoidance** (`avoid.rs`, DetourCrowd's `sampleVelocityAdaptive`, `processSample`, `sweepCircleCircle` and `isectRaySeg`, in its order of operations).
  - The obstacles are the neighbours (from `CrowdGrid`: last tick's position, velocity and desired velocity) and the walls the agent faces (`dtTriArea2D ≥ 0`).
  - The pattern: zero, then `rings` rings of `divs`, each rotated by π/divs. It runs `depth` passes, each halving the radius, and skips candidates past the desired speed + 0.001. Ties keep the first candidate.
  - Trigonometry comes from the fixed `tables.rs`.
  - An even `divs` fills the slot DetourCrowd leaves stale.
  - With `avoidance: None`, or no desired speed, `new_velocity` is the desired velocity.
- **The local boundary** (`boundary.rs`, `dtLocalBoundary`): gathered in `crowd.avoid` for walking agents with avoidance, when there is none yet or the agent is more than `collision_query_range / 4` from where it was gathered.
  - It takes the polygons within range around the corridor's first (≤ 16), their walls (≤ 18 each) within range, and the nearest 8 by `addSegment`'s insertion (an equal distance goes before; after the last it is appended while there is room).
  - There is no validity re-check: navmeshes don't change at runtime.
- **Simulation LOD:** `crowd.corridor`, `crowd.steer` and `crowd.avoid` are `.lod()`, so a far agent re-picks its corners, desired velocity, walls and chosen velocity only when due, and holds them in between. Everything else, arrival included, is full-rate. Without `LodProfile`s every agent is due every tick.
- The crowd and the Detour it calls build at `opt-level = 2` in dev (the workspace `Cargo.toml`), so the 1,000- and 10,000-agent tests stay fast.

- **Traffic** (`src/traffic/`, ADR-0079):
  - **The graph** is built at play start from `Lane` and `Junction` entities. Lane points live in the entity's space (translation and rotation, not scale). Bad lanes are logged, drawn red, put on their entity as `TrafficProblems`, and left out.
  - **Conflicts:** connectors cross sideways away from a shared end, or merge into a common lane.
  - **Routes:** Dijkstra by travel time, ties by id. Wandering takes `next[SplitMix64 of (vehicle, lane) mod n]`.
  - **`traffic.drive`:**
    - the IDM toward the nearest thing within 150 m: a vehicle; a person in the lane (width / 2 + radius sideways, 2 m vertically); an ungranted stop line, a dead end or the destination end (a stopped obstacle s₀ past the line);
    - the ballistic update;
    - lane ends crossed by the centre;
    - a yaw-then-pitch heading, so a vehicle never rolls;
    - arrival at the destination end or queued behind an arrived vehicle.
  - **`traffic.junctions`:** grants hold while the vehicle is about to enter, on, or with its rear in the connector. Requests come within twice the IDM's desired gap and are granted greedily by (priority, waiting since, id) if they conflict with nothing held.
  - **LOD:** nothing opts in.

## How to test
`cargo nextest run -p concordia-terra-crowd`. `tests/crowd.rs` runs nav's and the crowd's processors on a `SimLoop` without physics (surface agents move themselves), so ticks are hand-computable; the header derives the kinematics (2/15 m/s of acceleration a tick) and the collision bound (0.0081 per tick for a lone pair). `tests/avoidance.rs` (2.5c.2) checks avoidance, walls and LOD against controls with them off. `tests/traffic.rs` (2.5d) drives vehicles on the same rig with no navmesh (its header derives the IDM's first ticks); the IDM, routes, conflicts, grants and wandering have unit tests in `src/traffic/`. `tests/engine.rs` runs an engine with Jolt (physics agents), input (click-to-move), templates and the debug layer. Unit tests: the corridor helpers (`src/corridor.rs`), the sampler with hand-computed penalties (`src/avoid.rs`), the boundary's insertion (`src/boundary.rs`), the wall filter (`src/steps/avoid.rs`) and the fixed table (`src/tables.rs`). `benches/crowd.rs` (`crowd_10k`) is recorded in `benches/BASELINE.md`, with no pass/fail line. Tests that aren't about avoidance turn it off (`avoidance: None`), since it is on by default.

## Common tasks
- **A new crowd step:** a pure function in its own module (`steer.rs`, `motion.rs`, `collide.rs`, `corridor.rs`, `avoid.rs`, `boundary.rs`), glued by a step in `steps/` and declared in `processors.rs` with declared reads and writes; read other agents only from `CrowdGrid` or a snapshot.
- **A new Detour query:** add it to the `recast-sys` shim narrowly (a smoke test, the header hash), wrap it on `concordia_nav::NavMesh` with raw `u64` references, then call it here.

## Don'ts
- Don't wrap `dtCrowd` (N1): crowd state must stay hashed and inspectable.
- Don't read another agent's components directly in a parallel step; use the snapshot.
- Don't write `NavAgent` (gameplay's) or move characters (physics owns them).

## References
- DetourCrowd v1.6.0's zlib source (`DetourCrowd.cpp`, `DetourPathCorridor.cpp`, `DetourObstacleAvoidance.cpp`, `DetourLocalBoundary.cpp`, `DetourProximityGrid.cpp`, commit `6dc1667f`, read, not vendored) and Mikko Mononen's blog posts on DetourCrowd; van den Berg, Lin and Manocha, "Reciprocal Velocity Obstacles for Real-Time Multi-Agent Navigation" (ICRA 2008), and van den Berg, Guy, Lin and Manocha, "Reciprocal n-Body Collision Avoidance" (ISRR 2009); Godot's `NavigationAgent3D` avoidance docs (MIT), for semantics only. Traffic: Treiber, Hennecke and Helbing, "Congested traffic states in empirical observations and microscopic simulations" (Phys. Rev. E 62, 2000), and Treiber and Kesting, *Traffic Flow Dynamics* (2013), for the IDM and its ballistic update; Dijkstra (1959); SplitMix64 (Steele, Lea and Flood, 2014). No Unreal or other non-open source, and no traffic engine's code, was consulted.

## Status
M2 phases 2.5c.1 (crowd core), 2.5c.2 (avoidance, the local boundary and LOD; the `crowd_10k` bench) and 2.5d (lane traffic). Scale (100k, parallel or incremental grid and snapshots) is M7 7.7.

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
