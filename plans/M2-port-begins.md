# M2: Engine breadth (formerly "Port begins")

> **Renamed by ADR-0048 and restructured by ADR-0067** (refinement, 2026-11-19). Phases 2.1, 2.2 and 2.9 (Godot formats, GDScript translation, the playable slice) and every Godot-specific mapping in 2.3–2.7 move to M5. Terra's Mass-style core moves here from M7. The refinement section below wins wherever the older phase text disagrees. The file keeps its old name so links stay valid.

Status: In progress: 2.0, 2.0b (2026-11-19), 2.0c, 2.6 (2.6a–2.6d), 2.8, 2.3a, 2.3b and 2.3c (2026-11-20), the 2.3c follow-ups and 2.3d (2026-11-21) done; 2.4 next · Depends on: M1, M3's Studio track (done) · Detail level: refined at the milestone level; each phase is refined again at its start, against the code as built · Estimated effort: ~29–36 phase-weeks (see the refinement)

## Refinement (2026-11-19, against M0, M1 and the M3 Studio track as built)
**Status: author decisions D1–D8 taken, and the phase list approved (2026-11-19).**

### What exists (checked in code)
- **Terra** (`concordia-terra`): the archetype store, queries and a fixed-step `SimLoop`.
  - The `SimClock` has no time scale.
  - A `Schedule` runs systems in declared order, with parallelism only inside systems.
  - There are no processors with access declarations, no processing phases (sim vs presentation), no events, timers or observers, no command buffers, no shared or chunk components, no tags and no templates.
  - The world hash and thread-count determinism tests exist.
- **Physics:**
  - `CharacterController`/`CharacterMotion` over Jolt;
  - contact begin/end events published once per tick;
  - `physics.raycast`;
  - sensors are partial: colliders have `is_sensor`.
- **Input:** pointer rays from the cursor through the camera (`pointer.rs`), plus `input.inject`.
- **glTF import:** meshes, prefabs, and skins and clips stored "for M2's animation system" (`anim.rs`). Nothing samples or skins them yet.
- **Render:**
  - instance and material buffers, clustered forward, CSM and IBL;
  - one PBR material model with view modes;
  - the editor viewport is engine-drawn (ADR-0047).
- **Not present:** `wit/`, `concordia-script-*`, `-anim`, `-nav`, `recast-sys`, `-audio`, `-ui`, `-replay` and `-import-fbx`.
- **Studio** (M3): the gameplay programmer, animator, VFX artist, sound designer, composer and UI/UX roles are disabled in the default team, each waiting for an M2 feature (refinement G10 there). The skills `concordia-record-replay`, `concordia-fix-bug-with-replay` and `concordia-synthesize-sfx` carry "not available yet" notes.
- **Downstream needs:**
  - M3 3.1 generalizes M2's input log;
  - M3 3.9's fixture needs a WASM gameplay script with hot reload;
  - M6 needs both scripting hosts;
  - M8 garments need skinning.

### Author decisions (2026-11-19)
- **D1. Godot-specific work moves to M5.** That's the gdshader→Slang translator, Godot `.import` and `StandardMaterial3D` mapping, AnimationTree import, the `NavigationAgent3D`/`NavigationRegion3D` mapping, `Control`→UI prefab import and sound-bank cues. M2 builds the engine-native features they will map onto, tested with our own fixtures.
- **D2. Gate: an original sample game** in `examples/` that uses every M2 subsystem. The deterministic gate drives it headless in public CI; an optional live gate has the Studio team change it on the author's subscription. Its design is settled with the author at the start of its phase, and it is our own game, not modeled on any existing one.
- **D3. Terra's Mass core moves here from M7** (phases 7.1–7.6): processors with access declarations and phases, parallel deterministic scheduling, shared and chunk components, tags, command buffers, templates and traits, simulation and representation LOD, signals and observers, spawners and representation switching. Crowds and traffic (7.7) join the navigation phase. M7 keeps large-scale performance, AI-driven behavior (after M4) and its gate. As with ADR-0010, this is a clean room: built from Epic's public Mass documentation and talks only.
- **D4. A scripting system, but no Gameplay Ability System:** abilities, effects and attribute sets in the GAS sense are a non-goal. Gameplay is written as scripted processors (tier 1 native, tier 2 WASM) over Terra.
- **D5. Materials follow Unreal's Substrate model** (slabs with BSDF features, layering, mixing and coverage operators, a per-pixel closure budget), defined as text (`.material.ron`) first. Node-graph authoring waits for M4's graphs. The model replaces M1's PBR material, with a migration, and runs early so later phases use it. Clean room: built from Epic's public Substrate documentation and the SIGGRAPH 2023 Substrate course material only.
- **D6. Order: unblock dependents first** (below).
- **D7. Editor scope:** API and MCP tools plus engine-drawn viewport debug views (skeletons, navmesh, LOD buckets, audio emitters, UI bounds), and inspector widgets for new components. No new editor panels.
- **D8. Each phase unlocks its Studio role:** enable the role in the default team, widen its tool scope, finish its skill (removing the "not available yet" note), and update the sync and roster snapshots, all with tests.

### Gaps (assumed by the old plan, not built)
| # | Gap | Resolution | Phase |
|---|---|---|---|
| G1 | No processing phases, events, timers or time scale; the old plan assumed M0 had them | Terra core (D3): phases (`PrePhysics`, `Physics`, `PostPhysics`, `Sim`, `Presentation`), timers, `SimClock::time_scale` | 2.0 |
| G2 | Godot mappings inside 2.3–2.7 depend on the importer that moved to M5 | Move to M5 (D1); M2 keeps the native targets | all |
| G3 | M2's gate was the private port slice | Sample-game gate (D2), public CI | 2.10 |
| G4 | M4 4.13, M5 and M7 cite M2 content that moved | Notes in those plans (this refinement) | n/a |
| G5 | Studio roles and skills wait on M2 | D8, in every phase | all |
| G6 | `concordia-interchange` v0 was planned for M2 (2.1) | Moves to M5 with the Godot importer; M5 5.1 builds v1 directly | n/a |
| G7 | Sensors (`Area3D`-style enter/exit) aren't exposed as gameplay events | Physics contact events become Terra signals | 2.0b |
| G8 | Representation LOD needs instanced rendering of many entities | Uses M1's instance buffer; the GPU golden test moves with 7.6 | 2.4b |
| G9 | Determinism with a parallel scheduler (old risk) | Conflict-graph scheduling and thread-count hash tests in every Terra phase | 2.0 |

### Phases (new order, D6)
The numbers 2.1, 2.2 and 2.9 are retired: they moved to M5, and reusing them would muddle the history.
| Order | Phase | Content | From | Unlocks |
|---|---|---|---|---|
| 1 | **2.0 Terra core I: processors and phases** | processors with access declarations; processing phases; deterministic parallel scheduling (conflict graph); `SimClock::time_scale`; timers | M7 7.1, G1 | everything |
| 2 | **2.0b Terra core II: storage, commands, signals** | shared and chunk components; tags; command buffers applied at phase ends; signals (next-tick delivery, entity order) and add/remove observers; physics contacts as signals | M7 7.2, 7.5, G7 | |
| 3 | **2.0c Templates, traits and spawners** | `.template.ron` traits and templates; batch spawn; spawners with seeded distributions; `terra.*` API and MCP tools (`spawn_batch`, `despawn_batch`, `stats`, `inspect`) | M7 7.3, 7.6 (spawning), 7.8 (tools) | level-designer scopes |
| 4 | **2.6 Scripting tiers** | WIT world, WASM host, guest SDK, native hot reload; scripted processors in Terra's phases; `script.reload` | old 2.6 | gameplay-programmer; M3 3.9; M6 |
| 5 | **2.8 Replay (minimal)** | input log, `.replay.ron` v0, checkpoints, divergence report | old 2.8 | M3 3.1; replay skills |
| 6 | **2.3 Substrate-style materials and FBX** | the material model (D5) replacing M1 PBR, with migration; goldens per feature; FBX via ufbx | old 2.3 (Godot parts out), D5 | tech-artist scope |
| 7 | **2.4 Animation** | skeletons, clips, GPU skinning, blend and state-machine graphs (`.animgraph.ron`), events, attachments, tweens | old 2.4 (AnimationTree import out) | animator |
| 8 | **2.4b Terra LOD and representation** | simulation LOD (deterministic inputs only); representation LOD and switching (full, instanced, hidden) | M7 7.4, 7.6 (representation), G8 | |
| 9 | **2.5 Navigation, crowds and traffic** | recast-sys, navmesh build and queries, agents, Detour crowds as Terra processors, lane-based traffic (rule-based; AI-driven behavior stays M7/M4) | old 2.5 (Godot mapping out), M7 7.7 | |
| 10 | **2.7 Audio and in-game UI** | audio stack and cues; retained ECS UI, taffy, cosmic-text (ADR-0060) | old 2.7 (Godot import and sound bank out) | sound-designer, composer (synth in M3 3.7), ui-ux |
| 11 | **2.10 Sample game and gate** | an original sample game (D2), the deterministic gate, an optional live Studio gate | D2 | |

The VFX artist stays disabled: particles are not in M2 (open question, unchanged).

**New-phase tests (first lists; each phase is refined again at its start):**
- **2.0:**
  - `conflicting_processors_never_run_concurrently`;
  - `independent_processors_run_in_parallel`;
  - `declared_order_constraints_are_respected`;
  - `schedule_cycle_is_an_error_naming_the_cycle`;
  - `world_hash_identical_for_1_and_8_threads_with_parallel_processors`;
  - `presentation_phase_never_changes_world_hash`;
  - `time_scale_scales_sim_dt_deterministically` (hit-stop);
  - `timers_fire_on_the_tick_they_expire_including_large_steps`;
  - the M0 ECS benchmarks within `BASELINE.md`.
- **2.0b:**
  - `shared_component_is_stored_once_per_archetype_value`;
  - `chunk_component_is_per_chunk_and_survives_moves`;
  - `tags_filter_queries_without_storage`;
  - `deferred_spawns_apply_at_phase_end_in_deterministic_order`;
  - `model_proptest_with_command_buffers_matches_the_model`;
  - `signal_raised_this_tick_is_delivered_next_tick_in_entity_order`;
  - `observers_fire_on_add_and_remove_once`;
  - `sensor_enter_and_exit_arrive_as_signals`.
- **2.0c:**
  - `template_composes_traits_into_an_archetype`;
  - `trait_conflicts_are_reported_with_both_traits`;
  - `template_roundtrips_byte_identical`;
  - `batch_spawn_of_100k_from_template_is_deterministic`;
  - `template_schemas_come_from_reflection`;
  - `spawner_places_count_with_seeded_distribution`;
  - `terra_methods_have_generated_schemas_and_mcp_tools`;
  - `agent_spawns_from_a_template_over_mcp`.
- **2.3 (materials):**
  - `slab_bsdf_features_match_reference_values` (diffuse, specular, haze, fuzz, subsurface, thin film; CPU reference);
  - `vertical_layering_attenuates_the_bottom_slab` (energy conservation);
  - `horizontal_mix_and_coverage_blend_closures`;
  - `closure_budget_downgrades_gracefully_and_reports_it`;
  - `m1_pbr_materials_migrate_to_one_slab_unchanged_in_golden`;
  - a golden per feature;
  - `material_files_roundtrip_canonical`;
  - FBX tests as in the old 2.3.
- **2.4b:**
  - `far_entities_tick_at_their_lod_rate`;
  - `lod_transitions_are_hysteresis_damped`;
  - `simulation_lod_depends_only_on_deterministic_inputs`;
  - `representation_lod_follows_the_camera_without_affecting_world_hash`;
  - `representation_switches_between_full_instanced_and_hidden`;
  - `signals_survive_lod_changes` (from M7 7.5; moved here from 2.0b because LOD lands here);
  - `instanced_representation_draws_100k_in_one_pass` (GPU, golden).
- **2.5 (added):**
  - `crowd_of_10k_agents_avoids_overlap` (perf on the reference machine);
  - `traffic_follows_lanes_and_yields_at_junctions`;
  - `crowd_is_deterministic_across_runs_and_threads` (with Terra's parallel scheduler; this replaces the old "single-threaded crowd" constraint).
- **2.10:** set with the sample game's design. Its shape: scripted headless play driven through `input.inject`, `sim.step` and `query.run`; replay across thread counts; goldens; zero error logs; every Studio role that M2 unlocks lists at least one tool it uses in the sample.

### Phase 2.0 As built (2026-11-19, ADR-0063)
- **Processors** (in `concordia-terra`, not a new crate): `processor(name, phase)` gives a `ProcessorDecl`, with `reads`, `writes`, `writes_presentation`, `after`, `before` and `run`. The `Processor` trait and `View` (`query`, then `for_each`/`par_for_each`) complete it. The phases are `PrePhysics`, `Physics`, `PostPhysics`, `FrameEnd` and `Presentation`; the refinement's separate `Sim` phase became `PostPhysics`, as Mass names it.
- **Scheduling:**
  - Order comes from a topological sort of the constraints, ties broken by registration order. Every conflicting pair then follows that order, and each processor runs in the first wave after its predecessors.
  - A wave runs in parallel on the `JobSystem`, each processor seeing only its declared columns. Undeclared or read-only-as-mutable queries are `QueryError::Undeclared`/`ReadOnly`.
  - No `unsafe`: views are disjoint borrows of the archetype columns (ADR-0030 stands).
  - Change ticks are applied at the wave barrier.
- **Exclusive systems still work:** `Schedule::add` puts one in `PostPhysics`, and `add_in(phase, ..)` names the phase. Input sampling moved to `PrePhysics` and the physics step to `Physics`.
- **`SimLoop::new(rate, schedule, &mut world)`** returns `Result<_, ScheduleError>` (cycle, unknown target, duplicate name, and the presentation rules). It also adds the built-in `terra.timers` processor and creates the `SimClock`. `SystemCtx` gains `dt_exact`.
- **Presentation state:** `writes_presentation` or `World::register_presentation` marks a component presentation-only. It is left out of the hash; only `Presentation` processors may write it, and other phases may not use it.
- **Time:**
  - `SimClock` gains `time_scale` (`TimeScale`: finite, at least 0) and `time`; `dt` is the step times the scale.
  - `Timer::once`/`repeating` count down scaled simulated time and fire on the tick they expire, several times in one tick if needed.
  - Both clock fields are hashed. Edit worlds have no clock, so command-journal hashes are unchanged and the hash domain stays `v1`.
- **Tests:** `tests/processors.rs` (12) and `tests/timers.rs` (4), all in `concordia-terra`.
  - They follow the planned 2.0 list, plus unknown and duplicate names, phase order, undeclared access, the presentation rules, exclusive systems and invalid durations.
  - The planned `presentation_phase_never_changes_world_hash` is the 5th; the 2.0b items stay for 2.0b.
- **Red run:**
  - Against stubs, 13 failed for the right reason.
  - Three passed. Two were vacuous (the stub ran no processors), so I strengthened them to check that the processors changed state, and both then failed. The third, `exclusive_systems_still_run_with_the_whole_world`, is the backward-compatibility guard.
- **Changed deliberately:**
  - **The presentation test** now registers `Glow` as presentation in both runs. Presentation-ness belongs to the type, so `World::register_presentation` was added; before, it depended on whether a processor declared it.
  - **The M1 gate's pinned play-world hash** changed because of the new clock fields. Its golden image still matches. A domain bump was tried and reverted: it would have changed edit-world hashes and made every project's command journal stale.
  - **The `SimLoop::new` callers** were updated: the API's play start (an invalid schedule is now an API error), the determinism tests and the scene builtin test.
- **Not done here:**
  - The criterion benchmarks and `just perf-smoke` couldn't be judged against `BASELINE.md`: during this phase the machine was under heavy load (`ReportCrash` at 99% CPU, load average 14–17). `perf_smoke_iterate_1m_transform_velocity` measured 39 ms against its 25 ms budget, but main's code measured the same 39 ms then, so it isn't a regression. Re-measure on a quiet machine before 2.0b.
  - Resource access in views, and structural changes from processors (command buffers), come with 2.0b.

### Phase 2.0b As built (2026-11-19, ADR-0069)
- **Author decisions:**
  - typed signals in a world-level queue;
  - logical chunk components that aren't hashed, with real chunked tables recorded as a later performance update;
  - command buffers applied at phase end;
  - the perf fix as this phase's first step.
- **Perf fix first.** Phase 2.0's perf result wasn't machine load. On a quiet machine the perf smoke still failed (36.7 ms against 25 ms), and every write query was 40–200× slower than the M0 baseline.
  - The cause was M1 1.9e's change tracking: one `BTreeMap` lookup per entity a write query visits.
  - The fix keeps one change tick per archetype. Two new guard tests (joining and leaving a written archetype) pin the semantics.
  - The numbers before and after are in `crates/concordia-terra/benches/BASELINE.md`. All write benches are back at baseline.
- **Storage kinds:**
  - Tags are any zero-sized type, with no column.
  - Shared components use `register_shared`, values interned by canonical hash and archetypes keyed by value. They are read-only in queries.
  - Chunk components are logical: `CHUNK_ROWS` = 1024, and they aren't hashed.
  - Every kind hashes like a dense component. `storage_kinds_never_change_the_world_hash` pins a hash computed with the pre-2.0b storage, so edit-world hashes and command journals are unchanged.
  - Processor access to chunk components waits for its first consumer, 2.4b.
- **Commands:**
  - `View::commands` and `CommandBuffer`, plus `par_for_each_deferred` with one buffer per piece.
  - Buffers apply at the end of their phase in plan order. Spawn ids come from `DeterministicIdGen` seeded by (tick, phase).
  - Commands on dead targets are skipped and counted in `TerraStats`. Presentation processors get no buffer.
- **Signals:** they are delivered at the start of the next tick, by entity and then raise order. Pending signals are hashed, and only when there are some. `ViewQuery::for_each_of` reaches the signaled entities' components.
- **Observers:** `observer::<T>(name, Observe::Add | Remove)` runs after each simulation phase on net changes, in up to 8 rounds. It also sees direct changes made by exclusive systems.
- **Resources in views:** `reads_resource` and `writes_resource` take part in conflicts, and resources are lent per wave without `unsafe`.
- **Physics:** `CollisionEvents`, `CollisionEvent` and `ContactKind` are replaced by the `ContactBegan` and `ContactEnded` signals on both entities.
  - `sensor_reports_enter_and_exit_events_once` was rewritten deliberately as `sensor_enter_and_exit_arrive_as_signals`. It is the same check, plus delivery on the next tick to both entities.
- **Refactor:** `View` and `ViewQuery` moved to `view.rs`, and the two query fetch paths became one, where a `&T` element reads either rows or one shared value.
- **Tests:**
  - `tests/storage.rs` (7), `commands.rs` (8), `signals.rs` (8, observers included) and `resources.rs` (4);
  - `model_proptest_with_command_buffers_matches_the_model`;
  - two change-tracking guards;
  - the physics sensor test.
  - Every planned 2.0b test is there. `signals_survive_lod_changes` moved to 2.4b.
- **Red run:**
  - Against stubs, 26 of the new tests failed for the right reason.
  - Three passed vacuously, because the stubs did nothing. I strengthened them: the determinism test now also checks that observers fired, signals arrived and an entity was despawned; the by-name test checks storage kinds; the chunk-hash test checks the value was stored. All three then failed.
  - The two change-tracking guards passed at once by design; they protect semantics, while the perf smoke was this step's red test.
- **Perf after 2.0b:** see `BASELINE.md`.
  - The tag path first made `add_remove_component_10k` 4.38 ms. A fast path for zero-sized values and cached shared edges brought it to 2.97 ms, 11% above before.
  - `spawn_10k` stays 34% above M0, from M1 1.9e's per-spawn touch, which is left for later.
- **API counts unchanged:** 34 core methods, 31 MCP tools. The `terra.*` tools come in 2.0c.

### Phase 2.0c As built (2026-11-20, ADR-0070)
- **Author decisions:**
  - Traits are Rust types (reflected parameters, required processors), and `.template.ron` files compose them.
  - Terra templates replace `concordia-api`'s `EntityTemplate`.
  - Spawners live in the edit world, and batch spawns act on the play world only.
  - A new crate, `concordia-terra-sim`; the `terra.*` methods are core.
  - The MCP tool-list budget rises from 12k to 16k tokens (amends ADR-0033).
- **Format:**
  - `concordia-scene` gains `read_template`, `write_template` and `TemplateDoc`: canonical, validated against the registry, and byte-identical on round trip. `concordia fmt` formats templates too.
  - Templates live in `templates/**/*.template.ron`, named by file stem.
- **`concordia-terra-sim`:**
  - `EntityTrait`, `register_trait` and `TemplateBuilder` (`add`, `add_shared`, conflicts naming both traits).
  - `Template` and `CompiledTemplate` (`spawn`, `spawn_batch`, `missing_processors`) and `TemplateCatalog`.
  - `Spawner` with the `Grid`, `Box` and `Disc` distributions. They're seed-only and use no trigonometry. Spawned ids come from the spawner's seed and entity id.
  - The built-in `Movement` trait (adds `Transform` and `Velocity`, requires `integrate_velocity`).
- **API:**
  - `entity.templates` lists built-ins and project templates, with their traits, components and any `problem`.
  - `entity.create` spawns any template, and a template's `Name` is the default entity name.
  - Spawners expand when play starts. Templates that need a missing processor are refused, naming it.
  - New methods: `terra.spawn_batch`, `despawn_batch`, `stats` (counts, skipped commands, the phases and waves) and `inspect` (archetype, chunk, storage kinds, pending signals).
  - Counts: 38 methods and 35 MCP tools. `tools/list` is about 14.0k tokens.
- **Terra additions:** `World::entity_storage`, `Schedule::waves` / `observers`, `SimLoop::schedule` and `ReflectComponent::register_shared`.
- **Studio (D8):** the level designer gets the `terra_*` tools, `edit`/`write` for `templates/**`, and the new skill `concordia-terra-templates`. The roster, sync and M3S gate snapshots were updated after review.
- **Agent guide:** a new "Templates, spawners and crowds" section, and the method table.
- **Tests:**
  - `concordia-terra-sim`: `templates.rs` (10, including `concordia_fmt_formats_templates_canonically`, written after the implementation, so it passed at once: a guard for the `concordia fmt` claim) and `spawners.rs` (5);
  - `concordia-api`: `tests/terra.rs` (6);
  - `concordia-mcp`: `tests/terra_tools.rs` (2);
  - Studio: the level-designer scope test;
  - the count updates in `method_schemas`, `tools`, `stdout_hygiene` and the M0 gate.
  - Every planned 2.0c test is there.
- **Red run:**
  - 23 new tests failed against stubs for the right reason. Three existing tests also failed as expected knock-on effects: the method-index snapshot, the `Spawner` category, and the token budget.
  - The count and generated-schema tests passed against the stubs, because the stubs really register the methods. The existing hierarchy tests passed as `entity.create` regression guards.
- **Fixes during green:**
  - Four of my test mistakes: `component.add` takes no value (now `component.set`); query results are `rows`; an over-complicated id setup; and the trait list now includes the built-in `Movement`.
  - `TemplateError` boxes its `SceneError` (clippy, large error variant).
- **Not in this phase:** processor timings in `terra.stats` (M7), `terra.set_debug_view` (M7), and the `spawn_10k` regression noted in 2.0b. The 2.0b perf numbers still hold, because 2.0c adds no hot-path code.
- **CI:** a new advisory, RUSTSEC-2026-0587 (`wasmtime-wasi` 50, dev-only and unreachable here), fails `just deny` on every branch. A documented ignore is in its own PR (#35).

### Phase 2.6 refinement (2026-11-20; ADR-0061, ADR-0062)
This replaces the test list and implementation notes of the older "Phase 2.6" text below, which was written before Terra's core.

**Author decisions:**
- **WASM stays, compiled ahead of time.** Wasmtime precompiles each component to native code, cached in `.concordia/ddc`, so nothing is JIT-compiled or interpreted at runtime. (The C++ and C# alternatives were weighed: C++, Zig and C# NativeAOT fit the native tier's C ABI later.)
- **The scripting host is swappable.** The contract is a protocol: module, processor, component, signal and trait descriptors, wire-encoded batches, and run outputs. WIT is just the WASM binding of it. `concordia-script` holds a `ScriptBackend` trait, and every engine-side feature is written once against it. A conformance suite runs on every backend, including an in-process test backend.
- **Scripts define their own components, signals and traits.** Terra and the reflect registry gain dynamic, name-keyed types, with their schema taken from the module's `describe()`.
- **The native tier speaks the same protocol** over a C-ABI vtable. It is built in 2.6 too.
- **WASM instances are fresh per processor run**, from a precompiled `InstancePre` with the pooling allocator, so no hidden guest state outlives a run. A benchmark measures the cost.
- **Four sub-phases**, each its own commit and PR to main.
- **Wasmtime** is pinned at `=50.0.3`, which patches RUSTSEC-2026-0587, so the `deny.toml` ignore goes (51 is still a release candidate). *Updated at 2.6b's start: 51.0.0 had become stable, so it is pinned `=51.0.0` instead (author's decision).*

**How the old plan maps onto Terra:**
- **Systems → processors.** `describe()` declares processors: name, phase, component reads, writes and presentation writes by stable name, received signals, and before/after. They join Terra's conflict graph and deterministic waves (ADR-0063).
- **"Commands applied at the end of the system" → 2.0b's `CommandBuffer`,** applied at phase end in plan order. A run returns its column writes and commands together, so a trap applies nothing.
- **"Events" → 2.0b's typed signals.** Delivered signals arrive in the batch, by stable name and entity order. Raised signals are commands.
- **The `guest_commands_apply_through_command_log_with_script_actor` test conflicts with ADR-0032,** because the play world is never journaled. It becomes two tests:
  - `guest_commands_apply_at_phase_end_in_plan_order`;
  - `script_reload_journals_edit_world_migration_with_script_actor`: migrating script components in the edit world is one undoable transaction by `script:<module>`.

  Play-world faults and log lines are attributed to `script:<module>`.
- **Traits and spawners.** Modules declare traits (components with defaults, plus required processors) that `.template.ron` files can use. A `spawn-template` command spawns from a template.
- **`cargo component` → plain `cargo build --target wasm32-wasip2`** (ADR-0021).
- **Exclusive systems stay Rust-only:** they need the whole `World`, which can't cross the wire.
- **Time and RNG ride in the tick context.** Each processor gets an RNG seed derived from the session seed, the tick and its name, so processors never contend for `SimRng`. The SDK does transcendental math with `libm`, so the WASM and native builds give bit-identical results.

**2.6a: dynamic types in reflect and Terra; the wire codec**
- `concordia-reflect` gains:
  - `TypeDesc`, a serializable mirror of `TypeInfo`;
  - `DynamicValue`, which serializes like a static type of the same shape;
  - dynamic registrations by name, which can be replaced on reload;
  - `wire`, the columnar codec: little-endian, a fixed stride per type derived from its `Shape`, and strings, lists, maps and data-carrying enums in a side arena;
  - migration between two versions of a type.
- `concordia-terra` gains:
  - dynamic components and signals keyed by stable name;
  - processor access by name;
  - a by-name `View` path that encodes the declared columns to wire batches and writes rows back;
  - by-name commands.
- Tests, `concordia-reflect`:
  - `type_desc_roundtrips_type_info`;
  - `dynamic_value_serializes_like_the_equivalent_static_type` (JSON and the canonical hash);
  - `dynamic_type_registers_by_name_with_schema_and_validation`;
  - `dynamic_type_name_collision_is_an_error`;
  - `reregistering_a_dynamic_type_replaces_its_schema`;
  - `wire_layout_has_a_fixed_stride_from_the_shape`;
  - `wire_roundtrips_every_shape` (proptest);
  - `wire_strings_and_lists_live_in_the_arena`;
  - `wire_encoding_is_canonical_little_endian` (pinned bytes);
  - `static_and_dynamic_values_encode_identically_on_the_wire`;
  - `wire_decode_rejects_malformed_input_without_panicking`;
  - `migration_fills_added_fields_with_defaults_and_reports_removed_ones`.
- Tests, `concordia-terra`:
  - `dynamic_component_stores_queries_and_hashes_by_name`;
  - `dynamic_component_hashes_like_the_equivalent_static_type`;
  - `dynamic_and_static_components_share_an_archetype`;
  - `named_access_conflicts_like_typed_access`;
  - `named_view_encodes_columns_and_writes_rows_back`;
  - `named_view_rejects_undeclared_names`;
  - `named_commands_insert_remove_and_signal`;
  - `dynamic_signals_deliver_next_tick_in_entity_order`;
  - `named_processors_hash_identically_at_1_and_8_threads`.
- Tests, `concordia-scene`: `scene_with_dynamic_components_roundtrips_byte_identical`.

**2.6a As built (2026-11-20, ADR-0062):**
- **`concordia-reflect`:**
  - Field and type defaults became `DefaultValue` (a function, or stored JSON). `TypeRef::to` references an already-built info. Both changes let a runtime-described type carry the same metadata as a derived one.
  - `TypeDesc` (`TypeInfo::to_desc` / `TypeDesc::to_info`) and `register_dynamic`. A re-registration replaces the type by name and keeps its attached data. Built-in scalars, `Any`, `EntityId` and the glam types resolve even when they aren't registered.
  - `DynamicValue`, stored as canonical wire bytes, and `migrate`.
  - The `wire` module: a layout cache; an encoder that is a serde `Serializer` guided by the shape; a decoder that is a serde `Deserializer` and also validates; a transcoder that serializes a value as its Rust twin; and a canonical copier.
- **`concordia-terra`:**
  - Component identity moved from `TypeId` to stable names in access, presentation, observers and views. All 83 existing Terra tests passed unchanged after this step.
  - Dynamic columns and tags, and `World::insert_dynamic`. The typed and dynamic inserts share one dense path and one shared path.
  - Named access: `NamedQuery`, `Batch`, `View::batch` / `write_batch` / `signal_batch` / `signals_named`, and by-name commands. `write_batch` decodes everything, then assigns, so a bad row writes nothing.
- **Tests:**
  - `concordia-reflect`: `tests/dynamic.rs` (6) and `tests/wire.rs` (8, including two proptests);
  - `concordia-terra`: `tests/dynamic.rs` (9);
  - `concordia-scene`: `tests/dynamic.rs` (1; a dynamic twin of a Rust type saves byte-identical scene text).
  - Every planned 2.6a test is there. Two were added: `wire_decode_never_panics_on_arbitrary_bytes` and `dynamic_values_are_wire_bytes_with_their_type`.
- **Red run:**
  - 23 of 24 failed against stubs for the right reason: `to_desc`, `register_dynamic` and the wire layout were unimplemented, and named access was a no-op.
  - `wire_decode_never_panics_on_arbitrary_bytes` passed vacuously, because the stubs never decoded anything. I strengthened it to mutate real encodings and require the unmutated bytes to validate, and it then failed.
- **Changed deliberately during green:**
  - The never-panics test asserted "valid on the wire ⇒ decodes into the Rust type". That's wrong: `EntityId` checks UUID versions, which wire validation (like `validate_json`) leaves to the type. It now asserts "valid ⇒ readable as a dynamic value".
  - `ColumnWriter::finish` returns a `Result` (a row left unwritten is an error), so the call sites unwrap.
  - `View::signal_batch` takes the signal's `TypeInfo`, because an empty batch still needs its type.
  - The misuse test dropped a filter on an unknown name: a world can't tell a typo from a type it has never stored.
- **Existing tests, mechanical updates:**
  - `derive.rs` calls `DefaultValue::get`;
  - `registry.rs` uses the `deserialize` method, and a refutable `let` for the new error variants;
  - the zoo proptest strategies moved to `tests/common/arb.rs`, shared by the schema and wire tests.
- **Fixes found by the tests:**
  - serde's `IgnoredAny` reads every enum variant as a newtype, so the decoder accepts that for any payload;
  - descriptions resolve the built-in names.
- **Perf:** the perf smoke passes. The name keys only touch planning paths.
- **Not in this phase:**
  - Shared storage for script types: they are dense or tags.
  - Migrating stored values when a type's version changes in a world (2.6c/2.6d).
  - The Wasmtime pin and the `deny.toml` ignore move with the WASM host (2.6b).

**2.6b: the protocol, the backend seam, WIT, the guest SDK and the WASM backend (ADR-0062)**
- **Crates:**
  - `concordia-script`: the protocol types, `ScriptBackend`, `ScriptProcessor`, the conformance suite and the test backend;
  - `concordia-script-guest`: the SDK, which builds for both native and `wasm32-wasip2`;
  - `concordia-script-wasm`: the WASM backend;
  - `wit/concordia-game/`.
- **The WIT world** exports `describe` and `run(processor, tick-context, batch) -> result<run-output, string>`, and imports only `log`. That is one crossing per processor per tick.
- **Determinism:**
  - Cranelift with NaN canonicalization, no threads, and deterministic relaxed SIMD;
  - fuel per run, with no epoch interruption;
  - a memory cap;
  - WASI virtualized: clocks are sim time, random comes from the processor's stream, and there is no filesystem, env or stdio (stdout goes to the log).
- **AOT:** precompiled components are cached in the DDC, keyed by the wasm hash, the Wasmtime version and the config.
- **Tests:**
  - `wit_world_gameplay_validates_with_wasm_tools`;
  - `guest_component_builds_for_wasm32_wasip2`;
  - the conformance suite, run per backend:
    - `describe_registers_processors_components_signals_and_traits`;
    - `batch_delivers_1000_matches_columnar_in_one_call`;
    - `boundary_crossings_per_tick_are_constant_in_entity_count`;
    - `guest_commands_apply_at_phase_end_in_plan_order`;
    - `signals_reach_guests_and_guests_raise_signals`;
    - `fault_is_contained_reported_and_processor_disabled`;
    - `script_trait_composes_into_a_template`;
    - `script_spawns_from_a_template`;
    - `script_processors_hash_identically_at_1_and_8_threads`;
  - `test_backend_passes_conformance`;
  - `fuel_exhaustion_traps_deterministically`;
  - `nan_canonicalization_makes_float_results_bit_identical`;
  - `wasi_clocks_and_random_are_virtualized_to_sim`;
  - `guest_statics_do_not_survive_between_runs`;
  - `precompiled_component_is_cached_and_reused`;
  - `wasm_reload_preserves_world_state`.
- **Benchmark** (criterion, recorded in `BASELINE.md`): the same logic run native, as WASM without guards, as WASM with fuel, and as WASM with fuel and NaN canonicalization, at 10k and 100k entities. Fuel granularity is chosen from these numbers.

**2.6b As built (2026-11-20, ADR-0062):**
- **Author decisions at the start:**
  - Wasmtime 51.0.0 had become stable, so it is pinned `=51.0.0` (not 50.0.3). The M-1 gate passed unchanged, and the `deny.toml` ignore is gone.
  - The protocol types got their own small crate, `concordia-script-protocol` (reflect and core only), so guests don't compile Terra or the scene crate. `concordia-script` re-exports them.
- **Crates:**
  - `concordia-script-protocol`: descriptors, `TickContext`, `Batch`, `RunOutput`, `Command`, `ModuleDesc::validate`, and packed ids.
  - `concordia-script`:
    - `ScriptBackend`, and `ScriptModule` (`run(&self)`, so there is no guest state);
    - `ScriptHost` (load, processors, faults, logs, reload) and `ScriptProcessor`;
    - `register_module`, `processor_seed`, `TestBackend`;
    - the conformance suite behind the `conformance` feature (`conformance_suite!`, `Rig`, `run_all`).
  - `concordia-script-guest`: `Module`, `RunCtx`, `processor()`, `describe_type`, `export_module!` (wit-bindgen with `pub_export_macro`, `wasm32` only) and `math` (libm).
  - `concordia-script-wasm`: `WasmBackend`, `WasmModule`, `WasmConfig` and `Fuel`.
  - The WIT world is `wit/concordia-game` (`concordia:game@0.1.0`).
  - The conformance module, `tests/fixtures/script/conformance-guest`, is a workspace member. It builds as an rlib, and `just wasm` builds it as `conformance_guest.wasm` and `conformance_guest_v2.wasm`.
- **Additions elsewhere (each test-first):**
  - `ReflectComponent::from_wire` (Terra): a script command can carry any component, Rust or script-defined.
  - `DynamicTrait` and `register_dynamic_trait`, plus `CompiledTemplate::record` (terra-sim). `ReflectTrait` now holds a shared closure instead of a function pointer, so a trait can carry data from a description.
  - `SimRng` implements `rand_core::TryRng` (core), so it can be WASI's random source.
  - `concordia-reflect`'s stride check uses `u32::try_from`, because clippy for `wasm32` (now in `just lint`) flagged the old comparison as always false on 32-bit.
  - Wasmtime is built without default features. Its `cache` feature pulled in MPL-2.0 code that `cargo deny` rejects, and the DDC caches instead. Without the `threads` feature, wasm threads are compiled out rather than switched off.
- **Tests:**
  - The 9 conformance tests run per backend (`concordia-script/tests/conformance.rs` and `concordia-script-wasm/tests/conformance.rs`), plus `test_backend_passes_conformance`.
  - All 8 WASM tests are in `concordia-script-wasm/tests/wasm.rs`.
  - Added: unit tests for the protocol (4), the SDK (7) and the seed (2); `reflect_component_decodes_wire_values_for_static_and_dynamic_types` (Terra); 4 in terra-sim's `tests/dynamic_traits.rs`; and `sim_rng_is_a_rand_core_rng_with_the_same_stream`.
- **Red run.** Every new test failed against stubs for the right reason, except these:
  - Passed at once:
    - `a_well_formed_description_validates` is a guard, and the stub's `Ok` satisfied it.
    - The SDK's `log_lines_go_to_the_sink` and `the_rng_is_the_engine_seeded_stream` test two-line bodies that were already real in the skeleton.
  - `test_backend_passes_conformance` first checked only the backend's refusals and passed vacuously. It was strengthened to run the whole suite (`run_all`), and then failed.
  - `wit_world_gameplay_validates_with_wasm_tools` and `guest_component_builds_for_wasm32_wasip2` check artifacts that were already in place: the WIT was validated with `wasm-tools` as it was written, and `just wasm` had built the guest. Both were shown to fail on a world with an extra import and on a missing artifact.
- **Changed deliberately during green:**
  - A WIT interface of types alone is listed as an `import`, so the WIT test asserts the imports are exactly `host` and `types`, and that `types` declares no functions.
  - A tag type's `null` default turns into `None` on its JSON round trip, so script traits give unit-shaped (tag) components their unit value without a default.
  - The fault test's first tick is tick 0 (`SimClock::tick` before the step).
  - The fuel test's constants were calibrated. 1M `conf.burn` iterations use 12–15M fuel, so four runs pass on a 20M budget only if fuel is per run.
- **Benchmark (`crates/concordia-script-wasm/benches/BASELINE.md`):** 
  - **One tick, native and then WASM:**
    - 10k entities: 9.1 ms native; WASM 11.5 ms without guards, 14.2 ms with fuel, 14.5 ms with fuel and NaN canonicalization.
    - 100k entities: 91.5 ms native; WASM 107 ms, 141.5 ms and 139.7 ms.
  - All guards together cost about 1.55–1.6×. Fuel is about +30%, and NaN canonicalization is free.
  - A fresh instance costs about 21 µs per processor per tick, so fresh instances stay.
  - Fuel: the integrator needs 12–13k per row, so the default budget is 10M per run plus 50k per row.
  - The wire path dominates, natively too (about 0.9 µs per entity). A typed fast path for fixed-layout columns is the next optimization.
- **Not in this phase:**
  - Reloads that change processors or types; the API and `concordia.toml` wiring (2.6c, 2.6d).
  - The native backend (2.6c).
  - A typed fast path for fixed-layout columns.

**2.6c: the native backend (ADR-0061)**
- **`concordia-script-native`:**
  - The `concordia_module_v1` C-ABI vtable: ABI version, `describe`, `run`, `free`.
  - Copy-on-load with `libloading`. Swaps happen at tick boundaries, and old libraries stay loaded until play stops.
  - A static-link path.
  - A `syn`-based source check that rejects mutable statics.
  - Panics are caught and treated like traps.
  - All `unsafe` stays in this crate.
- **Tests:**
  - the conformance suite on the native backend;
  - `native_module_registers_types_and_processors_by_stable_name`;
  - `native_reload_swaps_behavior_without_losing_world_state`;
  - `component_layout_change_on_reload_migrates_via_reflect`;
  - `reload_with_incompatible_abi_version_is_refused`;
  - `module_with_static_mutable_state_is_rejected_by_the_check`;
  - `guest_panic_is_contained_like_a_trap`;
  - `static_link_and_dylib_paths_produce_identical_world_hash`.

**2.6c As built (2026-11-20, ADR-0061):**
- **Author decision at the start.** A C ABI has two sides, so "all `unsafe` stays in `concordia-script-native`" became: the ABI's one definition and the guest-side FFI glue go in a new crate, `concordia-script-abi`; host-side `unsafe` stays in `concordia-script-native`. The guest SDK has no `unsafe`, and there is no crate cycle (native → script → SDK → abi).
- **Crates and additions:**
  - `concordia-script-abi`: `ModuleV1`, the version and table symbols, `Buffer`, `Entry`, `guest::table` and `export_entry!`. Panics are caught at the boundary.
  - `concordia-script-native`:
    - `NativeBackend`: the dylib path (copy-on-load, ABI checked first, old libraries kept until the backend drops), and the static path (2.6b's in-process backend);
    - `NativeModule`;
    - `check_module_source` (`syn` with span locations).
  - `concordia-script-protocol::codec`: the binary run envelope (call and reply). Descriptions cross as JSON.
  - SDK: `export_module!` also emits the native exports when the module crate's own `dylib` feature is on. `native::Adapter<M>` is a `Module` as an ABI `Entry`.
  - Terra: `World::migrate_dynamic` and `migrate_dynamic_all`. They convert stored values, tag values and pending signals, staged across all types, then committed.
  - `concordia-script`:
    - `ScriptHost::reload` takes the world and returns a `ReloadReport`;
    - changed types are migrated, with rollback;
    - processors refresh their type table and templates every run;
    - `set_templates`.
  - Fixtures:
    - the conformance module gains `dylib` and `layout2` features;
    - `bad-abi-guest` claims ABI version 999;
    - `just native-fixtures` builds into its own target directory, and is run by `just test` and the regression workflow;
    - `concordia_tests::native_fixture_path`.
- **Tests:**
  - All 7 planned native tests, plus the conformance suite on the dylib path (`concordia-script-native/tests/`).
  - Added:
    - 4 ABI tests;
    - 4 codec tests, including a never-panics proptest;
    - 3 Terra migration tests;
    - 4 in `concordia-script/tests/reload.rs`: migration and its field report, unchanged types, rollback, and changed processors;
    - `missing_native_fixture_error_names_the_fix`.
- **Red run.** Every new test failed against stubs for the right reason, except these:
  - `reload_refuses_changed_processors` is a guard on 2.6b behavior and passed at once.
  - `unchanged_types_are_not_migrated` passed vacuously. I strengthened it to insert a value after the reload, and it then failed. That exposed a 2.6b bug: every reload re-registered every type, so later inserts through the registry hit "a different version is stored". Only changed types are re-registered now.
  - `migrate_dynamic_all_is_atomic_across_types` first failed to compile (the API was missing). Its first green run then showed that pending signals of a type never stored as a component weren't converted; that's fixed.
  - `missing_native_fixture_error_names_the_fix` was written with the helper refactor, and I didn't see it fail first.
- **Changed deliberately during green:** the reload test expected bare field names, but reflect's `MigrationReport` lists JSON Pointer paths (`/max`), which also cover nested fields. `TypeMigration` keeps reflect's format.
- **Seen once, not reproduced:** the first combined run had 4 timeouts while the machine's load average was 52. Reruns were clean.
- **Not in this phase:**
  - reloads that change processors (schedule rebuilds) and running the source check before `script.reload` (2.6d);
  - unloading old libraries (by design; ADR-0061).

**2.6d: API, samples and the Studio role**
- **Engine:**
  - The `[scripts]` section in `concordia.toml`.
  - `script.reload` (MCP `script_reload`), which reloads what's on disk and does not build. That makes 39 methods and 36 tools.
  - Sample logic under `examples/`, built as WASM, as a native dylib and statically.
- **Studio (D8):** the gameplay-programmer role is enabled, with `script_reload`, `scripts/**` and a scripting skill, and the snapshots are reviewed.
- **Tests:**
  - `script_reload_over_api_is_logged_with_its_actor`;
  - `script_reload_journals_edit_world_migration_with_script_actor`;
  - `every_backend_runs_the_sample_logic_to_identical_hashes` (1 and 8 threads);
  - `editing_a_native_module_during_a_headless_session_changes_behavior_after_reload` (the done criterion);
  - the method and tool counts and the token budget;
  - the gameplay-programmer scope test.

**2.6d As built (2026-11-20, ADR-0061, ADR-0062):**
- **Author decisions at the start:**
  - Scripts are an engine module (`concordia-scripting`), enabled by `[scripts]`, like Studio. The core stays at **38 methods and 35 MCP tools**, and `[scripts]` adds `script.reload` (36 tools), rather than the 39 and 36 core the plan expected. This keeps Wasmtime and the native loader out of projects that use no scripts.
  - The edit-world migration is a new **`EditOp::MigrateType`**, fully undoable and replayable. Applying, undoing, redoing and replaying commands now take `&mut TypeRegistry`.
- **Engine:**
  - `concordia-scripting`:
    - `ScriptsModule` (loads at engine start; a failing module is logged and skipped);
    - `ScriptsConfig` and `ScriptEntry`, and `link::<M>()` for the static path;
    - `script.reload` (edit-world migration as `script:<module>`, then the play world, with undo as rollback; logged as `script reloaded`);
    - an exclusive `scripts.emit_logs` system that logs script lines and faults in a deterministic order.
  - `concordia-api`: the `start_play_in(engine, world)` hook; `Engine::commit_edits_as`, `undo_as`, `registry_and_play_mut`, `template_catalog` and `sim_seed`; the error kind `ScriptFailed` (−32018).
  - `concordia-app`: `[scripts]` with `[[scripts.module]]` `kind`/`path`/`source`; the paths must stay inside the project; the module is added last.
  - `concordia new` writes an empty `[scripts]`.
  - `concordia-command`: `MigrateType`, and its exact inverse (remove, return to the old version, set every old value).
  - `concordia-reflect`: the registry keeps every version of a dynamic type, so re-registering a known description, with the same dependency versions, restores that exact version; `TypeRegistration::desc`. The 2.6c rollback no longer converts worlds back.
  - `concordia-terra`: `World::stores`.
  - `concordia-script`:
    - `emit_logs`: processors on worker threads only record, so the log order is the same at any thread count;
    - `enable_all` (each play session) and `changed_types`.
  - Sample: `examples/m2-scripting` (`scripts/wanderers` uses the processor RNG, libm trigonometry, column writes, commands, signals and a script trait in a template; a spawner places 200). `just wasm` builds it into the example's `build/`; `just native-fixtures` builds its library.
- **Studio (D8):**
  - The gameplay programmer is enabled, with `script_reload`, `cargo build *` in bash, `scripts/**` and the new `concordia-scripting` skill. The director may now spawn it.
  - Roster, sync and M3S gate snapshots were reviewed and updated. The MCP test projects mirror `concordia new` with `[scripts]`.
- **Tests:**
  - The plan's tests in `concordia-scripting/tests/scripting.rs`:
    - `script_reload_over_api_is_logged_with_its_actor`;
    - `script_reload_journals_edit_world_migration_with_script_actor`, with undo and redo;
    - `every_backend_runs_the_sample_logic_to_identical_hashes` (linked, WASM and native, at 1 and 8 threads);
    - `editing_a_native_module_during_a_headless_session_changes_behavior_after_reload` (the done criterion: the library on disk is replaced by a rebuilt one, then reloaded).
  - Counts and budget: `binary_serves_script_reload_when_scripts_are_configured` (36 tools, under 16k tokens; the budget part was retired by ADR-0073), and `scripts_module_registers_module_types_and_adds_script_reload` (core plus 1).
  - Scope: `gameplay_programmer_writes_and_reloads_scripts_with_the_scripting_skill`.
  - Added:
    - `concordia-command/tests/migrate.rs` (4);
    - 2 in reflect (version cache, dependencies);
    - 2 in Terra (`migrate_dynamic_all` atomicity, `stores`);
    - 3 in `concordia-script` (`changed_types`, `enable_all`, emit order);
    - 2 in the API (`start_play_in`, `commit_edits_as`/`undo_as`) and 1 for the error kind;
    - 2 in the app (`[scripts]`) and 1 for the example;
    - 4 more scripting-module tests (failed loads, the hidden-state refusal, faults across sessions, module types);
    - the `cli_new` sections.
- **Red run.** New tests failed against stubs, except these:
  - Passed at once:
    - `a_migration_that_cannot_convert_changes_nothing`, a guard the stub satisfied;
    - `start_play_in_sees_the_engine_registry_and_seed`, whose hook wiring went in with its stub;
    - `binary_serves_script_reload_when_scripts_are_configured` and `m2_scripting_example_spawns_and_runs_its_wanderers`, integration checks written after their parts.
  - `script_failed_has_its_own_code_and_a_reload_hint` was written with its variant, and I didn't see it fail first.
- **Found during green:**
  - Script log lines and faults were emitted from worker threads. Thread-local subscribers lost them, and their order depended on timing. They are now recorded, then emitted in a deterministic order.
  - A migration's undo can't convert values back when a field without a default was removed, so the inverse restores exact values instead.
  - The examples' build folder had matched the workspace glob, so it moved to `examples/m2-scripting/build/`.
  - `log.tail` returns 100 records by default; the sample test asks for 10,000.
  - The editor's e2e suite caught a regression: `concordia studio init` on a project without `[scripts]` failed. The default gameplay programmer's `script_reload` matched no tool. `studio init` now adds `[scripts]` when it's missing, just as it adds `[studio]` (`studio_init_adds_scripts_when_missing_for_the_gameplay_programmer`, red first).
- **Not in this phase:**
  - Reloads that change processors (schedule rebuilds).
  - Migrating scene files saved before a module changed a layout; reload while the scene is open instead (ADR-0061).
  - A rollback test for the edit step when the play step fails (the second step can't fail independently with today's fixtures).

### Phase 2.8 As built (2026-11-20, ADR-0072)
This replaces the older "Phase 2.8" text below, which was written before Terra and scripting.

**Reconciled with the code as built:**
- **"Hash restricted to Sim components":** presentation-only components were already excluded (ADR-0063), and resources such as `ActionStates` are never hashed, so nothing more was needed. A divergence report has to name what differs, so Terra gained `World::state_digest`: the unchanged world hash plus one hash per part (each component by name, and `@entities`, `@clock`, `@rng`, `@signals`) in the same pass.
- **"Game module hashes (native + wasm)":** `EngineModule::fingerprints` lets any module say what code it runs. The scripts module reports each loaded module by the BLAKE3 of its file, or of its description when linked. The header also holds the engine version, the scene and its edit-world hash, the seed, the rate and (for information) the thread count.
- **`hot_reload_of_identical_module_mid_replay_keeps_hash`** became `hot_reload_of_identical_module_mid_recording_keeps_hash`: `script.reload` of the same build during a recording leaves every checkpoint unchanged, and the replay (with no reload) matches. A reload that changes the code makes `stop` refuse with `ReplayMismatch`.
- **Placement:** an engine module, `concordia-replay`, enabled by `[replay]` (the `[scripts]`/`[studio]` precedent), so projects without it keep their tool set. `concordia new` writes it, and `studio init` adds it. That makes 35 core MCP tools plus 2.
- **Inputs:** recorded after binding, as each tick's changed `ActionState`s (just-pressed and just-released flags included). Raw pointer positions reach the simulation only through actions, so they need no track. Playback is `InputSampler::play_back` in `concordia-input`, never a system that overwrites `ActionStates`, so it doesn't depend on module order.
- **Checkpoints:** Terra's new end-of-tick hooks (`Schedule::add_tick_end`, run after the clock advances) make each checkpoint equal `sim.step`'s hash.

**Engine:**
- `concordia-replay`:
  - `ReplayModule`; `replay.record` (`start` with a path and `checkpoint_every`, default 60, or `stop`) and `replay.run` (`verify` or `resimulate`, `force`).
  - `.replay.ron` v0 (`Replay`, `ReplayHeader`, `InputChange`, `Checkpoint`), canonical through `concordia-scene`'s document writer. The reader checks the version, hashes and tick order.
  - A divergence is a result (`ok: false`, with the checkpoint tick, the last matching tick and the differing parts). A header mismatch is the new error `ReplayMismatch` (−32019). The replayed play world stays for queries.
- `concordia-terra`: `StateDigest`, `World::state_digest`, `TickEnd`, `tick_end`, `Schedule::add_tick_end` / `tick_end_names`.
- `concordia-input`: `ActionStates::changed_since`, `InputSampler::play_back`.
- `concordia-api`: `Fingerprint` and `EngineModule::fingerprints`; `Engine::fingerprints`, `restart_play`, `step_play` (now behind `sim.step` too), `scene_path`, `threads`, `sim_rate` and `project_file` (now behind the scene path check too).
- `concordia-scripting`: per-module fingerprints, updated on reload.
- `concordia-app`:
  - `[replay]`.
  - **`Project::engine_config`**. Found during this phase: `[sim] rate_hz`/`seed` and `[journal] sync` never reached any engine built from a project. Only the scripts module read the seed, so a project with `seed = 7` played with seed 0. The CLI (`run`, `headless`, in-process commands), the MCP server (`Options::base`) and the editor now all start from it. The five copies of "load project, set modules" became one.
- CLI: `concordia replay record <path> --ticks N [--inputs events.json] [--scene] [--checkpoint-every]` and `concordia replay run <path> [--mode] [--force] [--threads]`. `run` prints the result and exits 1 on a mismatch or divergence.
- Agent guide: a "Replays" section, the two methods, and the error codes. `script.reload` and −32018 `ScriptFailed`, both missed in 2.6d, were added too.

**Studio (D8):**
- `replay_*` for the QA playtester, level designer and gameplay programmer (M3 3.1's follow-up).
- QA preloads `concordia-record-replay`, and its prompt asks for a replay as evidence.
- Both replay skills now use the real tools and arguments. Their notes keep only what is still to come: `ab` and bisection (M3 3.1), `assert_eval` (3.2), and `input_inject` without an `[input] map`.
- Sync and M3S gate snapshots were reviewed and updated; they change only the three scopes, QA's skill and the prompt lines.

**Tests:**
- `concordia-replay`: `format.rs` (2), `record_run.rs` (10) and `scripts.rs` (3).
- `concordia-terra/tests/digest.rs` (5), `concordia-input` playback (2), `concordia-api` (4 engine helpers, 1 error kind), `concordia-scripting` fingerprints (2).
- `concordia-app` (2), `cli_replay`, the `cli_new` section list, `studio_init_adds_replay_when_missing_for_the_qa_playtester`.
- `concordia-mcp`: `replay_tools.rs` (2), plus 2 in `stdout_hygiene`.
- Studio: the QA scope test and `replay_skills_use_the_real_replay_tools_and_arguments`, which parses every example call in the skills as the real params type.
- `tests/integration/replay_m1_physics.rs` (3; the done criterion).
- **The plan's tests, as built:**
  - `replay_file_is_canonical_ron_v0` (insta, reviewed);
  - `replay_reproduces_world_hash_at_every_checkpoint`, as `recorded_m1_physics_session_replays_bit_identically` (M1's physics stack, a hash every 60 ticks, final hash equal to `sim.step`'s) and `replayed_input_moves_entities_exactly_as_recorded`;
  - `replay_detects_divergence_and_reports_first_tick_and_component` (a perturbation on tick 130 reports checkpoint 140, last match 130, part `Transform`);
  - `replay_header_mismatch_is_refused_unless_overridden` (code, scene hash, seed; `force` lists them and diverges at tick 0 on `Name`);
  - `replay_is_identical_across_thread_counts` (1 and 8);
  - `replay_runs_headless_faster_than_realtime`;
  - `hot_reload_of_identical_module_mid_recording_keeps_hash`;
  - `replay_record_and_run_over_mcp`.
- **Added:** input changes only on changed ticks, resimulate, unknown actions, logging with actors, stop without a recording, path checks, the header's script builds, the refusal after a code change, and `engine_config`.

**Red run:** every new test failed first for the right reason (stubs: an empty digest, hooks dropped, `not implemented yet`, an error code of −32603, no fingerprints, no playback, no `[replay]`, no `replay` subcommand). These passed at once:
- The three `replay_m1_physics` tests and `binary_serves_replay_tools_when_replay_is_configured` are integration checks written after the parts they join. They aren't vacuous: the physics test asserts the bodies fell, and the crate's divergence test shows the comparison fails when it should.
- `replay_methods_have_generated_schemas_and_mcp_tools`, for the same reason.

**Found in review:** if a play world failed to start after `replay.record start`, the queued recording joined the next, unrelated play world. `a_recording_whose_play_world_failed_to_start_does_not_join_a_later_one` reproduced it with the fix reverted, and the fix clears the queued recording.

**Found during this phase:**
- **Performance.** The first interpreter took 130 ms on `many_lights`, against M1's 6.5 ms. It re-evaluated view-only terms per light (table reads for the diffuse share, the sheen albedo and both transmittances) and copied the closure array into every call. Now the view-only terms and the light-independent factors (F0, GGX alphas, diffuse albedo × share / π) are computed once per closure. Light-side transmittance runs only under a vertical layer (`FLAG_LAYERED`, set by the encoder). A lit one-slab material stays in registers, a featureless slab takes `eval_plain`, and other programs use a thread-private closure array. Measured on the same Mac the same day, `main` is 2.46–2.63 ms best and 2.5–3.2 ms median; 2.3b is 2.90–2.94 ms best and 2.9–3.2 ms median, about 15–18% slower for one-slab materials. The comparison and goldens are unchanged, and the mutations were re-run on the final shader. Removing the rest needs per-feature pipeline variants, which is left for M4's GPU work.
- **A Metal miscompile.** With the closures in the thread-private array (or passed `inout`), the mask test dropped every masked fragment, though the values it compared were right: alpha 0.78 against a cutoff of 0.5, read back as color. The glTF alpha-blend golden caught it. Passing the array by value fixes it but costs about 40×. The mask test now runs first, from the program's alpha alone, before any closure is stored. A throwaway test isolated it, and `masked_materials_keep_fragments_at_or_above_the_cutoff` keeps it: it fails on the miscompiled shader. The root cause (Slang's emitted MSL or Metal's compiler) is not pinned down; it is noted in `concordia-render`'s AGENTS.md.

**Changed deliberately during green (my test mistakes):**
- The rig's `Transform` and `Velocity` need every field.
- At ticks 30 and 50 only `camera_pan`'s value changes, so ticks 31 and 51 rightly record nothing.
- Log fields are text (`"true"`).
- The no-journal check compares against the history before recording, because the rig's own spawns are journaled.
- Roster scopes are loaded with globs expanded.
- The in-process MCP test server needs the project's modules passed in.

**Tool-list budget (an open question for the author; answered by ADR-0073, see below):** the core list is still about 14k tokens, under ADR-0070's 16k. But a project with `[input]`, `[physics]`, `[assets]` and `[scripts]` was already about 18.9k before this phase, and is about 20.5k with `[replay]`, because the budget test only covers the core set. The replay tools were trimmed from about 1.9k to 1.6k, and `replay_tools_add_under_1700_tokens_to_a_full_project` guards their share.

**Not in this phase:** snapshots, bisection to tick, entity and field, `ab` runs, raw device events, and reloads recorded mid-session (M3 3.1); assertions (3.2).

### Per-role tool scopes (2026-11-20, ADR-0073)
The author's answer to 2.8's open question: no agent ever gets the whole tool list, so there is no budget on it. Budgets are per role, and only the admin grants tools, in the roster.
- **Roster:** every default role names its tools; there are no globs. Each tool is one its prompt or preloaded skills use, and each tool they tell it to use is in scope.
- **Director:** keeps its task tools (create, list, get, update, comment), `studio_*` and `txn_history`. It loses the eight world-inspection tools and `concordia-task-workflow`, and it now reviews the evidence specialists attach to tasks.
- **Specialists:** the level designer drops `replay_*` (nothing it does records replays) and keeps `scene_info` and `entity_spawn`, which its prompt now uses. The tech artist gains `scene_open`, `scene_save` and `sim_stop`, and the gameplay programmer gains `scene_open` and `sim_stop`. `concordia-team-comms` no longer names `query_run`, and it shows every role `studio_questions`.
- **Sizes, before → after:**

  | Role | Tools | Tokens |
  |---|---|---|
  | director | 25 → 13 | ≈20.8k → 12.5k |
  | gameplay-programmer | 42 → 16 | ≈22.6k → 12.4k |
  | level-designer | 51 → 35 | ≈28.3k → 20.4k |
  | qa-playtester | 28 → 22 | ≈21.2k → 17.6k |
  | tech-artist | 24 → 21 | ≈16.1k → 14.0k |
  | writer | 12 → 11 | ≈10.6k → 9.6k |

- **Tests:**
  - Retired: `tool_list_token_budget`, `replay_tools_add_under_1700_tokens_to_a_full_project`, and the 16k assertion in `binary_serves_script_reload_when_scripts_are_configured`.
  - Added: `every_enabled_default_role_stays_under_its_tool_budget` (`concordia-mcp`, 22k per role), `default_role_scopes_list_tools_by_name_not_by_glob`, `every_scoped_tool_is_used_by_the_role_prompt_or_its_skills`, `every_tool_a_role_skill_tells_it_to_use_is_in_scope` and `director_coordinates_with_task_team_and_history_tools_only`.
  - Changed: the QA replay test now requires that the level designer has no replay tools. M3S gate calls that used tools a role no longer has now use ones it has: the level designer's `task_list` became `task_get` and `schema_get`, and the tech artist's `component_set` became `component_patch`. The director's approval attempt is now refused by the hook's scope check, since it has no `task_approve`; the gate-party rule stays covered by `done_requires_gate_actor`. The M3S agents snapshot changes only the `tools:` lines.

### Phase 2.3 refinement (2026-11-20, ADR-0068)
Refined against the code as built. M1 has one flat `MaterialData` (in `concordia-assets`) packed into five shader rows and one `brdf()` in `forward.slang`. `concordia-reflect` has no `Box`, so the operator tree is stored as a flat map of named nodes. The editor has no material view (the inspector shows components only), and the tech artist is already enabled, so D8 here means updating its skill, prompt and scope. There are 27 render goldens, all on M1 PBR.

**Author decisions (2026-11-20):**
- **All six features** in 2.3: diffuse, specular, haze, fuzz, subsurface (a forward-only approximation; screen-space diffusion in M4) and thin film.
- **Closure budget:** a platform default that a project may lower in `concordia.toml` but never raise.
- **FBX last** (2.3d), as planned.
- **Editor:** selecting a `.material.ron` in the File System dock shows it in the existing Inspector dock (no new panel), edited through new Engine API methods `material.get` / `material.set` (file writes, logged with the actor, not undoable, like `asset.import`).
- **Exact energy conservation** over M1 pixel parity. The white-furnace grid found that M1's diffuse coupling gains up to 11% at grazing views. Migrated materials change slightly; goldens that move beyond tolerance are reviewed and re-blessed in 2.3c.

**Sub-phases (each its own branch and PR to main):**
- **2.3a Material model** (done, below).
- **2.3b Renderer** (done, below):
  - closure rows and the postfix program in the material buffer, and a closure loop in `forward.slang` with IBL per closure;
  - the sheen and direct-lobe albedo tables in the albedo texture;
  - the importer and `AssetSource` switch to `Material`, and the budget is fitted at upload;
  - `[render] closure_budget` (lowering only), and `render.stats` reporting downgrades;
  - view modes for closure count and per feature.
  - Tests: `shader_matches_cpu_reference_per_feature` (a sphere rendered unlit-by-IBL against `eval_slab`), a golden per feature, `closure_budget_downgrades_gracefully_and_reports_it` end to end, `project_cannot_raise_the_closure_budget`, and the view-mode goldens.
- **2.3c Migration, editor, Studio** (done, below):
  - Tests: `m1_pbr_materials_migrate_to_one_slab_unchanged_in_golden` (every M1 golden re-run through one slab within its tolerance; any that moves is reviewed), `material_get_and_set_roundtrip_canonical_text`, `material_set_is_attributed_and_not_journaled`, inspector widget tests for struct-variant enums and named-node maps, a Playwright test editing a slab, and the tech-artist scope and skill tests with sync snapshots;
  - `MaterialData` is retired, and glTF materials import as one slab.
- **2.3d FBX** (done, below): `ufbx-sys` and `concordia-import-fbx` (meshes, skins, clips into the glTF importer's asset types), licensing through `just deny`, and fixtures from a script. Tests: the five FBX tests of the older 2.3 section.

### Phase 2.3a As built (2026-11-20, ADR-0068)
- **`concordia-assets`:**
  - `Material`, `MaterialNode` (`Slab`, `Vertical`, `Horizontal` with `mix_texture`, `Coverage`), `Slab` (M1's fields plus `specular`, `transmittance`, `haze`, `fuzz`, `subsurface`, `thin_film`), `MaterialError`;
  - `read_material` (layered text, or M1 text migrated to one slab `surface`), `write_material`, `Material::from_pbr`, `validate`, `reachable`.
  - The importer still produces `MaterialData` until 2.3b.
- **`concordia-material` (new):**
  - `eval_slab`, `slab_transmittance`, `thin_film_reflectance`;
  - `MaterialProgram` (postfix ops on a stack) with `Shaded`;
  - `ClosureBudget` (1..=8), `fit_budget` → `ClosureReport`;
  - `dfg`: the split-sum table (moved from `concordia-render`, which re-exports it), the direct-lobe albedo table and the sheen albedo table; `sampling`.
- **Tests:**
  - `concordia-assets/tests/material_model.rs` (8, two insta snapshots reviewed: canonical car paint and the schema).
  - `concordia-material`: `bsdf.rs` (8), `layering.rs` (8), `energy.rs` (5), `dfg.rs` (4), `budget.rs` (6).
  - **The plan's tests, as built:**
    - `slab_bsdf_features_match_reference_values`, split into one test per feature, against an independent Python implementation;
    - `vertical_layering_attenuates_the_bottom_slab`;
    - `horizontal_mix_and_coverage_blend_closures`;
    - `closure_budget_downgrades_gracefully_and_reports_it` (on the CPU here; end to end in 2.3b);
    - `material_files_roundtrip_canonical`.
  - **Added:** `white_furnace_energy_never_exceeds_one` and `layered_stacks_never_exceed_one` (proptests), `bright_extremes_stay_within_one`, `a_bright_clear_coat_over_a_white_base_stays_within_one`, `m1_material_file_reads_as_one_slab`, `from_pbr_keeps_every_m1_field`, `node_cycles_and_missing_names_are_errors_naming_them`, `out_of_range_and_non_finite_values_are_rejected_with_their_path`, `downgrading_a_clear_coat_keeps_the_coat_as_haze`, `merging_slabs_with_different_textures_reports_the_dropped_one`, `direct_albedo_matches_the_lobe_eval_slab_uses`, and alpha, emission, closure-count and table tests.
- **Red run:** every test failed first for the right reason (stubs returning zero, empty programs, no validation, no migration). These passed at once:
  - Guards: `default_material_is_one_default_slab` (data definitions), `an_invalid_material_does_not_compile` and `an_invalid_material_is_refused` (validation already built).
  - Strengthened because they passed vacuously against zero stubs: the normalization test, `opaque_top_hides_the_bottom`, `horizontal_mix_and_coverage_blend_closures`, the lookup clamp test, and both furnace proptests (each now requires non-zero energy).
- **Found by the tests:**
  - **Mutation check:** removing diffuse's Fresnel attenuation was not caught by the furnace proptest, because random slabs rarely reach the bright corner. The deterministic extremes grid catches it, and also the same mutation of the final rule.
  - The extremes grid found a **NaN** in the sheen at head-on views (N·V + 1e-5 > 1 into Λ's `powf`), now clamped.
  - It also found that **M1's own BRDF gains energy at grazing views** (1.107). The author chose exact conservation (above).
  - **Table mismatch:** the split-sum IBL table's separable Smith term under-states the direct lobe's albedo at grazing angles (0.116 vs 0.156), which left 4% excess. Energy splits now use a direct-lobe table, and the IBL table is unchanged (no GPU change in 2.3a).
  - **A coarse grid in my own test** (uniform in cos θ) under-resolved a sharp lobe at grazing angles. The test now uses converged Python values.
- **Changed deliberately during green:** `fit_budget` returns `Result` (it refuses invalid materials rather than panicking), and the reference values were regenerated by the oracle after the energy decision.

### Phase 2.3b As built (2026-11-20, ADR-0068)
**Reconciled with the code as built:**
- **"Fitted at upload":** the renderer re-reads materials from its `AssetSource` every frame; nothing was uploaded. `Programs` (`programs.rs`) fits and compiles a material when it is first drawn or its content changes, and refits everything when the budget changes. A downgrade is logged once and listed in every frame's `render.stats` that draws it.
- **The albedo texture existed only with an environment.** The new diffuse split needs the direct-lobe table for every lit material, so the albedo tables are uploaded on the first frame. They form one 64×32 texture: the split sum and the direct lobe's (A, B) in the left half, the sheen albedo in the right. The shader reads each half at texel centers like the CPU tables, so IBL is unchanged.
- **Gaps the plan didn't name, settled in ADR-0068 decision 9:**
  - the GPU needs a mix texture, so `Op::Horizontal` carries it;
  - layered unlit materials and the albedo view needed a definition, which became `MaterialProgram::unlit`;
  - IBL per closure needed one too: the program runs once more for the environment, with transmittance toward the mirror direction;
  - ambient diffuse gets the same 1 − E_spec(N·V) split as direct light.
- **glTF materials import as one slab now** (moved from 2.3c), because a material artifact can have only one format. `MaterialData` remains as the migration source and the glTF intermediate.
- **Budget:** Metal defaults to 8 (author). `[render] closure_budget` outside 1..=8 is refused when the project loads; above a lower platform default it is ignored with a warning.
- **The plan's test against `eval_slab` on a sphere** became flat tiles at 18 orientations under an orthographic camera and one sun. Every sampled pixel then has an exact normal, view and light; a tessellated sphere's interpolated normals would not.
- **View modes:** `closures`, `haze`, `fuzz`, `subsurface`, `thin_film`. Normals and roughness/metallic show the first closure. The editor's View menu lists them, and `tsc` fails if the menu and the engine's `ViewMode` differ (`VIEW_MODES_MATCH_THE_ENGINE`).

**Engine:**
- `concordia-material`: `MaterialProgram::unlit`; `Op::Horizontal { mix, mix_texture }`.
- `concordia-assets`:
  - `MaterialImporter` v2 reads `read_material` text into `Material`;
  - `load_material` and `BuiltinAsset::Material` hold `Material` (the built-in is M1's white dielectric as one slab);
  - `From<MaterialData> for Material`.
- `concordia-import-gltf` v2: one-slab `Material` artifacts.
- `concordia-render`:
  - `forward.slang` loads up to 8 closures per pixel (textures applied) and runs the postfix program on a stack: per light, for IBL, and for the albedo (mask, unlit, albedo view). It mirrors `eval_slab`, `slab_transmittance`, `thin_film_reflectance` and the operators;
  - `material_rows.rs`: a header row, one row per op, eight rows per closure; at most 64 ops; `DrawPush.material` is the first row;
  - `ibl::albedo_table()`;
  - `platform_closure_budget`, `effective_closure_budget`;
  - `Renderer::{closure_budget, set_closure_budget, closure_reports}`;
  - `RenderModule::with_closure_budget`;
  - `FrameStats.{closure_budget, downgraded_materials}` (`MaterialDowngrade`, `ClosureMergeStats`, `MergedOperator`);
  - five `ViewMode`s.
- `concordia-app`: `[render] closure_budget`, `Project::closure_budget`.
- Agent guide: the new view modes and `render.stats` fields.
- No new methods, error codes or MCP tools, so role scopes are unchanged.

**Tests:**
- `concordia-material/tests/unlit.rs` (4).
- `concordia-assets/tests/material.rs`: 3 added; the M1 import test now expects the one-slab migration.
- glTF import tests read the slab.
- `concordia-render`:
  - `material_rows` unit tests (6, including the `FLAG_LAYERED` header flag);
  - `tests/budget.rs` (2);
  - `albedo_table_holds_the_split_sum_the_direct_lobe_and_sheen`;
  - `tests/materials.rs` (19, including `shader_matches_cpu_reference_at_grazing_light`: light from beside the tiles, so Fresnel dominates);
  - `closure_budget_downgrades_gracefully_and_reports_it` in `capture.rs`;
  - the capture schema snapshot (reviewed: it adds exactly the five view modes).
- `concordia-app` (2); the editor's view-mode test.
- **The plan's tests, as built:**
  - `shader_matches_cpu_reference_per_feature`: 14 cases (diffuse, metal and dielectric specular, haze on a metal and a dielectric, fuzz, subsurface, thin film, clear coat, tinted coat, a sheen layer, mix with coverage, blend). Each is within 1.2 sRGB steps of `MaterialProgram::eval`; the worst is 0.83.
  - A golden per feature: `diffuse`, `specular`, `haze`, `fuzz`, `subsurface`, `thin_film`, `clear_coat` and `mix` (`material_*`).
  - `closure_budget_downgrades_gracefully_and_reports_it`, end to end over the Engine API. With budget 1 a car paint is reported 2 → 1 (`paint`, vertical, `coat` + `base`) and logged. The image is byte-identical to the material fitted on the CPU, and it differs at the platform's 8.
  - `project_cannot_raise_the_closure_budget` (`concordia-app`: 9, 0 and 255 are refused at load), plus `a_project_budget_only_lowers_the_platforms`.
  - View-mode goldens `view_mode_{closures,haze,fuzz,subsurface,thin_film}`, with structural checks: closure counts get distinct colors, and feature views are black without the feature.
- **Added:**
  - `unlit_and_albedo_views_match_the_cpu_for_layered_materials`;
  - `a_lower_closure_budget_renders_the_fitted_material` (including refitting when the budget is raised again);
  - `an_edited_material_is_refitted`;
  - `masked_materials_keep_fragments_at_or_above_the_cutoff` (one slab, textured, layered; just above and below);
  - `the_projects_closure_budget_reaches_the_renderer`;
  - layered and cycle import tests, the built-in default, the encoder layout tests.

**Red run:** all 21 new render and app tests and all 4 material tests failed first for the right reason:
- the diffuse case was 1.6 sRGB steps off under M1's coupling;
- the stub drew only the first slab;
- budgets were fixed;
- goldens were missing.

The asset tests failed to compile against `MaterialData`, and the glTF tests failed to decode the old artifacts. Two exceptions to writing tests first, both checked:
- the `material_rows` unit tests were written with the encoder, then seen to fail against an empty one;
- `an_edited_material_is_refitted` was written after a mutation slipped through, then seen to fail with that mutation.

None passed at once.

**Mutation checks:**
- 21 shader mutations, each caught by `shader_matches_cpu_reference_per_feature` or the unlit test (re-run on the final, optimized shader). They covered the diffuse coupling, the direct table read as the split sum, sheen dimming, Λ's branches, thin-film wavelengths, haze weight, haze in the energy, the wrap scale, light-side transmittance, the medium's path length, sheen in transmittance, mix order, coverage and premultiplying.
- Three mutations slipped through at first, so the cases were strengthened. Haze at weight 0.5 is symmetric, so it became 0.3. Haze on a metal has no diffuse to dim, so a dielectric case was added. No case put fuzz on a layer, so a sheen layer was added. A tinted coat covers the medium's path length.
- The optimization's shortcuts: precomputing the diffuse share at the wrong cosine, skipping light-side transmittance in a layered program, and dropping the layer transmittance in the albedo view are all caught. Mixing alphas instead of layering them in `program_alpha` is caught by the mask test. A wrong Fresnel exponent in `eval_plain` was missed by the one-sun comparison, whose half vectors all lie near the view; the grazing-light test was added and catches it.
- Program cache: skipping the refit on a budget change is caught, and ignoring a content change is caught by the new test.
- Editor: dropping a mode fails `tsc`.

**Goldens (author approved, 2026-11-20):**
- 13 new.
- 5 re-blessed in 2.3b rather than 2.3c, moved by the energy rule: `shadow_box_plane_iso` (FLIP 0.032), `view_mode_lit` (0.015), `view_mode_shadow_cascades` (0.014), `many_lights_iso` (0.012) and `pbr_sphere_grid_directional` (0.010), all at tolerance 0.01.
- 1 more, found by the full run because it lives with the gates: `gates/m1_physics_stack` (FLIP 0.026 of 0.015; at most 6 steps), also the energy rule alone, approved separately. The M1 gate's `capture_screenshot` schema snapshot gained the five view modes (reviewed).
- Within tolerance and left as they were: the glTF goldens (0.004–0.018 of 0.02), `orthographic_isometric`, `view_mode_light_clusters`, `spot_light_cone` and `pbr_spheres_ibl` (0.002–0.003).
- Data views are byte-identical.
- With M1's coupling put back temporarily, every golden returned to M1, so the movement is the energy rule alone. 2.3c's migration check starts from this baseline.

**Changed deliberately during green (my test mistakes):**
- the thin-film case was too dark for the comparison's own lit-pixel guard, so it got a brighter sun;
- the subsurface golden's sky outshone the side sun, so it got a stronger sun and a dimmer sky;
- the comparison tolerance was tightened from 1.5 to 1.2 steps once the real error was measured.

**Not in this phase:**
- `material.get` / `set`, the inspector, Studio and retiring `MaterialData` (2.3c); FBX (2.3d);
- screen-space subsurface and code generation per material (M4);
- a benchmark for layered materials under many lights (`many_lights` measures one-slab materials).

### Phase 2.3c As built (2026-11-20, ADR-0068)
**Reconciled with the code as built:**
- **Hot reload did not reach the renderer.** The render module sent each asset to the render thread once and never again, so a saved material kept its old look in every capture and in the viewport; 2.3b's program cache refits only when new content arrives. Now `AssetDb::import` bumps an asset's generation whenever it finds new content (comparing import keys, so going back to an earlier version counts too), and the render module re-sends every material whose generation moved since it was sent (with any textures it now needs). Meshes and textures are still sent once (a follow-up).
- **`schema_get {"name": "MaterialData"}`, which the tech artist's prompt and skill used, never worked:** neither material type was registered. `Material` is now registered (not as a component), so `schema.get {"name": "Material"}` works, and the editor uses it.
- **The editor had no way to hear about file writes.** A new event topic, `asset` (`path`, `id`, `actor`), is published by `material.set` and `asset.import`. The editor subscribes to it: the viewport redraws, an Inspector showing that material reloads (including when an agent edits it through a shared engine), and the FileSystem dock refreshes.
- **Ranges are checked twice.** Reflected ranges (`roughness`, `mix`, ...) are refused while the params are decoded, and element ranges (`base_color[0]`) and structure errors by `Material::validate`. Both are `InvalidParams` (−32602), naming the node and field: a pointer such as `/material/nodes/coat/Slab/roughness`, plus `data.field` and `data.node` from validation. No new error code.
- **`material.set` creates files too** (my default, unasked): it validates, writes canonical text only when it changed (`written`), imports and returns the id. In the Studio, the server checks its path against the role's write paths like `scene_save` and `asset_import`, so a role can't write outside them or under a human-only gate.
- **The migration test** draws M1 *text*. The render tests' M1 materials are written by a test-only `M1` writer as M1 text and read back through `read_material`, so every M1 golden goes through the real migration path. The example project's three materials stay in M1 text on purpose, so the M1 gate golden checks it too.
- **Inspector structure edits (author's decision): draft until valid.** Every valid edit saves at once; an edit the engine refuses stays on screen as a draft with the engine's reason and a Revert button until the material is valid again. That makes multi-step changes possible (rename a node, then its references).

**Engine:**
- `concordia-assets`:
  - `MaterialGet` / `MaterialSet` (`material_method.rs`; `MaterialGetParams`, `MaterialSetParams` and results), in the assets module;
  - `MaterialFormat`, `read_material_with_format`, `Material::from_slab`, `MaterialError::{field, node}`, `AssetDb::id_at`, generation on content change;
  - `MaterialData`, `Material::from_pbr` and `From<MaterialData>` removed; M1's shape is the private `material_m1::M1Material` (M1's fields and defaults), read only.
- `concordia-import-gltf`: builds `Slab` directly (`slab`, `alpha_mode`); the artifacts are byte-identical, so it stays at version 2.
- `concordia-render`: `RenderState::material_generations`; `ship` re-sends stale materials (`assets::reload_materials`, sharing `add_material` with `resolve`).
- `concordia-api`: `Topic::asset`.
- `concordia-mcp`: `material.set` joins the server's path checks.
- Bench: `many_lights` also measures layered floors (2 and 3 closures).

**Editor:**
- `widgets.ts`: `variant` (struct-variant enums: `MaterialNode`, `AlphaMode`, and `Camera.projection`, which fell back to raw JSON before) and `map` (named entries: `Material.nodes`) widgets; `defaultValue` (required fields only, numbers at their minimum).
- `fields.tsx`: `VariantEditor` (pick the variant, edit its fields), `MapEditor` (rename, remove, add) and an Add button for empty options (haze, fuzz, ...).
- `MaterialInspector` (draft until valid), shown by `Inspector` when a material is selected. Clicking a `.material.ron` in the FileSystem dock selects it, and selecting an entity shows the entity again.

**Studio (D8):**
- The tech artist's scope gains `material_get` and `material_set`: 23 tools, about 19.8k tokens. On main it was 14.7k: 2.3b's view modes and stats fields had already grown it from ADR-0073's 14.0k, and the level designer from 20.4k to 21.1k and QA from 17.6k to 18.3k, unrecorded. Every enabled role stays under 22k.
- Its prompt reads and writes materials with the tools, checks `downgraded_materials`, and uses `schema_get {"name": "Material"}`.
- `concordia-add-material` now teaches the layered model: nodes, operators, slab fields and features, assignment, and the closure budget and views.
- The agent snapshot and the M3S gate snapshot change only the tech artist's `tools:` line and the edited prompt lines (reviewed).

**Tests:**
- `concordia-assets/tests/material_api.rs` (9); `importing_changed_content_bumps_the_generation_once`; `m1_defaults_are_kept_for_omitted_fields` and `m1_text_keeps_every_m1_field` (replacing `from_pbr_keeps_every_m1_field`).
- `concordia-render`: `a_saved_material_redraws_with_its_new_content` (`material.set`, then a file edit and `asset.import`).
- `concordia-mcp`: `agent_edits_a_slab_over_mcp` and `server_rejects_material_writes_outside_the_role_paths`.
- Studio: `tech_artist_edits_materials_with_the_material_tools_and_skill` and `material_skill_uses_the_real_material_tools_and_arguments` (every example call parses as the real params, and every example material validates).
- Editor: three widget tests, `material-inspector.test.tsx` (8), and the Playwright test `edit_a_slab_from_the_inspector_and_follow_an_agents_edit`.
- **The plan's tests, as built:**
  - `m1_pbr_materials_migrate_to_one_slab_unchanged_in_golden`: the sphere grid from M1 text is byte-identical to the same slabs written by hand, and within tolerance of the approved golden. Every other M1 golden also runs from M1 text;
  - `material_get_and_set_roundtrip_canonical_text`;
  - `material_set_is_attributed_and_not_journaled` (the history is unchanged, nothing to undo, the scene stays clean, and "material saved" carries the actor, path and id);
  - the inspector widget tests for struct-variant enums and named-node maps;
  - the Playwright test editing a slab;
  - the tech artist's scope and skill tests, and the sync and M3S snapshots.

**Red run:** every new test failed first for the right reason:
- the stub methods said `not implemented yet`;
- the generation never moved;
- the capture stayed red after `material.set`, and went blue only once re-sending was in;
- the material path was written outside the role's paths;
- there were no material tools in scope, no skill examples and no variant or map widgets;
- there was no material inspector.

These passed at once:
- `material_methods_have_generated_schemas`: it checks the declarations the stubs already had (a guard);
- `m1_pbr_materials_migrate_to_one_slab_unchanged_in_golden`: the migration was built in 2.3a (a guard, mutation-checked below);
- `agent_edits_a_slab_over_mcp` and the Playwright test: integration checks written after the parts they join. The Playwright test fails when the Inspector's material is unwired.

**Mutation checks:**
- The migration dropping roughness is caught by the migration golden test and the import test.
- M1's default metal set to 0 is caught only by `m1_defaults_are_kept_for_omitted_fields`: the test writer writes every field, so the goldens don't see defaults (the example project's files do, through the M1 gate).
- Never re-sending, and never bumping the generation, are each caught by `a_saved_material_redraws_with_its_new_content`.
- `migrated` always false, and `asset.import` not publishing, are each caught.
- In the editor, dropping the draft on a refusal is caught by both draft tests, and no variant widget by eight tests.

**Changed deliberately during green (my test mistakes):**
- Schema properties are sorted, so `Vertical` lists `bottom` before `top`.
- `mix_texture` is optional, so it isn't in the default.
- f32 values widen over JSON (0.2000000029), so they are compared within 1e-6.
- The old expectation that `Camera.projection` is raw JSON became the variant widget (the snapshot diff, reviewed, changes only `projection`).
- The session's subscription test gained `asset`.

**Goldens:** none new, none changed. Every M1 golden, now drawn from M1 text, and the gate golden `gates/m1_physics_stack` pass against the 2.3b baseline unchanged, so nothing was blessed.

**Left open by 2.3b, folded in:**
- **Layered materials under many lights** (`many_lights`, M3 Pro, the same day, three alternating runs). One slab: 2.94–3.03 ms median, against main's 2.96–2.98 ms, so it is unchanged (no shader change). A clear coat (2 closures): **23.4–23.5 ms**. A coat over a fuzzy mix (3 closures): **52.5–54.5 ms**. That is about 8× and 18× one slab, because every light runs the whole program through the thread-private closure array with light-side transmittance. A full screen of a layered material under 1,024 lights is far over budget: **follow-up** (per-feature pipelines or a cheaper layered path per light; M4's GPU work at the latest).
- **The Metal miscompile is reduced** to `tools/repros/metal-discard-after-private-array`: about 30 lines of hand-written MSL and a Swift host, no Slang. A fragment whose alpha (0.78) is read back from a thread-private struct array is discarded below a cutoff of 0.5; the same alpha read from the buffer is drawn, and writing the compared values out shows them right. Slang's MSL for the engine's shader is equivalent and correct, so it is Apple's compiler (macOS 26.2, 25C56, M3 Pro). Trying the reductions also showed that the trigger is reading the alpha back from the array: 2.3b's mask test placed after the stores, but reading `program_alpha` from the rows, passes. The guarded mask-first order is unchanged. **To do:** file it with Apple (Feedback Assistant), which is the author's call.

**Not in this phase:** hot reload of meshes and textures in the renderer; a node-graph view (M4); FBX (2.3d).

### 2.3c follow-ups As built (2026-11-21, ADR-0068, ADR-0073)
The author asked for three of 2.3c's open items before more features. They are one branch and one commit.

**1. Layered materials under many lights: a pipeline variant per program shape.**
- **Measured first** (`many_lights`, M3 Pro, main that morning: 3.38 / 23.47 / 52.46 ms). Skipping per-light transmittance alone gave 12.1 / 31.4 ms. Summing the closures without the program's stack gave 5.6 / 9.6 ms. So the dynamic interpreter (stack and closure indexing in thread memory, branches on data) and per-light transmittance were the cost, not the BSDF.
- **Built:** `ProgramShape` (`concordia-render/src/shape.rs`) holds the op kinds, which closure each slab op pushes, each closure's features, and whether it has a vertical layer. Each program with more than one op draws with `forward.slang` compiled for its shape (defines, same source; `[ForceUnroll]` over the shape's ops and closures). The stack, closure indices and feature branches resolve at compile time.
  - `Pipelines` keeps the compiler, compiles a fragment source per shape on first use, and keys forward pipelines by shape.
  - One-slab materials keep the base shader. `FLAG_LAYERED` left the material rows, because the shape owns it now; its row test moved into `shape.rs`.
  - `Renderer::material_variants()` counts the variants.
  - Considered and not done: generating code per material (ADR-0068's alternative), and a linear "monomial" form of the program per light, which is exact only without mixes under layers.
- **Result** (same Mac, three runs): one slab **2.95–3.01 ms** (unchanged), a clear coat **4.77–4.85 ms** (1.6× one slab, was 8×), three closures **7.28–7.44 ms** (2.5×, was 18×). A new shape costs one compile, about 0.2 s of slangc plus Metal's, cached in the DDC.
- **Tests:**
  - `shape.rs`: `one_slab_programs_use_the_base_shader`, `a_shape_defines_its_ops_slabs_features_and_layering`, `materials_differing_only_in_values_share_a_shape`. They failed on `todo!()` first.
  - `materials.rs`: `layered_materials_draw_with_one_variant_per_shape`. It was written before `material_variants` existed, so its first failure was a compile error.
  - `shader_matches_cpu_reference_per_feature`, the grazing-light, unlit and albedo comparisons, the masked-material test and every golden are guards: they passed unchanged, now through the variants.
- **Mutations, all caught:**
  - no variants: six tests, including the CPU comparisons, the clear-coat and mix goldens, and the variant test;
  - features dropped from the shape: the per-feature comparison;
  - layering dropped: three comparisons;
  - mixes taken as layers: the mix golden and two comparisons.

**2. Hot reload of meshes and textures** (and environment maps).
- `RenderState` now keeps the kind and generation of every imported mesh, material and texture it sends (`stale_assets`, `note_generations`, `assets::reload`, generalizing 2.3c's material-only bookkeeping).
- The render thread calls the new `Renderer::forget` on every asset it receives. It drops the GPU copy (mesh buffers, texture, environment levels and any prefiltering of the map).
- The old resources go to `Renderer::retired`. The next frame adopts them and frees them when it retires, after every earlier frame in flight.
- **Tests (red first, for the right reason):** `a_reimported_texture_redraws_with_its_new_pixels` and `a_reimported_mesh_redraws_with_its_new_geometry` in `capture.rs`, through the Engine API with the glTF importer. The mesh test checks that the result equals what a fresh engine draws. `forgotten_meshes_textures_and_environments_are_uploaded_again` in `render.rs` covers held assets being reused, then reloaded after `forget`, matching a fresh renderer.
- **Mutations, all caught:** dropping each of the mesh, texture and environment parts of `forget`; a thread that never forgets; and tracking materials only.
- **My test mistakes, fixed deliberately:**
  - sub-asset references use `#` (`asset:<id>#mesh/Quad/0`);
  - the forget test's first camera used daylight exposure, so it drew black;
  - one mutation run first failed to compile, so it was re-run.

**3. Per-role tool budget raised to 50k** (the author's decision, ADR-0073 amended). `ROLE_TOOL_BUDGET` is 50,000. Slimming the schemas is the follow-up, and the budget comes down after it. No test changed behavior: the budget test passes before and after.

**Also fixed:** the `concordia-command` crate doc example had been broken since 2.6d, which made `apply_one` and `undo` take `&mut TypeRegistry`. `just test`'s doc-test step failed on main, and 2.3c's checklist missed it.

### Phase 2.3d As built (2026-11-21, ADR-0068)
**Reconciled with the code as built:**
- **Skeletons and clips already had asset types**, so none were invented. The glTF importer writes `SkinData` (`skin/<index>`: joint names, the prefab's joint entity ids, inverse bind matrices), `ClipData` (`clip/<name>`: name, duration, TRS channels targeting prefab entities) and per-vertex `joints`/`weights` in `MeshData`. FBX writes exactly these.
- **"Clips with duration and rate":** `ClipData` has no rate field, and adding one would change every glTF clip artifact. As built, each stack is baked to linear keys from 0 at the file's frame rate (30 if it declares none), so the rate is the key spacing, and `duration` is the stack's playback length. If 2.4 needs the authored rate (for example, events on frames), it can add `ClipData::frame_rate` (glTF has none).
- **"Targets an existing skeleton by bone name":** a clip's channels target entities of its own file's prefab, as glTF's do. Retargeting goes through names: the target entity's `Name` and `SkinData::joint_names` (the bone nodes' names). 2.4's player maps by these; nothing else is needed now.
- **Sharing instead of duplicating:** the glTF importer's format-neutral code moved to a new crate, `concordia-import-model`: names, the prefab from a node tree, mesh finishing, texture color spaces and `load_prefab`. It also holds the new weight reduction. The FBX importer maps ufbx into it; glTF re-exports `generate_tangents` and `load_prefab` unchanged. The glTF artifacts are byte-identical: every artifact of every sample and fixture (345) was hashed before and after, by a temporary test that was not committed, so glTF stays at version 2.
- **ufbx does the conversion while loading:** right-handed, Y-up, meters, with `MODIFY_GEOMETRY` space conversion and geometry transforms baked in. Geometry, node transforms, bind matrices and keys all arrive converted, and the importer only flips V. A file's own handedness is mirrored on Z if it differs. External files are never opened by ufbx.
- **Blender writes absolute texture paths** (here, the author's home folder) into FBX files. The fixture script exports through `/tmp/concordia-fbx-fixtures/`, and `fixtures_hold_no_user_paths` guards it.
- **The rig fixtures keep Blender's default export** (objects scaled by 100 in a centimeter file), which many real files share. Only the static quad uses clean transforms, so its positions can be checked directly.
- **Textures:** embedded bytes, else the file next to the source through the asset database's sandboxed `read_relative`. A missing external file is skipped with a warning and the material keeps its factor; FBX files often point at the artist's own disk (my default, unasked). An image that fails to decode is an error, as in glTF. Textures are named by file stem, since FBX texture names are slot names.
- **Registration:** `concordia_app::model_importers()` (glTF, FBX) is used by the assets module and `concordia import`, so the list exists once. The CLI no longer depends on the glTF crate.
- **Studio:** no enabled role imports models. The level designer has no `asset_import`, and the modeler and animator are disabled until 3.7 and 2.4. The tech artist imports textures and owns materials, so its skill (`concordia-add-material`) and prompt now say that `.gltf`, `.glb` and `.fbx` models import the same way and that their materials are `asset:<id>#material/<name>` sub-assets, which `material_set` can't change. No tool scope changed, so the 50k budget test is unaffected.
- **`deny.toml` unchanged:** ufbx is vendored C, invisible to cargo-deny, and the new crates carry the workspace license. `just deny` passes.

**Engine:**
- `third_party/ufbx` (v0.23.1, commit 26a482ae, MIT or Unlicense, used under MIT; `VENDOR.md` has the SHA-256 sums).
- `ufbx-sys`: the `cu_*` flat C shim (`shim/concordia_ufbx.{h,c}`; every entry point bounds-checked), hand-written bindings guarded by a header blake3 and ABI size checks, built with `cc` as C11.
- `concordia-import-model` (new): `ModelTree`/`ModelNode`/`MeshPart`, `write_prefab` (now an error for a node reached twice or out of range), `load_prefab`, `load_prefab_doc`, `finish_mesh`, `generate_tangents`, `unique_names`, `TextureUsage`, `reduce_influences`, `insert_artifact`.
- `concordia-import-fbx` (new): `FbxImporter` (`fbx`, version 1). `ffi.rs` is the only `unsafe` (the crate denies it elsewhere).
- `concordia-import-gltf`: maps into `concordia-import-model` (version 2, byte-identical).
- `concordia-app`: `model_importers()`. `concordia-cli`: `import` uses it.

**Tests:**
- `ufbx-sys/tests/smoke.rs` (8): binding drift, struct sizes, declared units and axes, garbage, empty and truncated input, null and out-of-range arguments, triangulated corners and caps, the skinned fixture's seven influences, baking at a rate from 0.
- `concordia-import-model/tests/model.rs` (7): influences (largest four, sorted then normalized, invalid dropped, none → joint 0), names, mesh finishing, prefab ids by name with extra parts as children, tree errors.
- `concordia-import-fbx`: `tests/import.rs` (9) and `tests/robustness.rs` (2 proptests, 96 cases each: truncation and random byte edits never panic).
- `concordia-app`: `assets_module_imports_fbx_through_the_api`. `concordia-cli`: `import_handles_fbx`. Studio: `tech_artist_knows_materials_arrive_inside_gltf_and_fbx_models`.
- **The plan's tests, as built:**
  - `fbx_static_mesh_imports_positions_normals_uvs`:
    - the quad's 4 vertices and 6 indices, positions, +Y normals, UVs flipped to V-down, tangents, counter-clockwise winding, bounds;
    - the plain material's slab (base color, metallic 1, roughness 0.3);
    - the textured one's `texture/checker` (2×2 sRGB, top-left red);
    - the prefab's renderer references and the second quad's world position.
  - `fbx_skinned_mesh_imports_skeleton_and_weights`:
    - the 7 joints are the prefab's bone entities, by name;
    - every vertex's weights sum to 1 over at most 4 joints;
    - the seven-influence vertex keeps the four largest, renormalized;
    - **bind pose:** skinning every vertex at rest with the prefab's joints and the inverse binds puts it where the mesh node puts it (1e-4 m).
  - `fbx_animation_stack_imports_as_clips_with_duration_and_rate`: two clips with durations 0.5 and 1.0 s. Every channel has one key per frame at 24 fps from 0, unit quaternions, and targets a prefab entity. Every bone is rotated, and the leg actually swings.
  - `fbx_units_and_axes_normalize_to_engine_convention`:
    - in the Z-up centimeter file, Blender's (100, 200, 300) cm offset lands at the engine's (1, 3, −2) m, and the triangle's corners land where expected;
    - the normal is +Z (Blender's front), and the winding agrees with it (no mirror);
    - in Blender's default ×100 centimeter export, the body is 1.4 m tall at its true position.
  - `fbx_mixamo_style_clip_targets_existing_skeleton_by_bone_name`:
    - the animation-only file (no mesh, no skin) has one `mixamo.com` clip of 0.75 s;
    - every channel's target bone exists in the character, and every skin joint is rotated by the clip;
    - each bone's rest transform in the clip's file equals the character's.
- **Added:** `garbage_empty_and_truncated_fbx_are_descriptive_errors` (`AssetError::Import` with the path and an `FBX: ` reason), `fbx_import_is_deterministic`, `external_textures_are_read_next_to_the_file_and_missing_ones_are_skipped`, `fixtures_hold_no_user_paths`, the proptests.

**Red run:** every test failed first for the right reason:
- the shim stubs returned null and zero sizes, and the bindings had no hash line;
- the weight stub returned zeros;
- the stub importer said `not implemented yet`;
- the app and CLI said `no importer for … (extension "fbx")`.

One fix during green: Blender doesn't write `OriginalUpAxis` (ufbx reports it unknown), so the shim reports the file's declared `axes.up`.

These passed at once:
- The model crate's name, finishing and prefab-id tests: guards over code moved from glTF unchanged.
- Its two tree-error checks were written with the move, before their test. Both are mutation-checked below.
- The two proptests: vacuous against the stub, meaningful once ufbx runs.
- `fixtures_hold_no_user_paths`: a guard, because the fixtures were already fixed. It fails with a planted `/Users/` path.
- The Studio test: written after the skill edit, and it fails when the skill drops `.fbx`.

**Mutation checks:**
- **Caught:**
  - no V flip; no unit conversion; no axis conversion; a left-handed target (a mirror); conversion on the root only;
  - weights not normalized; three influences; no sort; an inverse bind off by 0.1%; renamed joints; joint indices off by one;
  - a fixed 30 Hz instead of the file's rate; a halved duration; dropped rotations;
  - embedded textures ignored; external files never read; a missing texture turned into an error; base color decoded as linear; metallic dropped; the first material in every slot;
  - the shim ignoring the corner cap; an unchecked node index (the test process crashes);
  - both tree errors in `write_prefab`;
  - the skill dropping `.fbx`.
- **Survived, with the reason:**
  - `mesh_node_to_bone` instead of `geometry_to_bone`: equivalent, because geometry transforms are baked into meshes.
  - No start trim: Blender always writes stacks from 0. I tried an action at frame 25, and Blender rebased it too. The trim is kept for other exporters and can't be tested with our fixtures.
  - The shim's empty-input and weight-range checks: ufbx guarantees both, so they are defense in depth.
  - A minimum sample rate equal to the frame rate: untestable with our fixtures, so the option was removed rather than kept untested.

**Changed deliberately during green (my test mistakes):** `AssetId` needs a v5 or v7 UUID, not `from_u128`; a `// SAFETY:` comment had to sit directly above its `unsafe` block.

**Goldens:** none added or changed. **Snapshots:** the tech artist's agent file (the edited prompt line and its hash; reviewed).

**Checklist:** `just lint`, Tauri clippy, `CONCORDIA_REQUIRE_GPU=1 just test` (1,267 tests and the doc tests), `just gate-tests M1` and `M3S`, `just deny` (and the editor's), `just editor-schemas` (no change) and `just editor-test` (118 unit, 22 end-to-end) all pass. The editor's `Cargo.lock` gains the new crates.

**Not in this phase:**
- FBX lights, cameras, blend shapes, vertex colors and a second UV set (glTF imports colors and `uv1`);
- separate roughness and metalness maps (ufbx gives single-channel maps; packing them into `metallic_roughness_texture` is a follow-up);
- more than one skin per mesh (the first is used);
- the Godot `.fbx.import` mapping (M5).

### Effort (re-estimated)
| Phase | Estimate |
|---|---|
| 2.0 / 2.0b / 2.0c Terra core | 3 + 3 + 2 weeks |
| 2.6 Scripting tiers | 3 weeks |
| 2.8 Replay (minimal) | 1 week |
| 2.3 Substrate-style materials + FBX | 4–5 weeks |
| 2.4 Animation | 3–4 weeks |
| 2.4b LOD and representation | 2 weeks |
| 2.5 Navigation, crowds, traffic | 3–4 weeks |
| 2.7 Audio + UI | 3–4 weeks |
| 2.10 Sample game + gate | 2–3 weeks |
| **Total** | **~29–36 phase-weeks**; some phases overlap (2.3 with the Terra work, 2.7 with 2.4–2.5) |

### Follow-ups for other plans (done in this refinement)
- **M5:** receives 2.1, 2.2, 2.9 and the Godot mappings (D1, G6).
- **M7:** keeps 7.0 (done), large-scale performance, AI-driven crowd and traffic behavior and its gate.
- **M4:** its 4.13 note now names M5's slice, not M2's.
- **`docs/design.md`:** the milestone list (M2's gate; Terra's core in M2).
- **`plans/README.md`:** the index, crate table, ADR registry and API names.

Game-specific details for the author's port live in the git-ignored `private/plans/M2-port-begins.md` (if present).

## Goal & context
> *Historical (the pre-ADR-0048 port goal, now M5's). M2's goal is the refinement above: engine breadth on Terra's Mass-style core, proven by an original sample game.*


M2 is where the engine stops being a tech demo and starts carrying a real game. The target is **the reference port**: a Godot 4.x isometric action-RPG with GDScript gameplay, `.tscn`/`.tres` scenes, custom `.gdshader` shaders, glTF/FBX models, `AnimationTree` state machines, navmesh levels (some hand-painted with a level-editor plugin, some procedural), autoload singletons, a C++ GDExtension, and a headless GDScript test suite that acts as the oracle. M2 builds the subsystems a game like it needs (scripting tiers, animation, navigation, audio, in-game UI, replay), the **Godot importer** and the **agent-driven GDScript→Rust translation workflow**, then proves them by porting **one vertical slice of the game's first level** until it is playable, passes the game's own translated test suites, replays deterministically and matches golden screenshots.

The Godot project is reached only through `CONCORDIA_PORT_SRC` (read-only) and the port project through `CONCORDIA_PORT_PROJECT`. Nothing from the game is committed to this repository.

### What a read-only survey of the reference port found

These facts shape every phase below.

- **Size.** Hundreds of GDScript files, roughly 100k+ lines of game code, plus a large test suite (tens of thousands of lines, over a hundred suites) and in-project tools. The biggest single scripts are the player and enemy controllers and the world/main script, each a few thousand lines.
- **Most of the scene graph is built in code, not in `.tscn`.** Scenes declare hundreds of `Node3D` plus dozens of `MeshInstance3D` and `Control` nodes, but only a handful of `AnimationPlayer`, `AnimationTree`, `CharacterBody3D`, `NavigationAgent3D` and `NavigationRegion3D` nodes. Scripts create meshes, shader materials, cameras, environments, containers, lights, particles and animation trees with `.new()` far more often. **Consequence:** a `.tscn` importer moves the data-heavy assets (rigs, animation graphs, HUD layouts, prop scenes), but most of the world is reproduced by *translated code*. The translation workflow is the critical path, not the importer.
- **Architecture is port-friendly.** Gameplay logic lives in plain-object *data models* (actor, player and enemy models), and nodes are the view (they hold a reference to their model). Many systems are static, node-free and pure (combat resolution, enemy decision-making, enemy stats, spawn placement, loot fan-out math), because the game's own test runner cannot pump frames. These map directly to Rust structs, components and pure functions.
- **A handful of autoload singletons** (settings, an actor registry/event hub, data loading and enums, a loot manager, a "game feel" service). Cross-cutting events are typed signals on the event-hub autoload (enemy died, enemy damaged, item picked up), with well over a hundred `signal` declarations and a couple of hundred `emit(` calls in total.
- **Balance data is typed `Resource`s** in `.tres` tables (enemy, rank, skill, rarity, affixes, XP, loot, sound bank, game feel, ...). `.tres` files with `PackedByteArray` use **format=4** (base64), e.g. the level data resources. Nodes carry `unique_id=` and resources `uid://` references.
- **Physics:** `CharacterBody3D.move_and_slide` (a couple of dozen uses), `Area3D.body_entered` sensors, ray queries, `RigidBody3D` loot balls. No `PhysicsServer3D` use.
- **Navigation:** the navmesh is **not baked by Godot's Recast**. A terrain script builds polygons from a fixed-size cell grid (the level's walkability grid is 1:1 with it) and `NavigationAgent3D` follows paths for the player (click-to-move) and enemies. The game warns that its hand-built navmesh breaks silently above roughly ten thousand polygons.
- **Animation:** `AnimationTree` with state machines (dozens of transitions), blend trees, `OneShot`, `TimeScale`, `Transition`, `TimeSeek`; `BoneAttachment3D` for held weapons; clips "authored in place" (root motion stripped). Tweens, timer awaits and `await`s are used heavily (hundreds of `await`s overall).
- **Feel:** hit-stop through `Engine.time_scale` (one writer, the game-feel autoload), camera punch, knockback, damage numbers.
- **Shaders:** dozens of `.gdshader` files (mostly spatial, a few canvas_item); around ten define a custom `light()` (cel/toon lighting through shared `.gdshaderinc` includes); heavy use of `TIME`, `discard` and `INSTANCE_CUSTOM`, and a few uses of `hint_screen_texture`/`hint_depth_texture`.
- **Assets:** over a hundred `.glb`, a few dozen `.fbx` (Mixamo-style clips, props, prop packs), a few `.gltf`, thousands of `.png`, a few dozen `.ogg`/`.wav`, audio buses SFX/Music.
- **Native code:** a C++ GDExtension (a mesh-cutting effect), with a GDScript twin that must stay identical.
- **Tests:** `godot --headless --path <src> <test-runner scene>`, a fixed RNG seed per suite, fails on any logged engine error and on a suite with zero assertions. The game's own docs say the suite **cannot** check input, movement, animation, physics overlap or feel. M2's playthrough and replay tests are designed to cover exactly that gap.
- **The main gameplay loop:** a boot scene → the world scene builds terrain, navmesh, level objects and armed encounters from a level-data resource → the player clicks to move (`NavigationAgent3D`) and holds a key to throw their weapon (a seeking projectile, the basic attack), with skills on the number keys → enemies patrol, chase and attack (an AI state enum plus a pure decision module) → a combat resolver resolves the hit (armour curve, resistances, crit, dodge, whole-HP rounding) → hit feedback (hit-stop, camera punch, knockback, numbers, sound) → the enemy dies (gibs, XP, loot roll) → a loot animation throws a ball that lands as a dropped-loot pickup → walking into it picks it up into the inventory → HUD orbs, level-up, skill points.

### The slice: the opening zone (the first level, cropped)

**Choice.** The slice is the opening of the game's first hand-authored level: one zone, from the player-start marker through a fixed end marker a few hundred metres along the main path.

**What the slice contains, abstractly:** one hand-authored opening zone; a click-to-move hero; two melee enemy types; a thrown/ranged weapon attack plus one melee area skill; hit feedback; death → XP → loot roll → pickup into inventory; the HUD (health/mana, level) and floating enemy health bars; the isometric camera; SFX.

**Why a slice like this.**
1. **It is literally the first level.** A new game starts the player there, and the design's gate says "the first level of the Godot game".
2. **It is authored, not generated.** Placement is exact and reproducible, so the procedural generator (a large share of the level code) does not have to be ported first, and the zone replaces the generated surroundings: no town, no procedural scatter population.
3. **Its enemy set is small and melee-only.** Two melee enemy types, both glTF rigs with `AnimationTree` state machines, at the lowest monster band with a level-1 hero, so the game's own early balance applies unchanged.
4. **It is sparse by design.** Enemy groups are placed farther apart than the engage radius, so the engagement cap and wave spawning do not interact. Its navmesh is small, with no extra dressing scenes.
5. **It exercises every M2 subsystem:** a rigged hero and two rigged enemy kinds (anim), click-to-move and chasing (nav), the weapon throw (seeking projectile, physics), the melee area skill (cone), loot drop and proximity pickup (physics sensor), health/mana orbs, floating health bars and damage numbers (UI), hit and cast cues (audio), terrain and prop shaders with custom `light()` (shader translation), the zone's models and decorative fire patches (import).

**Included in the slice:** level-data import (grid, walkability, terrain types/palette, enemy packs, objects, markers, monster band), terrain height sampling (the noise field the level relies on), the zone's main model, decorative fire patches, the navmesh, the hero (hold-key weapon throw; the melee area skill, with its unlock gate opened exactly as the game's test runner opens it), both melee enemy types at normal rank, combat resolution and hit feedback (hit-stop, knockback, camera punch, damage numbers), death → XP → loot roll → loot animation → dropped loot → pickup into the inventory, the HUD (health/mana orbs, level label), floating enemy health bars, the isometric camera with zoom, SFX through the sound bank, a fixed time of day.

**Excluded from the slice (stays in the port ledger as `untranslated` or `stubbed`):** NPCs, dialogue, barks, quests and the story system; checkpoints and save/load (persistence); gibs (the native GDExtension; enemies use a plain death clip + corpse); the elite/objective encounter and its rewards (a **stretch** item in Phase 2.9); weather and day/night cycle; minimap and world map; particles (`GPUParticles3D`/`CPUParticles3D`, see Open questions); x-ray cutout, footprints, ground tracks, water; skill tree, inventory and character-sheet windows (models yes, windows no); every skill except the basic attack and the one melee area skill; debug menu and dev keys; third-party trail addons.

**Slice size (estimate, to be replaced by `port.scan --slice opening-zone`):** tens of thousands of lines of GDScript in the transitive closure (the player and enemy controllers partially, the data models, combat, the player model, item and skill subsets, level data and terrain-sampling subsets, the loot manager, the autoloads, the camera, game feel, audio, HUD), roughly a fifth of the game code, plus several thousand lines of oracle tests.

## Scope / Non-goals

**In scope**
- `concordia-script-wasm`: Wasmtime component host for the WIT world `concordia:game/gameplay` (batch/query-oriented), deterministic configuration, sandbox, reload.
- Tier-1 Rust-native game crates with hot reload (`concordia-script-native`, see "Proposed convention additions").
- `concordia-audio`: decoding (WAV, Ogg Vorbis), own mixer with buses, simple spatialization, an offline backend for headless tests.
- `concordia-anim`: skeletons, clips, CPU/GPU skinning, blend trees, one-shots, state machines with Godot-compatible semantics, animation events, bone attachments, property tweens.
- `concordia-nav` + `recast-sys`: navmesh build from geometry plus area marks, path and nearest-point queries, path-following agents, Detour crowds, `NavigationAgent3D` mapping.
- `concordia-ui`: in-game HUD and menus (the approach is decided in Phase 2.7).
- `concordia-interchange`, introduced here: the importer-neutral model and its writer into an engine project.
- `concordia-import-godot`: `.tscn`/`.tres`/`project.godot`/`.import` parsing, node→entity/component mapping, resources, typed data resources, project settings (input map, autoloads, buses, stretch), an unmapped-feature report, the GDScript structure tools and gdshader→Slang translation.
- FBX import through ufbx (`concordia-import-fbx` + `ufbx-sys`).
- A minimal `concordia-replay`: input-log record/replay with world-hash checkpoints (M3 generalizes it).
- The agent translation workflow: MCP/CLI tools (`port.*`, `shader.translate`), mapping tables, the port ledger, translated-test scaffolding and an oracle harness. Agents do the translating; the engine never calls an LLM.
- The opening-zone slice, playable, with its oracle suites ported.

**Non-goals (M2)**
- The full reference port (see Risks: it continues through M3's Agent Studio).
- Automatic GDScript→Rust conversion. Tools give agents structure, mappings and scaffolds; agents translate; tests and replays verify (ADR-0005).
- Pixel parity with Godot. Golden images are of *our* render of the slice, blessed after human review. Cross-engine parity is checked only with luma statistics (Phase 2.9).
- A particle/VFX system (reported as unmapped; see Open questions), visual graphs (M4), playtest assertions and general replay (M3), the Studio (M3), USD (M5), Unity/Unreal importers (M5).
- Porting the game's level-editor plugin. It keeps working in Godot; its `.tres` output is re-imported. An engine-native level editor is later work.
- Motion matching, IK, retargeting beyond same-skeleton clips, audio DSP effects, rich text, UI theming.
- Windows/Linux for the gate (macOS/Metal first, per M1).

## Dependencies (what must already exist, from which milestone)

| Needed | From | Used for |
|---|---|---|
| ECS, fixed-step deterministic loop, world-state hash | M0 | every sim test, replay |
| `concordia-reflect` schemas, JSON Schema export | M0 | WASM wire encoding, MCP schemas for new tools, hot-reload migration |
| `concordia-command` with `Actor` | M0 | port ledger updates, script commands, imports |
| `concordia-scene` RON, stable UUIDv7 ids | M0 | importer output |
| `concordia-api` JSON-RPC + `concordia-mcp` + `concordia` CLI | M0 | every new tool |
| ECS events / timers | M0 (assumed; added here if absent) | signals, `await` translation |
| Metal RHI, render graph, clustered PBR, CSM, `capture.screenshot` offscreen | M1 | goldens, skinning, UI pass, custom lighting |
| `concordia-shaders` (Slang) | M1 | translated gdshaders |
| `concordia-assets` + DDC + hot reload, `concordia-import-gltf` | M1 | rigs, clips, props, FBX output |
| `concordia-physics` (Jolt) | M1 | character controller, sensors, ray picking, loot balls |
| `concordia-input` + `input.inject` | M1 | click-to-move, hold-to-attack, playthrough |
| Golden-image harness (FLIP), `just bless` | M1 | slice goldens |
| Minimal Tauri editor | M1 | optional: playing the slice by hand |
| Toolchain: `wasm32-wasip2`, wasmtime, wasm-tools, cargo-component, Slang, Blender 5.2 (fixture generation) | M-1 | WASM tier, shader tests, FBX fixtures |
| Godot 4.x installed locally (optional) | dev machine | `port.oracle` baseline counts and probes only, never required to build or test the engine |

## Crates, modules & directories (created / touched)

**Engine repo (created)**
- `crates/concordia-interchange/`: `InterchangePackage`, `IScene`, `IEntity`, `IComponent`, `IAssetRef`, `IMaterial`, `IDataDoc`, `ImportReport`; `writer` (→ `.scene.ron`, `.prefab.ron`, `.material.ron`, `.data.ron`, `.animgraph.ron`, `.import.ron`), `idmap`.
- `crates/concordia-import-godot/`: `text` (variant lexer/parser for tscn/tres/project.godot, formats 3 and 4), `scene` (node tree, instancing, overrides), `resource`, `project` (input map, autoloads, buses, display), `uid` (`.uid`/`.import` resolution), `map` (node/resource mapping), `mappings/*.ron` (tables), `gdscript` (tree-sitter-based AST, structure, dependency graph, slice closure), `gdshader` (lexer, parser, Slang emitter, report), `port` (ledger, scaffolder, oracle harness generator, scan/report), `animtree` (AnimationTree → animgraph).
- `crates/concordia-import-fbx/` + `crates/ufbx-sys/` (vendored `third_party/ufbx`, MIT).
- `crates/concordia-anim/`: `skeleton`, `clip`, `sample`, `skin` (CPU reference), `graph` (blend tree, one-shot, time-scale, transition, state machine), `events`, `attach`, `tween`.
- `crates/concordia-nav/` + `crates/recast-sys/` (vendored `third_party/recastnavigation`, zlib): `build`, `query`, `agent`, `crowd`, `areas`.
- `crates/concordia-audio/`: `decode` (hound, lewton), `mixer`, `bus`, `voice`, `spatial`, `backend` (`cpal`, `offline`), `cue`.
- `crates/concordia-ui/`: `node` components, `layout` (taffy), `anchors` (Godot-compatible anchor/offset rules), `widgets` (panel, texture rect, nine-patch, progress, label, button), `text` (cosmic-text), `input` routing, `render` (render-graph pass), `world_text` (billboards).
- `crates/concordia-script-wasm/`: `host`, `bindings` (wasmtime component bindgen), `wire` (reflect-driven columnar encoding), `determinism` (fuel, NaN canonicalization, virtual WASI clocks/random), `reload`.
- `crates/concordia-script-native/` (proposed): `GameModule` trait, C-ABI vtable, dylib loader with copy-on-load, reload + reflect migration, static-link path.
- `crates/concordia-script-guest/` (proposed): guest-side Rust SDK for `wasm32-wasip2` over the `wit/` world.
- `crates/concordia-replay/`: `InputLog`, `ReplayHeader`, recorder, runner, divergence report.
- `wit/concordia-game/`: `types.wit`, `world-query.wit`, `commands.wit`, `events.wit`, `rng.wit`, `time.wit`, `log.wit`, `gameplay.wit` (the world).
- `tests/fixtures/godot/mini-project/` (our own small synthetic Godot project, written by us), `tests/fixtures/fbx/` (generated by `tools/fixtures/make_fbx.py` in Blender), `tests/fixtures/audio/`, `tests/fixtures/gdshader/`.
- `tests/gates/m2_port_begins.rs`, `docs/guides/porting-from-godot.md`.

**Engine repo (touched)**
- `concordia-terra` (events/timers if M0 lacks them; `Sim` vs `Presentation` system sets; `SimClock::time_scale`), `concordia-physics` (character controller over Jolt `CharacterVirtual`, sensor enter/exit events, ray pick), `concordia-render` (GPU skinning, lighting-model extension point for translated `light()`, per-instance material params, UI pass, world-space text/billboards), `concordia-shaders` (compiling translated modules), `concordia-assets` (FBX, audio, anim graph, data docs, sidecars), `concordia-input` (pointer→world pick helper, imported bindings), `concordia-api`/`concordia-mcp`/`concordia-cli` (new methods), `justfile` (`gate M2`, `corpus`, `play-slice`), `deny.toml` (new licenses).

**Port project (outside the engine repo, private: it contains the game's assets)** at `$CONCORDIA_PORT_PROJECT`:
```
concordia.toml
.mcp.json, .claude/skills/port-gdscript/SKILL.md   # written by `concordia port init`
assets/, scenes/, data/                            # importer output (text) + source assets
crates/game-core/      # RNG (Godot PCG32-compatible), enums, data tables, data models (actor, player, enemy)
crates/game-combat/    # combat resolution, damage results, resistances, hit feedback rules, melee arc, seeking projectile
crates/game-items/     # items, rarity, affixes, loot tables/manager, inventory
crates/game-level/     # level data, terrain height sampling, encounters
crates/game-actors/    # player + enemy controllers as ECS systems, camera, game feel
crates/game-ui/        # HUD bindings
crates/game-brain/     # enemy decision logic, built BOTH natively and as a wasm32-wasip2 component
crates/game-game/      # the GameModule: registers components/systems (cdylib + rlib)
port/ledger.ron, port/idmap.ron, port/slices/opening-zone.ron, port/oracle/, port/probes/*.gd
tests/oracle/*.rs, tests/playthrough/*.rs, tests/golden/m2_slice/*.png, tests/replays/*.replay.ron
```
The Godot project at `$CONCORDIA_PORT_SRC` is **read-only** for every tool in this milestone.

## Phases

Phase order is a dependency order, but 2.3–2.8 can run in parallel once 2.1 and 2.2 land. 2.9 needs all of them.

### Phase 2.1: Interchange model and Godot text formats (moved to M5, ADR-0048/0067)

- **Objective:** parse every Godot text format the reference port uses, map scenes and resources onto an importer-neutral interchange model, and write that model into an engine project as canonical text, with a report of everything that did not map.
- **Deliverables:** `concordia-interchange` (model, writer, id map); `concordia-import-godot::{text, scene, resource, project, uid, map}`; `mappings/nodes.ron`, `mappings/resources.ron`, `mappings/project.ron`; API methods `port.scan` and `port.import` (MCP `port_scan`, `port_import`); CLI `concordia port scan <godot-dir>` and `concordia import godot <godot-dir> --out <project> [--slice <name>]`; `tests/fixtures/godot/mini-project/`.
- **Tests first**
  - `concordia-import-godot` unit/integration:
    - `variant_parser_roundtrips_all_literal_kinds` (proptest): `Vector2/3`, `Color`, `Transform3D`, `Basis`, `Quaternion`, `&"StringName"`, `NodePath("...")`, typed `Array[Color]([...])`, `Dictionary`, `Object(InputEventKey, ...)`, `ExtResource("id")`, `SubResource("id")`, ints/floats/bools/null parse and re-emit identically.
    - `tres_format4_packed_byte_array_decodes_base64`: a format=4 `PackedByteArray("AgIC...")` decodes to the exact bytes.
    - `tres_format3_packed_arrays_decode_inline`: format=3 inline packed arrays decode the same values.
    - `tscn_parser_reads_header_ext_sub_nodes_connections`: header, `ext_resource`, `sub_resource` (declared above their users), `node` (with `parent`, `unique_id`, `instance`, `groups`) and `connection` sections all populate the model.
    - `uid_references_resolve_through_uid_and_import_files`: `uid://...` resolves to the same file as its `path=`; a stale `path` with a valid `uid` resolves by uid.
    - `instanced_scene_overrides_merge_in_document_order`: an `instance=ExtResource(...)` node with property overrides and editable children yields the overridden values.
    - `project_godot_input_map_maps_to_engine_bindings`: `InputEventKey` (physical_keycode), `InputEventMouseButton`, `InputEventJoypadButton`/`Motion` with deadzones become concordia-input bindings with the same action names (e.g. `move`, `attack`, `skill_1`).
    - `project_godot_autoloads_become_singleton_resources`: each `*res://` autoload becomes a named singleton entry pointing at its (untranslated) script reference.
    - `project_display_stretch_settings_map_to_ui_scaling`: the base viewport size and `canvas_items`/`expand` map to `concordia-ui` scaling settings.
    - `bus_layout_tres_maps_to_audio_bus_config`: `default_bus_layout.tres` yields Master, SFX→Master, Music→Master with gains.
    - `script_class_resource_becomes_typed_data_doc`: a fixture resource with `script_class="<TypeName>"` becomes a `.data.ron` with `type: "<TypeName>"` and every exported field, including nested sub-resources.
    - `standard_nodes_map_per_table`: each fixture node type in `mappings/nodes.ron` produces the listed components (e.g. `MeshInstance3D`→`Transform`+`MeshRenderer`, `CollisionShape3D`→collider, `Camera3D`→`Camera`, `OmniLight3D`→`PointLight`).
    - `unmapped_node_type_is_reported_not_dropped`: an unknown node becomes an entity with a `GodotProvenance` component and a report entry with path, type and severity.
    - `import_preserves_unknown_properties_in_provenance`: properties with no mapping are kept verbatim in `GodotProvenance.raw`.
    - `reimport_is_idempotent_and_ids_are_stable`: importing twice yields byte-identical output; ids come from `port/idmap.ron` (Godot scene uid + `unique_id` or node path → a deterministic id, see README "Ids").
    - `script_references_become_script_slots_with_ledger_entries`: a node's `script = ExtResource(...)` becomes a `ScriptSlot { source: "res://..." }` component and a ledger row `untranslated`.
  - `concordia-interchange`:
    - `package_writes_canonical_scene_ron` (insta snapshot of the mini-project's main scene).
    - `writer_emits_import_sidecars_with_stable_asset_ids` (`.glb` → `.glb.import.ron`, same id on re-import).
    - `report_entries_are_sorted_and_deduplicated`.
  - Cross-crate (`tests/`):
    - `port_scan_over_mcp_returns_unmapped_report`: over `concordia-mcp`, `port_scan` on the mini-project returns counts by node type, resource type, script, shader and asset, each with mapping status; the schema is generated from `concordia-reflect`.
    - `port_import_is_a_logged_transaction_with_actor`: `port.import` with `actor: agent:gameplay-programmer#1` appears in the command log.
  - Corpus (`#[ignore]`, run by `just corpus` when `CONCORDIA_PORT_SRC` is set; fails, never skips, if the variable is set and anything errors):
    - `corpus_parses_every_tscn_tres_and_project_godot`: every `.tscn`, `.tres` and `project.godot` in the reference port parses with zero errors.
    - `corpus_scan_report_snapshot`: the summary (counts per type and status) is snapshotted in the port project, so drift is visible.
- **Implementation notes**
  - Godot is MIT-licensed, so its public docs ("TSCN file format", class reference) **and** its source are allowed references under `docs/clean-room.md` (permissive license). Prefer the docs; record any source file consulted in `AGENTS.md`; copy no code.
  - The parser is a hand-written lexer + recursive-descent parser over a `GodotVariant` enum; it keeps source spans for reports.
  - Interchange is deliberately thin: entities with typed components (from the engine's reflect registry), asset references (glTF in M2; USD arrives in M5), materials, typed data documents, animation graphs, and the report. The writer is the **only** path from any importer into a project (M5 reuses it).
  - Node mapping is table-driven (`mappings/nodes.ron`: `godot_type`, `components`, `property_map`, `status: mapped|partial|unmapped`, `notes`). The same tables feed the agent tools in Phase 2.2.
  - Godot `Node3D` hierarchy → entity hierarchy with `Parent`/`Transform`; groups → tags; `connection` sections → `SignalWire` records for the translator (not executed).
  - Large grids (walkability, terrain type, ...) stay base64 inside `.data.ron` in M2 (Open question: PNG layers).
- **Done criteria:** all tests green; `just corpus` green on the reference port; the scan report for the full project exists and is attached to the private refinement notes.

### Phase 2.2: GDScript structure tools and the translation workflow (moved to M5)

- **Objective:** give agents everything they need to translate GDScript into Rust test-first, through MCP/CLI, with no API key: the AST and structure of a script, the dependency graph and slice closure, mapping tables for Godot APIs, a port ledger, and scaffolds that turn the game's test suites into failing Rust tests.
- **Deliverables:** `concordia-import-godot::{gdscript, port}`; `mappings/api.ron` (Godot API → engine API/Rust idiom), `mappings/idioms.ron` (signals, `await`, `@onready`, `@export`, groups, autoloads, `queue_free`, `_ready`/`_process`/`_physics_process`, `Engine.time_scale`, tweens, timers); the oracle harness crate template (`check`, `equal`, `not_equal`, `is_true`, `is_false`, `not_null`, `greater`, `in_range`, `allow_error`, zero-assertion failure); API methods `port.ast`, `port.map`, `port.scaffold`, `port.status`, `port.update`, `port.oracle` (MCP `port_ast`, `port_map`, `port_scaffold`, `port_status`, `port_update`, `port_oracle`); CLI `concordia port init|ast|map|scaffold|status|update|oracle`; the Claude Code skill `port-gdscript` written by `concordia port init`; `docs/guides/porting-from-godot.md`.
- **Tests first**
  - `gdscript_ast_parses_class_name_extends_exports_signals_regions`: `class_name`, path-based `extends "res://..."`, `@export` (with types and defaults), `@onready var x = $Path`, `signal`, `enum`, `const preload`, `static func`, inner classes and `#region` are all present in the structure output.
  - `gdscript_structure_reports_await_emit_and_tween_sites`: every `await`, `emit(`, `create_tween()`, `create_timer()` and `Engine.time_scale` write is listed with a span and a mapping hint.
  - `gdscript_structure_lists_godot_api_calls_with_mapping_status`: `move_and_slide`, `get_next_path_position`, `intersect_ray`, `AnimationTree.travel`... each carries `mapped|partial|unmapped` from `mappings/api.ron`.
  - `dependency_graph_resolves_class_name_preload_and_extends`: edges from `class_name` uses, `preload`/`load` literals, path `extends`, autoload names and `.tscn` script slots.
  - `slice_closure_is_deterministic_and_minimal`: the closure from a slice manifest's entry points on the mini-project equals a hand-written expected set, in stable order.
  - `mapping_tables_validate_against_reflect_schema`: every `mappings/*.ron` entry has a unique key and a valid status; every `mapped` entry names an engine API or type that exists in the registry.
  - `port_map_returns_entry_with_rust_idiom_and_example`: `port.map {godot: "CharacterBody3D.move_and_slide"}` returns the engine idiom, notes and a code example that compiles as a doc test.
  - `scaffold_suite_emits_one_failing_test_per_label`: each `test("label")` group in a GDScript suite becomes `#[test] fn <snake_label>()` containing the original GDScript as comments and `todo!("port: <label>")`, so the scaffold **fails first**; labels are preserved verbatim in a `#[doc]` for 1:1 traceability.
  - `scaffold_module_emits_typed_skeleton`: a script scaffold emits a Rust module with structs for `class_name` types, fields for `@export` (reflect-derived, PascalCase serialized names kept via `#[reflect(rename)]`), and `todo!()` method bodies with spans.
  - `oracle_harness_messages_match_godot_runner_format`: `equal(2, 3, "x")` fails with `"[Suite] label: x — expected 3, got 2"`, matching the reference runner's test base class (the format is pinned in the mini-project fixture).
  - `oracle_suite_with_zero_assertions_fails`: mirrors the game runner's rule.
  - `oracle_disallowed_error_log_fails_suite`: an `error!` log during a suite fails it unless `allow_error("substring")` matches, as in the reference runner's error counter.
  - `oracle_baseline_parser_reads_runner_output`: a captured fixture of a Godot runner's output yields per-suite assertion counts and pass/fail into `port/oracle/baseline.ron`.
  - `ledger_update_requires_actor_and_is_logged`: `port.update` without an actor is refused; with one, the change is in the command log.
  - `ledger_status_transitions_are_monotonic_unless_reopened`: `untranslated → scaffolded → translated → verified`; a backward move needs `reopen` with a reason.
  - `ledger_verified_requires_green_oracle_and_parity`: `verified` is refused unless the linked oracle suites pass and their assertion counts are ≥ the Godot baseline.
  - `port_init_writes_mcp_json_and_skill` (insta): `.mcp.json` registering `concordia-mcp` and `.claude/skills/port-gdscript/SKILL.md`.
  - `port_tools_work_with_no_api_keys`: with every `*_API_KEY` variable scrubbed from the environment, every `port.*` call succeeds.
  - Corpus (`#[ignore]`): `corpus_every_gd_file_parses_without_error_nodes` (every `.gd` file in the reference port), `corpus_slice_closure_snapshot` (the slice closure, with line counts, snapshotted in the port project).
- **Implementation notes**
  - Parse with `tree-sitter` + the `tree-sitter-gdscript` grammar (both MIT; vendor the grammar in `third_party/` so it can be patched for recent Godot 4.x syntax: typed dictionaries, `@abstract`, etc.).
  - **The workflow an agent follows** (written into the skill and the guide): `port_status {ready: true}` picks a ledger row whose dependencies are `verified` → `port_ast` → `port_map` for anything `partial|unmapped` → `port_scaffold` (failing tests from the oracle suites + module skeleton) → implement in Rust until `cargo nextest run -p game-*` is green → `port_update status=translated` → run the slice playthrough/replay → `port_update status=verified`. TDD falls out naturally: scaffolds start red.
  - **Translation rules** in `mappings/idioms.ron` and the guide: translate behavior, not structure; plain-object data models → plain structs/components; static pure classes → free functions; signals → typed ECS events; `await create_timer(t)` → a `Timer` component and an explicit state; `await` on signals → explicit state machines; tweens → `concordia-anim::tween`; `_physics_process` → `Sim` systems at fixed step; `_process` → `Presentation` systems (never touch sim state); `Engine.time_scale` → `SimClock::time_scale`; autoloads → ECS resources; `queue_free` → despawn command; groups → tags; `@export` names keep their PascalCase serialized form.
  - **Numeric semantics:** GDScript `float` is 64-bit and `int` is 64-bit, while `Vector3` is 32-bit. Ported gameplay numbers use `f64`/`i64` where GDScript used scalars, so oracle tolerances hold.
  - **Godot-compatible RNG:** `RandomNumberGenerator` is PCG32 with Godot's `randf_range`/`randi_range` mappings. `game-core::GodotRng` reproduces them (Godot is MIT; record the reference), so seeded rolls in oracle tests and probes match the original.
  - `port.oracle` optionally runs the installed Godot read-only (`--headless --path <src> <test-runner scene>`) and parses its output; probe scripts live in `<port>/port/probes/` and are run with Godot's `--script`, **never written into the game project**. If Godot is absent, the committed `baseline.ron` is used.
- **Done criteria:** all tests green; a real agent session (Claude Code, subscription only) scaffolds two of the reference port's pure-logic suites (e.g. RNG and affixes) into failing Rust tests and makes them pass using only the MCP tools, recorded in the ledger with its actor.

### Phase 2.3: Asset import: FBX, materials, gdshader→Slang

- **Objective:** import every asset kind the slice uses, and give agents a gdshader→Slang translator that does the mechanical work and reports precisely what needs a human or agent.
- **Deliverables:** `concordia-import-fbx` + `ufbx-sys`; Godot `.import` sidecar mapping (`.glb.import`/`.fbx.import`/`.png.import`/`.ogg.import` → `.import.ron`); `StandardMaterial3D`/`ORMMaterial3D` → `.material.ron`; `ShaderMaterial` → material referencing a translated Slang module; `concordia-import-godot::gdshader`; `concordia-render` lighting-model extension point; API `shader.translate` (MCP `shader_translate`), CLI `concordia shader translate`; `mappings/shader_builtins.ron`; `tools/fixtures/make_fbx.py`.
- **Tests first**
  - FBX (fixtures generated by our Blender script, committed small):
    - `fbx_static_mesh_imports_positions_normals_uvs`.
    - `fbx_skinned_mesh_imports_skeleton_and_weights` (weights sum to 1, ≤4 influences after reduction, bind pose matches).
    - `fbx_animation_stack_imports_as_clips_with_duration_and_rate`.
    - `fbx_units_and_axes_normalize_to_engine_convention` (cm→m, Z-up→Y-up, handedness).
    - `fbx_mixamo_style_clip_targets_existing_skeleton_by_bone_name`.
  - Godot assets:
    - `godot_import_sidecar_options_map_to_import_ron` (loop flags, root-motion stripping, texture compression and filtering, audio loop).
    - `standard_material3d_maps_to_pbr_material_ron` (albedo, ORM, normal, emission, transparency, cull, unshaded).
    - `shader_material_references_translated_module_and_uniforms`.
  - Shader translator:
    - `gdshader_parser_accepts_fixture_corpus` (spatial and canvas_item, `#include` of `.gdshaderinc`).
    - `uniform_hints_map_to_material_parameters` (`source_color`, `hint_range`, `filter_*`, `repeat_*`, `instance uniform`).
    - `render_mode_flags_map_to_pipeline_state` (`blend_add`, `blend_mix`, `unshaded`, `cull_disabled`, `depth_draw_never`, `shadows_disabled`, `fog_disabled`).
    - `vertex_and_fragment_builtins_translate` (`VERTEX`, `NORMAL`, `UV`, `COLOR`, `ALBEDO`, `ALPHA`, `ROUGHNESS`, `EMISSION`, `TIME`, `INSTANCE_CUSTOM`, `discard`).
    - `custom_light_function_targets_lighting_model_interface` (`LIGHT`, `ATTENUATION`, `DIFFUSE_LIGHT`, `SPECULAR_LIGHT`).
    - `screen_and_depth_texture_hints_are_reported_as_manual_work`.
    - `report_lists_every_untranslated_construct_with_source_span` (each item: id, span, construct, suggested approach; the emitted Slang carries a matching `// PORT-TODO(<id>)`).
    - `translated_fixture_compiles_with_slang` (through `concordia-shaders`).
    - `translation_is_deterministic` (same input → byte-identical Slang and report).
  - Render (touched):
    - `custom_lighting_model_toon_golden`: a sphere lit through a translated cel `light()` (written from scratch in fixtures), golden image.
    - `per_instance_material_param_golden`: one shared material, per-instance fill values (a common health-bar pattern), golden image.
  - Corpus (`#[ignore]`): `corpus_shader_report_summary`: every gdshader in the reference port either compiles or has ≥1 manual item; the summary (counts per construct) is snapshotted in the port project.
- **Implementation notes**
  - ufbx (MIT) via a small `-sys` crate; FBX output goes into the same mesh/skeleton/clip asset types as glTF.
  - gdshader is GLSL-like; the translator emits a Slang module implementing the engine's material interface (`IMaterialSurface` for vertex/fragment, `ILightingModel` for `light()`). M1's clustered forward pass gains a lighting-model specialization constant per material.
  - Manual-work categories: screen/depth sampling (needs scene-color/depth inputs in the render graph), unsupported built-ins, `canvas_item` shaders (routed to `concordia-ui`'s pass), driver-specific precision, and any construct the parser keeps as opaque.
  - The slice's shaders, translated and finished by agents in Phase 2.9: the toon terrain shader, the toon prop/rock shaders, the fire shader, the floating health-bar shader, and the hero and enemy materials (mostly `StandardMaterial3D`). Canvas_item shaders for UI the slice does not show are not needed.
- **Done criteria:** tests green; the corpus report exists; translator output for the slice's shaders compiles (manual items may remain until 2.9).

### Phase 2.4: concordia-anim: skinning, blending, state machines

- **Objective:** play the ARPG character set: the hero rig and the two melee enemy rigs with their `AnimationTree` graphs, in the fixed-step sim so animation-timed gameplay (a throw released at a frame, an attack landing) is deterministic.
- **Deliverables:** `concordia-anim` (skeleton, clips, sampling, CPU reference skinning, graph nodes: clip, blend2/blendN, one-shot, time-scale, transition, state machine), animation events (from Godot method/call tracks), `BoneAttachment`, property tweens; GPU skinning in `concordia-render`; `.animgraph.ron`; `concordia-import-godot::animtree` (AnimationTree/AnimationNode* → animgraph).
- **Tests first**
  - `skeleton_bind_pose_matches_gltf_inverse_bind_matrices`.
  - `clip_sampling_interpolates_trs_linear_slerp_and_step`.
  - `cubic_spline_tracks_sample_per_gltf_spec`.
  - `cpu_skinning_matches_reference_vertices` (hand-computed 2-bone fixture).
  - `gpu_skinning_golden_matches_cpu_skinning` (golden image + vertex readback within epsilon).
  - `blend2_at_half_weight_is_normalized_average_pose`.
  - `one_shot_overlays_then_fades_out_on_completion` (fade-in/fade-out times, filters by bone mask).
  - `time_scale_node_scales_playback_rate`.
  - `transition_node_switches_input_with_xfade_and_reset`.
  - `state_machine_travel_finds_shortest_transition_path`.
  - `transition_xfade_blends_over_configured_time`.
  - `at_end_switch_mode_waits_for_clip_end`.
  - `transition_allowed_is_not_transition_taken`: an auto-advance transition fires only when its condition is true (a "prove the animation is actually running" lesson from the reference port).
  - `animation_events_fire_once_per_crossing_including_loops_and_large_steps`.
  - `anim_in_fixed_step_is_deterministic_across_thread_counts` (pose hash after N ticks).
  - `bone_attachment_follows_bone_world_transform` (the held weapon).
  - `tween_reaches_target_with_easing_and_dies_with_its_entity`.
  - `animation_tree_imports_to_animgraph_ron` (insta, on a synthetic fixture modeled on a typical enemy rig scene: StateMachine + BlendTree + OneShot + TimeScale + Transition).
  - Corpus (`#[ignore]`): `corpus_slice_rigs_import_without_unmapped_anim_nodes` (the hero and both enemy rigs).
- **Implementation notes**
  - Godot's `AnimationTree` semantics are the compatibility target (switch modes immediate/sync/at-end, advance modes disabled/enabled/auto, advance conditions, `travel()` path-finding, one-shot fades). Godot is MIT; reading its docs/source is allowed (record references).
  - Gameplay-relevant animation (state, events, root bones used for timing) updates in the `Sim` set at fixed dt; render interpolates poses. Pose evaluation is SoA, per-entity independent, so it parallelizes without affecting determinism.
  - Root motion is not needed (the reference port's clips are authored in place; `NavigationAgent3D` owns position).
  - References: glTF 2.0 spec (animation, skins), "Animation blending" chapters in *Game Engine Architecture* (Gregory), Godot AnimationTree docs.
- **Done criteria:** tests green; the hero and both enemy types play idle/walk/run/attack/hit/death through their imported graphs in a test scene, verified by events and a golden image.

### Phase 2.5: concordia-nav + recast-sys: navmesh, click-to-move, crowds

- **Objective:** build navmeshes with Recast, answer path queries with Detour, move agents (player click-to-move, enemy chase) and run enemy crowds deterministically; reproduce the reachability of the reference port's grid-built navmesh.
- **Deliverables:** `recast-sys` (vendored recastnavigation, zlib, `cc` build, a narrow C shim); `concordia-nav` (tiled build from collision geometry + per-cell area marks, path/straight-path/nearest-point/raycast queries, `NavAgent` path follower, `NavCrowd` over `dtCrowd`); `NavigationAgent3D`/`NavigationRegion3D` mapping; API `nav.build`, `nav.path` (MCP `nav_build`, `nav_path`).
- **Tests first**
  - `recast_builds_navmesh_for_flat_plane` (polygon count > 0, bounds).
  - `ffi_handles_are_released` (allocation counters return to zero after drop).
  - `navmesh_build_is_deterministic_byte_identical`.
  - `blocked_area_marks_carve_unwalkable_cells` (a walkability grid with BLOCKED cells yields no polygons there).
  - `grid_walkability_reachability_equivalence_fixture`: connected components of a fixture walkability grid equal the reachability of the built navmesh over sampled cell pairs.
  - `path_query_returns_straight_path_on_open_floor`.
  - `path_avoids_blocked_cells`.
  - `nearest_point_snaps_click_to_navmesh` (within one grid cell, the tolerance click-to-move relies on).
  - `unreachable_target_returns_partial_path_to_closest_point`.
  - `large_level_tiles_without_silent_islands` (a large synthetic fixture, kilometre-scale, well over ten thousand polygons, all walkable components reachable).
  - `nav_agent_arrives_within_tolerance_using_character_controller` (ECS + physics).
  - `crowd_agents_separate_and_do_not_overlap`.
  - `crowd_simulation_is_deterministic_across_runs_and_threads`.
  - `navigation_agent3d_maps_to_nav_agent_component` (radius, height, target desired distance, path desired distance).
  - `nav_path_over_api_returns_waypoints`.
- **Implementation notes**
  - Recast/Detour (zlib) are listed in `docs/design.md`. Build from the terrain mesh + static colliders, then apply area marks from the level's walkability (UNPAINTED/WALKABLE/BLOCKED tri-state semantics live in `game-level`, which supplies marks). Tiled navmesh from the start, so the hand-built navmesh's polygon-count cliff does not exist.
  - Crowd and queries run single-threaded inside the `Sim` set, in stable entity order, so they are deterministic.
  - Clicks: the pointer ray picks terrain through `concordia-physics` ray queries, then `nearest_point` snaps it.
  - References: Mononen, Recast/Detour docs and source (zlib); "Navigation meshes" (Snook, *AI Game Programming Wisdom*).
- **Done criteria:** tests green; a debug draw of a fixture level's navmesh renders in a golden image.

### Phase 2.6: Scripting tiers: native hot reload and the WASM host

> **Superseded in detail by the "Phase 2.6 refinement (2026-11-20)" section above** (sub-phases 2.6a–2.6d, a swappable backend, script-defined types). The text below is the original plan.

- **Objective:** run gameplay as tier-1 Rust native crates with hot reload, and as tier-2 WASM components behind the `concordia:game/gameplay` WIT world, both deterministic and both going through the command log.
- **Deliverables:** `wit/concordia-game/*.wit`; `concordia-script-wasm` (host, wire encoding, determinism config, sandbox, reload); `concordia-script-guest` (guest SDK); `concordia-script-native` (`GameModule` trait, C-ABI vtable, dylib loader, reflect-based migration, static path); API `script.reload` (MCP `script_reload`); a sample guest and a sample native module under `examples/`.
- **Tests first**
  - WIT and guest:
    - `wit_world_gameplay_validates_with_wasm_tools`.
    - `guest_component_builds_for_wasm32_wasip2` (via `cargo component`).
  - WASM host:
    - `host_instantiates_component_and_calls_describe` (systems, their queries and their schedule set).
    - `batch_query_returns_columnar_components_for_all_matches`: 1,000 matching entities arrive in one call, column-major, decoded by the guest SDK.
    - `boundary_crossings_per_tick_are_constant_in_entity_count` (crossing counter: same for 10 and 10,000 entities).
    - `guest_commands_apply_through_command_log_with_script_actor` (e.g. `script:game-brain`).
    - `guest_trap_is_contained_reported_and_system_disabled` (the host keeps running).
    - `fuel_exhaustion_traps_deterministically` (same tick, same fuel count on every run).
    - `nan_canonicalization_makes_float_results_bit_identical`.
    - `wasi_clocks_and_random_are_virtualized_to_sim` (no wall clock; `random_get` comes from the engine RNG stream).
    - `wasm_reload_preserves_world_state`.
  - Native tier:
    - `native_module_registers_components_and_systems_by_stable_name`.
    - `native_reload_swaps_behavior_without_losing_world_state`.
    - `component_layout_change_on_reload_migrates_via_reflect` (field added with default; field removed is reported).
    - `reload_with_incompatible_abi_version_is_refused`.
    - `module_with_static_mutable_state_is_rejected_by_lint` (a `#[deny]` lint/check in the module macro).
    - `static_link_and_dylib_paths_produce_identical_world_hash`.
  - API:
    - `script_reload_over_api_is_a_logged_command`.
    - `wasm_and_native_tiers_run_same_logic_to_identical_results` (the sample logic built both ways).
- **Implementation notes**
  - **WIT world shape** (`package concordia:game@0.1.0`): imports `world-query` (`query(spec) -> batch`: entity ids + one byte column per requested component, encoded with the reflect "wire" layout: little-endian, fixed-size fields, strings/lists in a side arena), `commands` (`submit(list<command>)`, applied at the end of the system with the module's actor), `events` (drain/emit typed events by name), `rng` (deterministic streams), `time` (tick, fixed dt, sim time scale), `log`; exports `systems` (`describe() -> list<system-desc>`, `run(system-id, tick-context)`). One call per system per tick; never per entity (ADR-0003).
  - Wasmtime config: Cranelift, `cranelift_nan_canonicalization(true)`, `consume_fuel(true)` with a per-tick budget, no threads, WASI p2 with virtualized clocks/random/filesystem (read-only asset access only), epoch interruption as a wall-time safety net that only applies in non-replay runs.
  - **Native hot reload:** the game is a crate exposing `GameModule` (register components/systems/resources). Dev builds compile it as a `cdylib` with a versioned C-ABI vtable; the loader copies the library to a unique path before `dlopen` (macOS caches by path), unloads only at a tick boundary, and migrates component data through reflect when schemas change. Game state lives only in ECS components/resources, so reload is safe by construction. Release builds link the same crate statically. Types cross the boundary by stable reflect name, never by `TypeId`.
  - References: WIT/component-model spec, Wasmtime docs, Bevy/Fyrox hot-reload write-ups (MIT/Apache) for prior art.
- **Done criteria:** tests green; editing the sample native module during a running headless session changes behavior after `script.reload` with the world hash continuing from the same state.

### Phase 2.7: concordia-audio and concordia-ui

- **Objective:** sound for the slice (cues through buses) and an in-game UI good enough for the HUD, floating bars, damage numbers and simple menus.
- **Deliverables:** `concordia-audio` (decode, mixer, buses, voices, spatial, `cpal` and `offline` backends, cues from a Godot-side sound bank resource); `concordia-ui` (approach below); world-space text/billboards in `concordia-render`; Godot `Control` mapping in `mappings/nodes.ron`; HUD-scene import to a UI prefab.
- **UI approach (decision, recorded in an ADR):** a **retained-mode UI made of ECS entities** with UI components (`UiNode`, `UiStyle`, `UiImage`, `UiText`, `UiProgress`, `UiNinePatch`, `UiButton`), saved in ordinary `.prefab.ron`/`.scene.ron`, laid out by **taffy** (MIT; flexbox/grid for containers) plus a Godot-compatible **anchor/offset** mode (so imported `Control` layouts land where they were), text by **cosmic-text** (MIT/Apache), drawn by one batched render-graph pass. Why: it is text-first and diffable, every UI mutation is a normal command (agents and the editor use the same API), it imports Godot `Control` trees directly, and it skins like a game rather than a tool. Rejected: egui (immediate-mode tool UI, hard to skin, no stable entities for agents), a webview in the shipped game (heavy, a second runtime, not deterministic), a full reimplementation of Godot's `Control`/`Container` system (too large; only the subset the game uses is mapped).
- **Tests first**
  - Audio:
    - `wav_and_ogg_decode_to_expected_frames` (generated fixtures; lewton, hound).
    - `mixer_sums_voices_and_applies_bus_gain_db`.
    - `bus_routing_sends_sfx_and_music_to_master`.
    - `voice_limit_steals_lowest_priority_oldest`.
    - `spatial_voice_attenuates_with_distance_and_pans`.
    - `offline_backend_renders_bit_identical_buffers` (hash of N seconds of mixed output).
    - `audio_callback_never_allocates` (allocation counter in the mix callback).
    - `streamed_music_does_not_underrun_offline`.
    - `sound_bank_cue_maps_pitch_and_volume_variation_with_seeded_rng` (a cue with stream list, volume and pitch ranges, as typical Godot sound banks define them).
    - `audio_events_are_presentation_only_and_do_not_change_world_hash`.
  - UI:
    - `anchor_and_offset_layout_matches_godot_rules` (all anchor presets and offsets on fixture rects; expected rects computed from Godot's documented rules).
    - `box_and_margin_containers_layout_children` (VBox/HBox/Margin/Panel/Center/Grid subset).
    - `canvas_items_expand_stretch_scales_from_base_size` (a wide base size, e.g. 2:1).
    - `texture_progress_bar_fill_modes_reflect_value` (the health/mana orbs).
    - `nine_patch_slices_correctly`.
    - `label_shapes_wraps_and_aligns_text`.
    - `pointer_over_ui_consumes_click_before_world` (clicking the HUD never triggers click-to-move).
    - `ui_mutations_go_through_commands_with_actor`.
    - `control_tree_imports_to_ui_prefab` (insta on a synthetic HUD fixture: `Control` root, several `TextureProgressBar`s, `TextureRect`s, `Label`).
    - `hud_fixture_golden`, `world_text_damage_number_golden` (billboarded `Label3D` replacement).
- **Implementation notes**
  - Audio: `cpal` (Apache/MIT) output; own mixer at 48 kHz f32 with lock-free command queue; `lewton` (MIT/Apache) for Vorbis, `hound` for WAV; **no** `symphonia` (MPL-2.0; `cargo-deny` would flag it). Spatialization is distance attenuation + equal-power pan in M2. The sound-catalog export target is later work (design.md).
  - UI text is rasterized into an atlas; SDF text is later work. `canvas_item` gdshaders route here as UI materials when needed.
- **Done criteria:** tests green; a fixture HUD driven by a component value updates its orb and label in a golden sequence; a cue plays audibly in a manual check (`just play-slice` later).

### Phase 2.8: concordia-replay (minimal)
> Done (2026-11-20). See "Phase 2.8 As built" in the refinement above, which supersedes this text.


- **Objective:** record an input log during a run and replay it to the same world state, relying on M0's fixed-step determinism. M3 generalizes this into full record/replay and playtest assertions.
- **Deliverables:** `concordia-replay` (`InputLog`, `ReplayHeader` {engine build, game module hashes (native + wasm), scene hash, seed, tick rate, thread count}, recorder, runner, divergence report); `.replay.ron` schema `version: 0` (input-only); API `replay.record`, `replay.run` (MCP `replay_record`, `replay_run`); CLI `concordia replay record|run`.
- **Tests first**
  - `input_log_records_injected_and_device_input_with_tick_stamps`.
  - `replay_file_is_canonical_ron_v0` (insta).
  - `replay_reproduces_world_hash_at_every_checkpoint` (M1's physics scene, hash every 60 ticks).
  - `replay_detects_divergence_and_reports_first_tick_and_component`.
  - `replay_header_mismatch_is_refused_unless_overridden` (module or scene hash differs).
  - `replay_is_identical_across_thread_counts`.
  - `replay_runs_headless_faster_than_realtime`.
  - `hot_reload_of_identical_module_mid_replay_keeps_hash`.
  - `replay_record_and_run_over_mcp`.
- **Implementation notes:** inputs are recorded post-binding as action events per tick (so a replay survives a rebinding) plus raw pointer positions for picking; presentation state (audio, UI animation) is excluded from the hash; the hash is M0's world-state hash restricted to `Sim` components.
- **Done criteria:** tests green; a recorded M1 physics session replays bit-identically.

### Phase 2.9: The opening-zone slice (moved to M5)

- **Objective:** import the reference port, translate the slice closure with agents through the Phase 2.2 workflow, port the slice's oracle suites, and make the opening zone playable, replayable and visually checked.
- **Deliverables:** the port project at `$CONCORDIA_PORT_PROJECT` (`concordia port init`, `concordia import godot --slice opening-zone`); `port/slices/opening-zone.ron` (entry points, level, loadout, enemy kinds, exclusions, end marker, seed); `game-*` crates; translated slice shaders with manual items closed; the playthrough script; the recorded replay `tests/replays/opening_zone.replay.ron`; goldens `tests/golden/m2_slice/{slice_start,slice_combat,slice_loot}.png`; Godot probe captures in `port/oracle/` (terrain heights, loot rolls at fixed seeds, luma statistics from the game's own screenshot/measurement harness scenes); `just play-slice` (windowed).
- **Tests first** (all in the port project, written and failing before translation; the oracle ones come from `port.scaffold`)
  - **Oracle suites** (ported from the reference port's GDScript suites; assertion counts must be ≥ the Godot baseline). The selected suites cover: seeded RNG; combat resolution; armour; resistances; damage rounding; the basic attack; the thrown weapon; hit feedback; enemy decision logic (run **twice**: native and inside the `game-brain` component); enemy AI states; enemy archetypes; enemy ranks; item rarity; affixes; loot tables; loot drops; inventory; health bars (structural assertions re-targeted at our shared material and per-instance fill); level data (the parts about grids, markers, packs and objects); level encounters; camera zoom; player state. Several thousand lines of GDScript tests in total. Groups that test excluded features are scaffolded and marked `#[ignore = "port: excluded from opening-zone (feature X)"]` in the ledger, never deleted.
  - Slice integration (`tests/playthrough/`):
    - `slice_import_has_no_blocking_unmapped_features` (the slice's report: only `non-blocking` entries, each with a ledger note).
    - `slice_level_data_roundtrips` (grid dimensions, packs, objects, markers including the player-start marker, monster band, zone-replacement flag).
    - `terrain_height_samples_match_godot_probe` (sampled heights equal the probe capture within 1e-4 m).
    - `slice_navmesh_reachability_matches_level_grid` (sampled cell pairs: reachable on our navmesh iff connected under the Godot grid builder's rules).
    - `player_click_to_move_reaches_marker` (pointer injected at a projected screen point, real picking path).
    - `hold_to_throw_weapon_seeks_and_damages_nearest_enemy`.
    - `melee_area_skill_hits_enemies_in_cone_only`.
    - `melee_enemy_patrols_chases_and_attacks` (AI state sequence).
    - `enemy_pack_resolves_to_one_of_two_melee_types_with_seeded_rng`.
    - `enemy_death_rolls_loot_matching_godot_probe_at_same_seed` (`GodotRng` parity).
    - `loot_ball_lands_and_proximity_pickup_adds_to_inventory`.
    - `full_bag_leaves_drop_on_ground_and_rearms_when_room_is_made` (a documented shipped behavior of the reference port, pinned).
    - `hit_stop_scales_sim_time_deterministically`.
    - `hud_health_orb_tracks_player_hp_ratio`, `enemy_health_bar_fill_tracks_hp`, `damage_numbers_spawn_on_landed_hits`.
    - `cast_and_impact_cues_reach_sfx_bus` (offline backend event log).
    - `enemy_brain_decisions_come_from_wasm_component` (host call counter > 0; choices equal the native build's on the same inputs).
    - `slice_runs_with_zero_error_logs` (the game's smoke-test rule).
    - `slice_shaders_luma_stats_within_tolerance_of_godot_harness` (mean and 5/50/95th-percentile luma of terrain, props and fire vs. the probe capture; tolerance set at refinement).
- **Implementation notes**
  - **Order of translation** (dependency-first, oracle-backed first): `game-core` (RNG, enums, tables as `.data.ron`, actor/enemy/player data models) → `game-combat` → `game-items` (rarity, affixes, loot manager, inventory) → `game-brain` → `game-level` (level data, terrain sampling and nav marks) → `game-actors` (player controller, enemy controller, weapon throw/seeker, melee area skill, loot animation and dropped loot, camera, game feel) → `game-ui` → `game-game`.
  - The player and enemy controllers are multi-thousand-line monoliths; agents translate **only the regions in the slice closure**, recording excluded regions in the ledger row (`partial`, with region names), so the remaining port knows exactly what is left.
  - Agents work as `agent:gameplay-programmer#n` (translation), `agent:shader-dev#n` (slice shaders' manual items), `agent:qa#n` (oracle runs, replay). In M2 these are ordinary Claude Code subagents/sessions using the port skill; M3's Studio formalizes the roles.
  - Both enemy types attack in melee; `game-brain` still runs every tick in one batch for all enemies (choice: skill or weapon), so the WASM boundary is exercised under load.
  - The elite/objective encounter is a **stretch** item: attempt it only after the gate passes.
- **Done criteria:** every test above green; ledger rows for the slice closure are `verified` or `partial` with named excluded regions; a human plays the slice from the player-start marker to the end marker in `just play-slice` and records "playable" (or bugs) in the plan's Status notes.

## Milestone gate (superseded: this was the port-slice gate, now M5's; M2's gate is phase 2.10, D2)

**Command:** `CONCORDIA_PORT_SRC=<godot-project> CONCORDIA_PORT_PROJECT=<port-project> just gate M2`

which runs `cargo nextest run --test m2_port_begins` (`tests/gates/m2_port_begins.rs`). This is a **private-content gate** (see `plans/README.md`): it reads the game only through the two env vars and runs on the self-hosted runner on the author's machine, never in public CI. If either variable is unset, the gate **fails** with an explanatory message; it never skips. The gate asserts, in order:

1. **Import is reproducible and complete for the slice.** `concordia import godot $CONCORDIA_PORT_SRC --slice opening-zone` into a temp dir produces output byte-identical to the committed port project's imported files, and the slice's unmapped report has zero `blocking` entries.
2. **Oracle suites pass.** `cargo nextest run` over the port project's `tests/oracle` is green; for every selected suite the assertion count is ≥ the Godot baseline in `port/oracle/baseline.ron`; the enemy decision-logic suite is green in both native and WASM builds.
3. **Ledger is honest.** Over MCP (`port_status`, from a test client of `concordia-mcp`, no API key in the environment), every slice-closure row is `verified` or `partial` with named excluded regions; none is `untranslated` or `scaffolded`.
4. **Headless scripted playthrough** (seed pinned in `opening-zone.ron`), driven only through Engine API calls (`input.inject`, `sim.step`, `query.run`): from the player-start marker the hero click-moves along the main path to the end marker; ≥3 enemies die, including at least one of each melee type; both the weapon throw and the melee area skill land hits; the hero takes damage and ends with 0 < HP < MaxHP; XP increases; ≥1 loot item lands, is picked up (inventory count +1, dropped-loot entity despawned); the HUD health orb fill equals HP/MaxHP every 60 ticks; at least one cue reaches the SFX bus; zero error logs; enemy decisions came from the WASM component.
5. **Replay.** The playthrough is recorded with `replay.record`; `replay.run` replays it twice (1 thread and all cores); world-state hashes match the recording at every 60-tick checkpoint and the same assertions as step 4 hold on the replayed world.
6. **Golden screenshots.** During the replay, `capture.screenshot` at three pinned ticks (start of the zone; mid-combat with health bars and damage numbers; a loot drop on the ground with the HUD) matches the port project's `tests/golden/m2_slice/*.png` within each image's FLIP tolerance.

## Risks & mitigations

- **The full port is large, and M2 only starts it.** Roughly 100k+ lines of game GDScript (plus a large test suite and in-project tools), hundreds of scenes, dozens of shaders. The slice is roughly a fifth of the game code. Even at a sustained pace of a few thousand *verified* lines per week across several agents, the remainder is **months of work, realistically 6–12 months**, with the long tail in procedural level generation and biomes, UI windows, skills and the story system. *Mitigation / how it continues:* the port ledger is the backlog. M3's Agent Studio turns it into the task board: the Director assigns `ready` ledger rows to Gameplay Programmer instances per subsystem, the Shader/Tech Artist works through the shader report, the QA/Playtester proves each subsystem with oracle suites and replays, and the Level Designer later ports the level editor. Suggested order after M2: items/economy and skills (pure, heavily tested) → enemy roster and ranks → procedural zones, generator and biomes → story/dialogue → remaining UI windows → tools and the native GDExtension (rewritten in Rust; it is the author's own code). Each subsystem lands behind its oracle suites and a new slice replay.
- **Godot semantics leak into behavior** (`_ready` order, scene-tree order, synchronous signals, `await` timing, `_process` vs `_physics_process`). *Mitigation:* explicit idiom table, oracle tests, and the playthrough/replay tests that cover what the game's own suite cannot (input, movement, animation, overlap, feel).
- **Determinism** (Jolt, Detour crowd, animation-timed events, hit-stop time scale, WASM floats, hash-map iteration). *Mitigation:* single-threaded nav/crowd in stable order, anim in the `Sim` set, `SimClock` time scale, NaN canonicalization and fuel in WASM, cross-thread-count replay tests in 2.4/2.5/2.6/2.8, Jolt's deterministic build flags (assumes M1).
- **Visual parity**: custom `light()` toon shading, screen/depth-texture shaders, `WorldEnvironment` features and particles have no M1 equivalent. *Mitigation:* the lighting-model hook (2.3), luma-statistics parity against the game's own measuring harnesses, goldens of our own output blessed after review, particles reported and deferred.
- **tree-sitter-gdscript may lag recent Godot 4.x syntax.** *Mitigation:* vendored grammar, corpus test over every `.gd` file in the reference port, patch upstream.
- **Scope: 12+ crates in one milestone.** *Mitigation:* each crate ships the minimum the slice needs (no IK/retargeting, no audio DSP, no rich text, single navmesh); phases 2.3–2.8 run in parallel; anything not needed by the gate goes to the backlog.
- **Native hot reload on macOS** (dlopen caching, TLS destructors, Rust ABI instability). *Mitigation:* copy-on-load, C-ABI vtable, no statics in game crates, tick-boundary swaps, a static-link fallback that is always tested.
- **Private content vs. CI.** The game's assets cannot go in the open-source repo, so the M2 gate cannot run on public CI. *Mitigation:* the gate runs on a self-hosted runner on the author's machine (or locally) with the two env vars; public CI runs everything else, including all fixture-based tests. Game-specific details stay in the git-ignored `private/` folder.
- **Asset licenses.** Some prop packs in the reference port may be marketplace content. *Mitigation:* the port project is private; the importer records source and license metadata where known (design.md guardrail).
- **Weak oracles.** Several suites are "structural" (e.g. a health-bar suite that asserts shader parameters). *Mitigation:* re-target structural assertions to our equivalents, and rely on playthrough/replay/golden tests for behavior.

## Open questions

- Grid layers (walkability, terrain type, ground surface, ...) as base64 in `.data.ron`, or as PNG layers beside it (diffable images, editable in tools)?
- Wire encoding across the WIT boundary: reflect-described fixed layout (planned) vs. a schema'd format such as postcard; how strings/lists are arena-encoded.
- Particles: a minimal CPU emitter in M2 for loot effects and fire patches, or wait for an M4 VFX graph? (Planned: wait; report as non-blocking.)
- Should the port project be its own private git repository (planned) or a private branch of the game repo?
- Do probe scripts run with Godot's `--script` from outside the project reliably for everything we need (terrain sampling is pure; luma harnesses need a window)? Fallback: capture probe values once by hand and commit them to the port project.
- Should the reference port's grid-to-polygon navmesh builder be ported verbatim as an alternative nav source, if Recast reachability ever disagrees with the grid oracle?
- Does M0 already provide typed ECS events and timers, or does M2 add them to `concordia-terra`?
- Is `cosmic-text` the right text stack for the shipped game UI, or is a smaller atlas-only rasterizer enough for M2?

## ADRs to write
- ADR-0067: M2 refinement: engine breadth on Terra's Mass core, Substrate-style materials, Godot mappings to M5, sample-game gate (written).
- ADR-0068: Substrate-style material model (slabs, operators, closure budget), written in phase 2.3a.
- ADR-0069: Terra storage kinds, command buffers, signals and observers (written in phase 2.0b).
- ADR-0070: Terra templates, traits and spawners; the MCP tool-list budget rises to 16k (written in phase 2.0c; the budget superseded by ADR-0073).
- ADR-0073: tool budgets are per agent role; access is admin-configured in the roster; no whole-list budget (written 2026-11-20).
- ADR-0064 and ADR-0065 (port projects, Godot import identity) move to M5 with the port; their numbers stay reserved.

M2 draws from the 0060–0079 block (`plans/README.md`).

- ADR-0060: In-game UI approach (retained-mode ECS UI, taffy + Godot anchor mode, cosmic-text).
- ADR-0061: Tier-1 native hot-reload ABI (`concordia-script-native`, C-ABI vtable, no statics, reflect migration).
- ADR-0062: WIT `concordia:game/gameplay` world shape and wire encoding (batch, columnar, deterministic config).
- ADR-0063: Gameplay determinism contract (Sim vs Presentation sets, animation in Sim, `SimClock` time scale, WASM NaN canonicalization and fuel).
- ADR-0064: Port projects and private content (port project layout, `CONCORDIA_PORT_SRC`/`CONCORDIA_PORT_PROJECT`, self-hosted gate runner, goldens outside the engine repo, git-ignored `private/` notes).
- ADR-0065: Godot import identity (`port/idmap.ron`, deterministic id minting, provenance components) and Godot-as-MIT reference policy.
- ADR-0066: Audio stack (cpal, lewton, hound; no MPL decoders in the runtime).

## Effort estimate

| Phase | Estimate (author + agents) |
|---|---|
| 2.1 Interchange + Godot text formats | 2–3 weeks |
| 2.2 GDScript tools + translation workflow | 2–3 weeks |
| 2.3 FBX, materials, gdshader→Slang | 2–3 weeks |
| 2.4 concordia-anim | 3–4 weeks |
| 2.5 concordia-nav + recast-sys | 2 weeks |
| 2.6 Scripting tiers | 3 weeks |
| 2.7 concordia-audio + concordia-ui | 3–4 weeks |
| 2.8 concordia-replay (minimal) | 1 week |
| 2.9 Opening-zone slice | 5–7 weeks |
| **Total** | ~23–30 phase-weeks; **~22–26 calendar weeks** with 2.3–2.8 in parallel |

The slice translation (2.9) is the most uncertain line; re-estimate it at refinement from the real `port.scan --slice` closure.

## Proposed convention additions

These are used above; most were merged into `plans/README.md` in the 2026-11-17 consistency pass:

- **Crates:** `concordia-script-native` (tier-1 hot-reload host, M2), `concordia-script-guest` (guest SDK for `wasm32-wasip2`, M2), `concordia-import-fbx` + `ufbx-sys` (FBX via ufbx, M2). Vendored: `third_party/ufbx`, `third_party/recastnavigation`, `third_party/tree-sitter-gdscript`.
- **File extensions:** `.animgraph.ron` (animation graphs/state machines), `.data.ron` (typed data documents, e.g. imported Godot `Resource`s such as level data and balance tables). `.replay.ron` is introduced in M2 as **schema version 0** (input log only); M3 extends it.
- **Engine API / MCP methods:** `port.scan`, `port.import`, `port.ast`, `port.map`, `port.scaffold`, `port.status`, `port.update`, `port.oracle`, `shader.translate`, `script.reload`, `nav.build`, `nav.path` (MCP: `port_scan`, ... with `.`→`_`). Actor prefix `script:<module>` for commands submitted by gameplay modules.
- **CLI:** `concordia port init|scan|ast|map|scaffold|status|update|oracle`, `concordia import godot`, `concordia shader translate`, `concordia replay record|run`.
- **just recipes:** `just corpus` (runs `#[ignore]` corpus tests against `CONCORDIA_PORT_SRC`), `just play-slice`.
- **Environment variables:** `CONCORDIA_PORT_SRC` (Godot project, read-only) and `CONCORDIA_PORT_PROJECT` (port project).
- **Port project layout:** `port/ledger.ron`, `port/idmap.ron`, `port/slices/<name>.ron`, `port/oracle/` (baseline + probe captures), `port/probes/*.gd`; game crates prefixed `game-` in the port project; mapping tables at `crates/concordia-import-godot/mappings/*.ron`.
- **Test conventions:** "corpus" tests are `#[ignore]` and fail (never skip) when their env var is set; ported oracle suites live in `<port>/tests/oracle/` and must match or exceed the Godot baseline assertion count.

**Conflicts with earlier conventions (resolved in the consistency pass):**
- Goldens and gate depending on private game content: resolved by the README's "private-content gates" rule (gate file in `tests/gates/`, content through the env vars, self-hosted runner, never public CI).
- `replay.record`/`replay.run` and `.replay.ron`: M2 uses both in a minimal v0 form; M3 extends them.
