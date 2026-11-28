# M8: Advanced physics (destruction and GPU garments)
Status: Planned (ADR-0012) · Depends on: M1 (Jolt, physics module, RHI compute, Slang), M2 (skeletal animation and skinning, mesh import) · Destruction phases 8.1–8.6 may start right after M1; garment phases 8.7–8.10 need M2 · Detail level: **Refined after M4 (2026-11-27; Q1–Q8 approved)** · **Phase 8.0 done** (8.0a–d: hull and compound colliders, contact impacts, mid-play bodies, the soft-body reference) · **Phase 8.4 done** (fields: strain, crumble, anchor, kill, force, impulse, sleep, disable) · **8.3 done** (breaking on Jolt) · **8.2 done** (connection graph, cluster levels) · **Phase 8.1 done** (8.1a–c: Manifold cutter and five patterns; hulls and interior UVs; `.breakable.ron` and `fracture.generate`) · Estimated effort: ~20 weeks (range 16–26), author plus agents

> Outline plan with phase breakdowns and test lists. Refine when M2 lands: the skinning data layout (8.7, 8.10), the mesh import path for garments, and M3's replay format if caches build on it (8.6).

## Refinement after M4 (2026-11-27; Q1–Q8 approved by the author the same day)
The plan predates M2. Checked against `main` after M4; the phases below stand, with the changes and proposals here. Q1–Q8 were my recommendations; the author approved all eight as written.

### What exists (findings)
- **Physics.**
  - `jolt-sys` binds world create/step/save/restore, body add/remove/state/move-kinematic, contact events (who and whether a sensor, **no impulse**), raycasts (now skipping sensors, M4 4.13a) and characters.
  - `concordia-physics` colliders are box, sphere, capsule and cylinder only. **No convex hull, compound or soft-body shapes are bound,** though vendored Jolt v5.7 has `ConvexHullShape`, `StaticCompoundShape`, `MutableCompoundShape` and soft bodies.
  - Bodies are made by `sync_bodies` from components; nothing splits a body during play.
- **Rendering.**
  - GPU compute on Metal and Vulkan through the RHI, with Slang.
  - GPU skinning (`skinning.rs`, `skin.slang`).
  - Meshlets with GPU culling and instanced draws (M4 4.3–4.4); meshopt (simplification); `concordia-bvh`; per-pass GPU timing; FLIP goldens and the path tracer.
- **Everything else.**
  - Replay format 2 (M4 4.12d) with per-tick checkpoints and an external-output list.
  - Terra signals and observers.
  - The graph node library (get/set for any reflected component, so fields become graph-scriptable for free).
  - The editor's panel and engine-drawn overlay patterns (M2, M4 4.11), and MCP tool generation.
- **Out of date in this plan:**
  - Open question 1 (start destruction before M2) no longer applies.
  - The skinning layout and replay format it waited for exist.
  - M12's rigs don't exist yet, so garments follow the skinned pose as M2 draws it (8.7's note stands).

### Decisions (approved)
- **Q1: a physics groundwork phase, 8.0, before 8.1.** It binds and exposes:
  - convex hull and compound colliders (`ColliderShape::ConvexHull`, `::Compound`; mutable compound for splitting);
  - contact impulse and point in `ContactBegan`, and a per-contact strength signal;
  - adding and removing bodies mid-play deterministically (sorted, between steps);
  - Jolt soft bodies, test-only, as 8.8's oracle.

  Tests first; every existing physics replay and golden unchanged.
- **Q2: how meshes are cut (ADR-0180).** Run the planned spike in 8.1, leaning towards binding `manifold` (Apache-2.0) behind a new `manifold-sys` crate (the `jolt-sys` pattern, vendored under `third_party/`). Our own exact-predicate cutter is the fallback if the binding is too heavy or not deterministic across platforms.
- **Q3: where caches live (ADR-0181).** In the DDC, rebuilt from the `.breakable.ron` recipe plus a recorded replay of the collapse, since text stays the source of truth. A committed binary only by an explicit `destruction.export`. Caches reuse replay format 2's checkpoint machinery rather than a second recording system.
- **Q4: authority (ADR-0183).**
  - Gameplay destruction is CPU only, on Jolt: deterministic, replayable, hashed.
  - GPU cloth is cosmetic by default: not hashed, with a deterministic mode for tests.
  - GPU-only cosmetic debris goes to the backlog.
- **Q5: two tracks, like M4.** Destruction (8.0–8.6) first; garments (8.7–8.10) can start once 8.0 lands, since they need only the soft-body oracle and skinning; 8.11 joins them.
- **Q6: a proving ground.** A small original example, `examples/m8-quarry`: a stone wall on an anchored base, a projectile launcher and a caped character on an M2 walk clip. It is generated from seeds like the canyon (recipes committed, meshes in the DDC), so the gate needs no port content.
- **Q7: the agent demo** (gate item 5) runs scripted through the Engine API in CI, as M3's Studio loop did, plus a live run on the author's subscription recorded in the As built.
- **Q8: perf.** Destruction's 120 Hz bench and cloth's 1.5 ms per character are asserted on the reference machine, under P1's rule from M7 (perf runs alone, after the budgets are recalibrated). They are recorded, not gated, if M8 runs before M7's recalibration.

### As built
**8.0a** (`m8/8.0-physics-groundwork`, done): convex hull and compound colliders.
- **`jolt-sys`:** `CJ_SHAPE_CONVEX_HULL` and `CJ_SHAPE_COMPOUND` (Jolt `ConvexHullShape` with the default convex radius; `StaticCompoundShape` of hulls), plus `cj_convex_hull_triangles`.
- **`concordia-physics`:** `ColliderShape::ConvexHull { points }` and `::Compound { parts }`, validated (at least 4 finite points; a hull with no volume is rejected and reported). `hull_triangles` exposes the hull's faces.
- **`concordia-nav`:** hull and compound colliders are navmesh sources, through the physics hull.
- **Editor:** schemas and TypeScript types regenerated.
- **Bugs found on the way, each pinned by a test:**
  - Jolt builds a zero-volume hull from flat points (NaN mass), so the shim now checks volume.
  - Jolt keeps hulls around their centre of mass, so the triangles must be placed there; without that, an off-origin hull's triangles came back centred.
  - Navigation's `outward` orients triangles from the shape's origin, which flips the faces of a compound's off-centre parts, so hulls skip it.
- **Test log:**
  - The jolt-sys smoke tests failed first: link error, then rejection and recentring.
  - The physics `shapes` tests and the navmesh test failed first: no variants, then the wrong area, which led to the two navigation bugs above.
  - One fixture of mine needed hull faces mid-cell; a face on a cell boundary goes to one side only (the file's own rule).
  - Contract consumers: API, MCP, app, scene, crowds and reflect; editor schemas.
  - Checks: fmt, clippy (workspace).

**8.0b** (done): contact impacts.
- **Opt-in:** `ContactReports { min_strength }` gives `ContactImpact { other, strength, point }` signals. Existing scenes raise the same signals as before, so no replay or golden changed.
- **Strength:** the closing speed along the normal times the pair's reduced mass, taken in the shim's `OnContactAdded`. A 2 kg ball dropped 5 m measures 19.75 N·s against the expected 19.81.
- **Test log:** the `impacts` tests failed to compile first (no types), then passed. The measured value was printed to confirm it.

**8.0c** (done): bodies changing mid-play.
- `midplay` tests pin what M2 built: spawned pieces carry the whole body's momentum, and a mid-play split hashes the same at 1, 2 and 8 threads and on a rerun.
- **Test log:** these passed once my test was right; they prove, rather than drive, the existing mechanism. The first failure was mine: I forgot the bodies' default linear damping (one tick of 0.05/s).

**8.0d** (done; **Phase 8.0 done**): Jolt's soft body as the cloth reference.
- **`jolt-sys`:** `cj_soft_body_add` and `cj_soft_body_positions`.
- **`concordia-physics`:** `SoftReference` and `SoftSheet`, which own their own Jolt world.
- **What the tests show:** a sheet pinned along its top row hangs about 1 m below it, stays finite, and repeats exactly.
- **Rejected as errors:** no faces, an index out of range, a degenerate face, mismatched lengths, and NaN. Jolt only asserts against degenerate faces, and its asserts are compiled out, so the shim checks them itself.
- **Test log:** the smoke test failed to link first, and the physics test failed to compile first (no types); both then passed.

**8.1a** (done): the cutter and the patterns (ADR-0180).
- **`manifold-sys`:** Manifold v3.5.5's core (Apache-2.0), vendored under `third_party/manifold` and built with `cc`: sequential, no fused multiply-add, no cross sections. A C shim exposes one operation, `cm_clip_convex`, which clips a closed mesh by a convex cell and marks the cut faces. Hand-written bindings, checked by a header hash, and a checked wrapper, `safe::clip_convex`.
- **`concordia-fracture`:** `fracture(mesh, pattern, seed)`. The patterns are `Uniform`, `Clustered` and `Radial` (Voronoi cells from bisector planes), `Slices` and `Bricks` (running bond). Pieces come with a material slot per triangle (`Surface` or `Interior`); cells that miss the mesh make no piece. At most 1,024 cells.
- **What the tests show:**
  - on a box and a concave L-prism, the pieces' volumes sum to the source's within 1e-4;
  - no point is in two pieces (sampled with the generalised winding number);
  - every piece is closed and manifold;
  - cut faces, and only those, are interior;
  - the same seed gives the same bytes;
  - bad meshes and patterns are errors.
- **Moved to 8.1b:** `hull_contains_its_piece` (hulls per piece through `concordia-physics`), interior UVs, `fracture.generate` and `.breakable.ron`.
- **Test log:**
  - `manifold-sys`'s clip tests were first run after the shim existed, so they weren't seen failing first.
  - The fracture tests were written first and run against a stub that returns no pieces: 7 of 8 failed. `every_piece_is_closed_and_manifold` passed on the stub because it found no pieces to check. All 8 then passed with the cutter.
  - `pieces_are_the_same_bytes_on_every_platform` pins a blake3 of every pattern's output, taken on macOS (ARM). Linux CI (x86) only lints, so this pin is first checked on Linux at the next regression run.
  - Checks: fmt, clippy, cargo deny licenses.

**8.1b** (done): what a piece needs for play.
- `Piece::centre_of_mass` (uniform density).
- `Piece::collider`: a `ColliderShape::ConvexHull` of the piece's vertices, relative to its centre of mass. Concave pieces get their hull; convex decomposition (CoACD or V-HACD) moves to the backlog until a test needs it.
- `Piece::interior_uvs(metres_per_tile)`: a planar projection in world space, so both sides of a cut line up.
- **Not yet:** the source mesh's own UVs and normals carried through the cut. Surface faces get zeros until rendering needs them (8.5).
- **What the tests show:** every vertex is within Jolt's 1 mm hull tolerance of its hull, concave pieces included; the collider sits around the centre of mass; the UVs on both sides of a cut agree, scale with `metres_per_tile`, and a bad scale is an error.
- **Test log:** the four `pieces` tests were written first and failed against stubs, each for its own reason; they then passed.

**8.1c** (done; **Phase 8.1 done**): the recipe and the Engine API.
- **`.breakable.ron`** (`Breakable`) is the recipe:
  - its fields: source mesh reference, pattern, seed, metres per UV unit, optional interior material;
  - canonical text through `concordia-scene`;
  - cluster levels and strengths join it in 8.2.
- **The pieces are derived data in the DDC**, keyed by:
  - the recipe's canonical text;
  - a blake3 of the source mesh's data;
  - `FRACTURE_VERSION`.

  They are rebuilt from the recipe when the key is missing. ADR-0181 (8.6) is still owed for collapse caches.
- **`[fracture]`** adds `FractureModule`:
  - `fracture.generate` loads the mesh through the asset database, welds it and cuts it, caches the pieces, then writes the recipe as one undoable document edit;
  - `fracture.inspect` summarises the pieces and says whether they came from the cache.
- **Welding:** imported meshes split vertices along hard edges and seams, so their vertices are welded by position before cutting.
- **Studio roles** get the fracture tools with the rest of M8's MCP tooling, in 8.10.
- **What the tests show:**
  - a generated box fractures into pieces that keep its volume;
  - the recipe reads back canonically;
  - the pieces come from the DDC, and rebuild to the same pieces once it's deleted;
  - undo and redo remove and restore the recipe;
  - each bad field is named, and nothing is written.
- **Test log:**
  - **Not test-first:** I wrote the `generate` tests before the code, but first ran them after the module existed. Two of my test assumptions were then wrong: mutating methods need an open scene, and a missing actor is the engine's `ActorRequired`.
  - The `weld` tests were checked against an identity stub (both failed), then passed.
  - **Contract consumers:** app, MCP and Studio tests (346 passed). The editor schemas are unchanged, since their project has no `[fracture]`.
  - **Checks:** fmt, clippy.

**8.2** (done): cluster levels and the connection graph.
- **`structure(fracture, spec)`:**
  - **Bonds:** an edge per pair of pieces whose interior faces lie on one plane facing each other. The area is their overlap, by polygon clipping in the plane, so a brick touching two bricks above gets two edges. Strength is area × the spec's `strength`, unless an edge override replaces it.
  - **Mass** per piece and per cluster comes from `density`, with mass-weighted centres.
  - **Cluster levels:** each groups the level below by k-means, which starts from farthest points (no randomness), or by a hand-made grouping. Each level has a damage threshold. A level's bonds sum the piece bonds that cross between its clusters.
- **`.breakable.ron`** gains `structure: StructureSpec` (density, strength, levels, edge overrides). `fracture.generate` takes it as an optional `structure`, defaulting to stone with no levels.
- **What the tests show:**
  - slices form a chain, and a running bond has exactly its seven touching pairs;
  - strength scales with contact area, and an override replaces it (either order);
  - each cluster's mass is the sum of its pieces' (on the L, 6000 kg at 2000 kg/m³), and the top cluster's centre is the L's centroid;
  - a manual grouping's level edge joins its two halves;
  - the spec round-trips canonically;
  - bad specs are errors.
- **Test log:**
  - The tests were written first, and run against a stub before the code: 6 of 7 failed. The round-trip test doesn't use `structure`.
  - One expected value of mine was wrong: a cut across x is a 1 × 1 face whatever the slice thickness. I fixed the test, not the code.
  - **Checks:** fmt, clippy.

**8.3** (done): breaking at runtime (S1–S8 above).
- **New crate `concordia-destruction`:**
  - the components `Breakable` and `Fragment`;
  - the `destruction.break` system;
  - `BreakableSource` and `MemorySource`;
  - `[destruction] breaks_per_tick` (`DestructionModule`), which loads recipes when play starts through `concordia_fracture::load`. That loader is new, and `fracture.inspect` now uses it too.
- **What the tests show:**
  - an unstrained breakable stays one compound body, with the structure's mass;
  - a weak blow leaves damage but no break;
  - a blow on the middle cut splits exactly there, and a second blow finds nothing more to break;
  - each part moves as it did inside the whole (v + ω × r, same ω), and linear momentum is kept. Angular momentum follows from the parts moving rigidly with the same ω, so the test doesn't measure it separately;
  - anchored parts stay static while the loose half falls;
  - a budget of one break per tick spreads three breaks over three ticks;
  - poses and pieces are identical at 1, 2 and 8 threads and on a rerun;
  - through the Engine API, a fractured slab falls, breaks on the ground in play, and the edit world's slab is untouched.
- **Test log:**
  - **Not test-first:** I wrote the `breaking` tests before the code, but first ran them after the system existed, and all 7 passed. Run against a no-op system, all 7 failed.
  - The engine test also passed on its first run. With the system left out of the play schedule, it failed.
  - **Contract consumers:** app, API, MCP and Studio tests (471 passed, including the fracture crate's). The editor schemas are unchanged, since their project has no `[destruction]`.
  - **Checks:** fmt, clippy.

**8.4a** (done): fields, part one.
- **`Field`** is an ordinary component: undoable, edited like any other, placed by its entity's `Transform`.
  - Shapes: sphere, box, plane, everywhere.
  - Falloffs: none, linear (over a sphere's radius or a box's half diagonal), and 1 / (1 + d²).
  - Optional duration.
- **Effects:**
  - `Strain`: magnitude per second.
  - `Crumble`: breaks every bond of what it touches.
  - `Anchor`: pins the covered pieces, so their fragment becomes static.
  - `Kill`: removes debris; the intact breakable is never debris.
- **When they act:** at one fixed point of `destruction.break` (after setup, before impacts, in id order).
- **Methods:** `destruction.crumble {entity}` and `destruction.apply_strain {center, radius, amount}` for agents, play only. They answer "nothing is playing" otherwise.
- **Split into 8.4b:** `force`, `impulse`, `sleep` and `disable`. The physics runtime only reads `Velocity` when it creates a body, so these need a new shim call that sets velocity or adds an impulse on a live Jolt body. Cloth reading force fields stays with 8.9.
- **Test log:**
  - The six `fields` tests were written first and failed against an empty field pass, then passed.
  - The two engine tests were written before the methods but first run after they existed: fields as undoable components, and agents crumbling and straining a slab.
  - The breaking tests' harness moved to `tests/common` for reuse.
  - **Contract consumers:** app, API, MCP and Studio (453 passed with destruction).
  - **Checks:** fmt, clippy.

**8.4b** (done; **Phase 8.4 done**, apart from cloth reading force fields, which waits for 8.9): pushing live bodies.
- **`jolt-sys`:** `cj_body_add_impulse` and `cj_body_set_active`; bindings regenerated.
- **`concordia-physics`:** `Impulse` (one-shot, dynamic only), `Sleep` (one-shot) and `PhysicsDisabled` (body out of the world while present). They are applied in entity order after the body sync.
- **Field effects:**
  - `Force`: an impulse of magnitude × weight × dt each tick;
  - `Impulse`: magnitude × weight once, on the field's first tick;
  - `Sleep` and `Disable`.

  Pushes act at the fragment's centre of mass, away from the field's centre or along a plane's normal. Every field now ages in `FieldAge`, so a field knows its first tick.
- **What the tests show:**
  - an impulse changes velocity once by impulse / mass, and is consumed;
  - an angular impulse spins;
  - a sleep request stops a moving body;
  - a disabled body leaves the world and returns, falling;
  - a force field accelerates the slab steadily (1 m/s² on 2000 kg);
  - an impulse field kicks it once;
  - sleep and disable fields act on what they cover.
- **Test log:**
  - The physics `pushes` tests were written first and first run after the runtime code. With the runtime ignoring the components, all four failed.
  - The four new field tests failed against unhandled effects, then passed.
  - **Contract consumers:** editor schemas regenerated (`Impulse`, `Sleep`, `PhysicsDisabled`); editor UI 128 tests; jolt-sys, physics, destruction, nav, API, app, MCP, Studio and scene (607); the physics, replay and gate integration tests. At first 55 of 56 passed: the Lamplight showcase failed only because this worktree had never built Lamplight's modules (`just lamplight-build`), and it failed the same way on main. Once they were built, all 40 Lamplight tests passed.
  - **Checks:** fmt, clippy.

### Effort, revised
About **18 weeks (15–23)**, down from 20: rendering, instancing, compute and replay groundwork already exist, but 8.0 is new (about 1.5 weeks). The cutter (8.1), the solver (8.8) and weight painting (8.10, needs visual design) remain the large items.

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
- **8.3 refinement (2026-11-28; my calls under the M8 autonomy grant):**
  - **S1:** a new crate, `concordia-destruction`, with a `[destruction]` module that runs only in play.
  - **S2:** `Breakable { recipe, anchors, impact_radius }` is authored on an entity that has a `Transform` and a `RigidBody`. When play starts, the entity becomes one body: a compound of its pieces' hulls, with its mass from the structure. It also gets a `Fragment { pieces, damage, broken }`, which lives in the world, so it is hashed and saved with play state.
  - **S3:** pieces and structure come through a `BreakableSource` trait. The project's source reads `.breakable.ron` and the DDC; tests use an in-memory one.
  - **S4: strain.** A `ContactImpact` on a fragment damages each of its internal edges by the impact's strength × (1 − d / `impact_radius`). Here d is the distance from the impact to the midpoint of the edge's two piece centres. An edge breaks when its damage reaches its strength.
  - **S5: splitting.** Union-find over the unbroken internal edges.
    - Each component becomes a new entity whose id is derived from its parent and its lowest cell, so ids don't depend on threads or order.
    - It keeps the parent's transform, so body space stays mesh space.
    - Its velocity is the parent's at the component's centre of mass (v + ω × r), so momentum is conserved.
  - **S6:** components holding an anchored cell are static; the rest become dynamic.
  - **S7:** at most `breaks_per_tick` edges break per tick (default 64), sorted by fragment then edge. The rest break on later ticks.
  - **S8:** cluster levels and their damage thresholds aren't used at runtime yet; fields (8.4) use them. In 8.3 the whole breakable starts as one cluster.

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
