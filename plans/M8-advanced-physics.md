# M8: Advanced physics (destruction and GPU garments)
Status: Planned (ADR-0012) · Depends on: M1 (Jolt, physics module, RHI compute, Slang), M2 (skeletal animation and skinning, mesh import) · Destruction phases 8.1–8.6 may start right after M1; garment phases 8.7–8.10 need M2 · Detail level: **Needs refinement after M2** · Estimated effort: ~20 weeks (range 16–26), author plus agents

> Outline plan with phase breakdowns and test lists. Refine when M2 lands: the skinning data layout (8.7, 8.10), the mesh import path for garments, and M3's replay format if caches build on it (8.6).

## Goal & context
Bring Chaos-class physics to Concordia **on top of Jolt**, not instead of it:
1. **Destruction.** Fracture meshes into pieces, organized in cluster levels. A connection graph holds the pieces together with strength limits. At runtime, impacts and **fields** apply strain; connections break and pieces fly apart as Jolt bodies. Debris is budgeted, and destruction can be cached and played back.
2. **A GPU garment solver.** Characters wear simulated clothing (capes, skirts, coats, sleeves) that follows the animated body. It's solved on the GPU in our own RHI, with painted controls (max distance, backstop, stiffness), long-range tethers, body collision and self-collision.

**What Jolt already gives us** (pinned v5.7, MIT), reused rather than rewritten:
- rigid bodies, convex hull and compound shapes;
- contact listeners, which report impulses for strain;
- deterministic multithreaded stepping (ADR-0041);
- CPU soft bodies with skinning constraints (since v5.0);
- a GPU compute interface and strand hair (since v5.6, marked work-in-progress).

**What we build:**
- fracture authoring;
- the cluster and connection-graph runtime;
- fields, the debris budget and caching;
- the garment asset, a GPU solver on `concordia-rhi`, and collision;
- editor and agent tooling.

**Why our own GPU solver** rather than Jolt's compute interface:
- Garments feed the skinned-mesh render pipeline. Solving in our RHI keeps vertex data on the GPU with no copies, and uses our render graph, Slang toolchain and Metal/Vulkan backends.
- Jolt's CPU soft body becomes the **test oracle** for the solver's math.

**Clean room** (`docs/clean-room.md`): Unreal's Chaos is source-available, not open source, so no Chaos code may be read or ported. Epic's public documentation may be read to learn *which* features users expect (geometry collections, clustering, connection graphs, strain, fields, caches; cloth max distance and backstop). The *how* comes from published papers and permissively licensed code (see References).

## Scope / Non-goals
**In scope**
- Offline fracture:
  - patterns: Voronoi (uniform, clustered, radial), planar slices and bricks;
  - interior faces with their own material and UVs;
  - collision hulls for each piece;
  - several cluster levels;
  - deterministic output from a seed.
- Runtime destruction:
  - a connection graph with strength limits, and accumulated strain;
  - breaking a cluster into connected components, keeping momentum;
  - anchors;
  - deterministic results.
- Fields: strain, force or impulse, anchor, sleep, disable, kill and crumble, with shapes and falloff. They work for both destruction and cloth.
- A debris budget (lifetime, count, size culling) and GPU-instanced fragment rendering.
- Destruction caches: record, then play back without simulating.
- Garments:
  - simulation mesh vs render mesh (embedding);
  - painted maps for max distance, backstop, stiffness and mass;
  - long-range attachment tethers;
  - skinning to the animated body.
- The GPU solver:
  - XPBD with small steps;
  - constraints for stretch, shear, bending, tethers, max distance and backstop;
  - a CPU reference build of the same solver.
- Collision:
  - body proxies (capsules, spheres, convex shapes on bones);
  - nearby Jolt world shapes;
  - self-collision;
  - friction;
  - wind.
- Engine API, MCP tools and editor panels:
  - fracture preview;
  - cluster and connection-graph debug views;
  - weight painting ("needs visual design": Godot has no equivalent);
  - garment debug views.

**Non-goals (this milestone)**
- Networked or replicated destruction (with multiplayer, later).
- Jolt's GPU strand hair: wrapped later, once Jolt marks it stable (backlog).
- Flesh and muscle simulation (Chaos Flesh-style), and sewing garments from 2D panels as in Marvelous Designer (backlog).
- Fluids.

## Dependencies (what must already exist, from which milestone)
| Needed | From |
|---|---|
| `concordia-physics` over `jolt-sys`, deterministic stepping, contact events (Terra signals `ContactBegan`/`ContactEnded` since M2 2.0b) | M1, M2 |
| RHI compute pipelines, Slang compute shaders, GPU instancing, render graph | M1 |
| `EngineModule` seam, play world vs edit world (ADR-0032, ADR-0046) | M0, M1 |
| Skeletons, skinning, animation clips and the skinned-mesh render path | M2 (2.4) |
| Mesh and material import for garment and fracture source meshes | M1 (glTF), M2 (2.3, FBX) |
| Replay (if caches reuse its format) | M3 (optional) |

## Crates, modules & directories (created / touched)
- **`crates/concordia-fracture`:** fracture authoring, pure geometry with no physics dependency:
  - patterns and convex cells;
  - mesh cutting;
  - interior faces and hulls;
  - cluster levels and the connection graph.
- **`crates/concordia-destruction`:** the runtime engine module:
  - breakable bodies on Jolt;
  - strain and breaking;
  - fields;
  - debris;
  - caches;
  - `destruction.*` methods.
- **`crates/concordia-cloth`:** the garment asset, the CPU reference solver, the GPU solver (Slang shaders under `shaders/cloth/`), collision, and the `cloth.*` methods.
- Touched:
  - `concordia-physics` (contact impulse events, compound body splitting);
  - `concordia-render` (fragment instancing, simulated-vertex input to the skinned path, debug views);
  - `editor/ui` (fracture, graph and paint panels).
- **New formats:**
  - **`.breakable.ron`:** the fracture recipe (source mesh, pattern, seed, cluster levels, strength overrides). The pieces are derived data in the DDC.
  - **`.garment.ron`:** the garment (render mesh, simulation mesh, per-vertex maps, constraint settings).
  - **Destruction caches** are binary, so they live in the DDC, or in a referenced binary file, by ADR.

## Phases

### Phase 8.1: Fracture authoring (`concordia-fracture`)
- **Objective:** turn a mesh into pieces from a seeded pattern, deterministically.
- **Work:**
  - **Convex cells** by clipping the mesh bounds against half-spaces (Voronoi via its Delaunay dual, or direct cell clipping). Patterns: uniform, clustered, radial (impact point), planar slices, bricks.
  - **Robust mesh-against-cell intersection.** A spike first: bind `manifold` (Apache-2.0) vs our own exact-predicate cutter.
  - **Interior faces** get their own material slot and generated UVs.
  - **Collision per piece:** convex hulls (Jolt `ConvexHullShape`), with optional convex decomposition for concave pieces (CoACD, MIT, or V-HACD, BSD-3).
  - **`fracture.generate {mesh, pattern, seed}`** is an Engine API method (an MCP tool for agents) that writes a `.breakable.ron`.
- **Tests first:**
  - `same_seed_same_pieces_byte_for_byte`
  - `pieces_partition_the_volume` (summed piece volumes equal the source's within ε; no overlaps by sampling)
  - `every_piece_is_closed_and_manifold`
  - `interior_faces_use_the_interior_material`
  - `hull_contains_its_piece`
  - `degenerate_inputs_are_errors_not_panics` (an open mesh, zero cells, NaN)
  - proptest: random seeds and cell counts keep the volume invariant.

### Phase 8.2: Cluster levels and the connection graph
- **Objective:** the structure that holds pieces together.
- **Work:**
  - **Cluster levels:** pieces group into parent clusters by proximity, k-means over cell sites, or a user-defined grouping.
  - **The connection graph:** an edge per pair of pieces whose faces touch. Strength = contact area × material strength, overridable per level and per edge in `.breakable.ron`.
  - **Damage thresholds per level.**
  - **Mass properties** per piece and per cluster, from density.
- **Tests first:**
  - `touching_pieces_are_connected_and_separate_ones_are_not`
  - `edge_strength_scales_with_contact_area`
  - `cluster_mass_is_the_sum_of_its_pieces`
  - `graph_round_trips_through_breakable_ron_canonically`

### Phase 8.3: Breaking at runtime on Jolt (`concordia-destruction`)
- **Objective:** intact clusters are single cheap Jolt bodies that split when strained.
- **Work:**
  - **An intact cluster is one Jolt body** with a compound of its pieces' hulls.
  - **Contact impulses** (from Jolt's contact listener, through `concordia-physics` events) add strain to the edges near the contact.
  - **Breaking:** an edge whose strain passes its threshold breaks. Union-find over the remaining edges gives the connected components, and each becomes a new body. Linear and angular velocity are kept at each component's centre of mass.
  - **Anchors** keep components static.
  - **Determinism:**
    - break order sorted by piece id;
    - a cap on breaks per tick, with the excess carried to later ticks;
    - simulation only in the play world (ADR-0032).
- **Tests first:**
  - `an_unstrained_cluster_stays_one_body`
  - `impulse_above_threshold_splits_into_connected_components`
  - `momentum_is_conserved_across_a_split` (linear and angular, within ε)
  - `anchored_components_stay_static`
  - `destruction_hash_is_identical_at_1_2_and_8_threads`
  - `break_budget_defers_excess_breaks_to_later_ticks`
  - `edit_world_is_never_broken` (ADR-0032)

### Phase 8.4: Fields
- **Objective:** a region of space that applies an effect, for designers and agents.
- **Work:**
  - **Components:** `Field {shape: sphere | box | plane | everywhere, falloff: none | linear | inverse_square, effect, magnitude, duration}`.
  - **Effects:** external strain, force, impulse, anchor, sleep, disable, kill and crumble (break every edge of a cluster).
  - **Applied at a fixed point in the tick**, so they're deterministic.
  - **Cloth reads force fields** too, once 8.9 lands.
  - **`destruction.apply_strain`** and **`destruction.crumble`** methods for agents.
- **Tests first:**
  - `strain_field_breaks_only_inside_its_shape`
  - `falloff_scales_effect_with_distance`
  - `anchor_field_pins_pieces_it_covers`
  - `kill_field_despawns_debris_in_play_world_only`
  - `fields_are_ordinary_components_editable_and_undoable`

### Phase 8.5: Debris, rendering and performance
- **Objective:** big destruction stays within budget.
- **Work:**
  - **Rendering:** pieces render as GPU-instanced draws per breakable (one mesh buffer, one transform buffer), including interior faces.
  - **Sleeping:** resting debris sleeps.
  - **A debris budget:** maximum count, lifetime, and minimum size. Below the size cutoff, debris loses collision or turns into particles; past the budget, the oldest debris goes first.
  - **Distance-based LOD:** far clusters break at a coarser level.
  - **A perf bench:** `cargo bench --bench destruction` on the M3 Pro reference machine.
- **Tests first:**
  - `debris_over_budget_removes_the_oldest_first`
  - `small_debris_drops_collision`
  - `one_draw_per_breakable_regardless_of_piece_count` (stats)
  - a golden image of a broken wall;
  - the bench: a 2,000-piece wall breaking holds 120 Hz simulation at 60 fps (target refined in 8.11).

### Phase 8.6: Destruction caches
- **Objective:** author a collapse once, then play it back cheaply and identically (cinematics, heavy set pieces).
- **Work:**
  - `destruction.record` saves each piece's transform per tick, compressed (quantized and keyframed).
  - `destruction.play` drives the pieces from the cache, with no simulation, optionally switching to live simulation at a chosen tick.
  - Caches are binary, stored by an ADR-decided location; the `.breakable.ron` references them.
- **Tests first:**
  - `playback_matches_the_recorded_simulation_within_quantization`
  - `cache_is_invalidated_when_the_recipe_changes`
  - `switch_to_live_sim_continues_with_the_cached_velocities`

### Phase 8.7: Garment asset (`concordia-cloth`)
- **Objective:** turn an imported mesh into a garment description.
- **Work:**
  - **Two meshes:** a simulation mesh (coarse, welded) and the render mesh, which is embedded in it by barycentric mapping with a normal offset.
  - **Per-vertex maps:** max distance (0 = fully skinned), backstop distance and radius, stiffness scale, mass and damping.
  - **Long-range attachment tethers,** computed from the skinned vertices (geodesic or Euclidean).
  - **Binding** to the M2 skeleton by skin weights. *Note (2026-11-20, ADR-0016):* with M12's rigs, garments and body proxies follow the final presented pose, after both Simulation and Presentation rigs (M12 open question 16).
  - **`.garment.ron`,** in canonical text.
  - **`cloth.create {mesh, ...}`** for agents.
- **Tests first:**
  - `embedding_reproduces_the_render_mesh_at_rest`
  - `zero_max_distance_vertices_are_skinned_exactly`
  - `tethers_connect_every_free_vertex_to_its_nearest_anchor`
  - `garment_round_trips_canonically`
  - `invalid_maps_are_errors` (a wrong length, NaN, negative distance)

### Phase 8.8: The solver (CPU reference, then GPU)
- **Objective:** a stable, fast cloth solver, the same math on the CPU and GPU.
- **Work:**
  - **XPBD with small steps** (many substeps, one iteration each).
  - **Constraints:** stretch and shear (distance or triangle strain), bending (dihedral, or isometric bending for speed), LRA tethers, max distance and backstop, all against the skinned positions.
  - **Graph colouring** for parallel Gauss–Seidel on the GPU; Jacobi as a fallback.
  - **Slang compute kernels** on `concordia-rhi`.
  - **A deterministic mode:** fixed order, no atomics on floats.
  - **A two-week spike** comparing Vertex Block Descent (and its 2025 augmented variant) with XPBD for stiffness and speed. The result is recorded in an ADR.
- **Tests first:**
  - `cpu_solver_matches_jolt_soft_body_on_a_hanging_sheet` (Jolt as oracle, within tolerance)
  - `gpu_matches_cpu_reference_within_tolerance` (needs a GPU; skipped otherwise unless `CONCORDIA_REQUIRE_GPU=1`)
  - `energy_does_not_grow_on_a_resting_sheet`
  - `tethers_bound_stretch_under_gravity`
  - `max_distance_is_never_exceeded`
  - `deterministic_mode_is_bitwise_repeatable`
  - `substep_count_trades_stiffness_monotonically`

### Phase 8.9: Collision and forces
- **Objective:** garments that don't pass through the body or the world.
- **Work:**
  - **Body proxies:** capsules, spheres and tapered capsules attached to bones, generated automatically from the skeleton and skinned mesh, and editable.
  - **Nearby Jolt world shapes:** queried each frame (broad phase) and converted to primitives or convex shapes for the solver.
  - **Self-collision:** a GPU spatial hash, vertex–triangle tests, thickness.
  - **Friction.**
  - **Wind,** plus force fields from 8.4.
  - **Robustness:** resetting on teleports (big jumps in the root's movement), a maximum velocity clamp, and per-garment simulation LOD (distance, then falling back to plain skinning).
- **Tests first:**
  - `cloth_never_penetrates_body_proxies_beyond_thickness`
  - `self_collision_prevents_pass_through_on_a_folded_sheet`
  - `teleport_resets_without_explosion`
  - `world_collider_from_jolt_stops_a_falling_sheet`
  - `wind_field_moves_cloth_in_its_direction`
  - `lod_falls_back_to_skinning_beyond_distance`

### Phase 8.10: Rendering, editor and agent tooling
- **Objective:** see it, tune it, and let agents drive it.
- **Work:**
  - **Rendering:** solver output feeds the skinned-mesh path straight from GPU buffers. Normals are recomputed on the GPU, and the render mesh is updated through the embedding.
  - **Editor:**
    - fracture preview with an explode slider;
    - cluster-level and connection-graph overlays (engine-drawn, ADR-0047);
    - a strain heat map;
    - fields drawn as gizmos;
    - garment weight painting (brush, falloff, mirror; "needs visual design");
    - constraint and collision-proxy debug views.
  - **MCP tools:** `fracture.generate`, `destruction.*`, `cloth.*`, and debug captures.
- **Tests first:**
  - `simulated_vertices_reach_the_renderer_without_readback` (render stats);
  - a golden image of a cape at rest;
  - unit tests for the paint brush math;
  - e2e:
    - paint max distance and play: the painted region stays skinned;
    - fracture a cube in the editor and preview the explode view.

### Phase 8.11: Gate assembly, performance baseline, docs
- Gate fixtures:
  - a fractured wall: about 2,000 pieces, 3 levels, an anchored base;
  - a walking character (an M2 clip) wearing a cape and a skirt.
- Performance baselines on the M3 Pro.
- `docs/guides/destruction.md` and `docs/guides/garments.md`.
- Crate `AGENTS.md` files with references.

## Milestone gate (the executable acceptance test: exact command and what it asserts)
`just gate M8` runs `tests/gates/m8_*.rs`, which asserts:
1. **Destruction:** a projectile hits the gate wall, and the wall breaks.
   - The destruction world hash is identical at 1, 2 and 8 threads, and on rerun.
   - Anchored pieces don't move.
   - Momentum is conserved within ε at the first split.
   - Debris settles within the budget within N seconds.
   - A golden image of the aftermath passes.
2. **Fields:** a strain field crumbles only the region it covers.
3. **Caches:** a recorded collapse plays back within quantization error.
4. **Garments:** the walking character's cape and skirt simulate on the GPU at 60 fps or better, with the solver under a budget set in 8.11 (target ≤ 1.5 ms per character on the M3 Pro).
   - No vertex exceeds its max distance.
   - No vertex penetrates a body proxy beyond the garment's thickness.
   - The GPU result matches the CPU reference within tolerance over 300 frames.
   - A golden image passes.
5. **The agent demo:** Claude Code on the user's subscription, through MCP, fractures a mesh with `fracture.generate`, places it, breaks it with a strain field, steps the simulation, and describes what `capture_screenshot` shows.

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| Mesh booleans are numerically fragile (slivers, non-manifold pieces) | Spike `manifold` first; exact predicates; validity tests on every piece; proptests |
| Splitting Jolt compounds mid-step costs frame time, or breaks determinism | Break between steps only; a per-tick budget; sorted order; a thread-count hash test |
| GPU floating-point nondeterminism | A deterministic mode with no float atomics; gameplay-relevant destruction stays on the CPU (Jolt); cloth is cosmetic by default |
| Cloth instability (stiff materials, fast motion) | Small steps, tethers, velocity clamps, teleport reset; the VBD spike as a fallback |
| Self-collision cost | Spatial hash; self-collision per garment opt-in; LOD |
| Clean-room exposure from Chaos's docs | Docs for feature names and behaviour only; papers for algorithms; references recorded in each crate's `AGENTS.md` |
| Depends on M2 skinning for garments | Garment phases come after M2; destruction proceeds first |

## Open questions
1. **Schedule:** should destruction (8.1–8.6) start right after M1, ahead of M2, or does all of M8 follow M2?
2. **Gameplay authority for destruction:** CPU on Jolt only (proposed), with GPU-only cosmetic debris as a later option?
3. **Cache storage:** DDC (rebuildable) or a committed binary file next to `.breakable.ron`, which is authored data? This needs an ADR, because text formats are the source of truth.
4. **Solver default:** XPBD or VBD, decided by the 8.8 spike.
5. **Should garments also get a CPU path** for platforms without compute (the reference solver at reduced quality)?

## ADRs to write
- **ADR-0180:** fracture geometry backend (`manifold` bindings vs our own cutter).
- **ADR-0181:** where destruction caches live.
- **ADR-0182:** cloth solver choice (XPBD small steps vs VBD).
- **ADR-0183:** determinism contract for GPU cloth (cosmetic by default).

## References (clean room)
- **Cloth solving:**
  - Macklin, Müller, Chentanez, *XPBD: Position-Based Simulation of Compliant Constrained Dynamics* (MIG 2016).
  - Macklin et al., *Small Steps in Physics Simulation* (SCA 2019).
  - Kim, Chentanez, Müller, *Long Range Attachments: A Method to Simulate Inextensible Clothing in Computer Games* (SCA 2012).
  - Bergou et al., *A Quadratic Bending Model for Inextensible Surfaces* (SGP 2006).
  - Chen, Liu, Yang, Yuksel, *Vertex Block Descent* (SIGGRAPH 2024), and *Augmented VBD* (SIGGRAPH 2025, CC BY 4.0 paper).
- **Fracture:** Müller, Chentanez, Kim, *Real Time Dynamic Fracture with Volumetric Approximate Convex Decompositions* (SIGGRAPH 2013).
- **Libraries:**
  - Jolt Physics (MIT): rigid bodies, soft bodies as a test oracle, release notes for v5.0 and v5.6;
  - `manifold` (Apache-2.0);
  - CoACD (MIT);
  - V-HACD (BSD-3).
- **Feature expectations only (no code):** Epic's public Chaos Destruction, Chaos Fields and Chaos Cloth user documentation.

## Effort estimate
| Phases | Weeks |
|---|---|
| 8.1–8.2 fracture and graph | 4–5 |
| 8.3–8.4 runtime breaking and fields | 3–4 |
| 8.5–8.6 debris, rendering, caches | 2–3 |
| 8.7–8.8 garment asset and solver (incl. VBD spike) | 4–6 |
| 8.9 collision | 2–3 |
| 8.10–8.11 tooling and gate | 2–3 |
| **Total** | **~20 (16–26)** |
