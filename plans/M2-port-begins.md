# M2: Engine breadth (formerly "Port begins")

> **Renamed by ADR-0048 and restructured by ADR-0067** (refinement, 2026-11-19). Phases 2.1, 2.2 and 2.9 (Godot formats, GDScript translation, the playable slice) and every Godot-specific mapping in 2.3–2.7 move to M5. Terra's Mass-style core moves here from M7. The refinement section below wins wherever the older phase text disagrees. The file keeps its old name so links stay valid.

Status: In progress: 2.0, 2.0b (2026-11-19), 2.0c, 2.6 (2.6a–2.6d), 2.8, 2.3a, 2.3b and 2.3c (2026-11-20), the 2.3c follow-ups, 2.3d, 2.4 (2.4.1–2.4.5, 2026-11-21) and 2.4b (2.4b.1, 2.4b.2; 2026-11-21) done; 2.5 refined (2.5a–2.5d), 2.5a and 2.5b (2.5b.1, 2.5b.2a, 2.5b.2b; 2026-11-22) done; 2.5c (2.5c.1, 2.5c.2) and 2.5d (2026-11-22) done; phase 2.5 complete; 2.7 split into 2.7a (audio) and 2.7b (UI); 2.7a refined (2.7a.1, 2.7a.2), 2.7a.1 and 2.7a.2 (2026-11-22) done; phase 2.7a complete; 2.7b refined (2.7b.1–2.7b.3, 2026-11-22); 2.7b.1, 2.7b.2 and 2.7b.3 (2026-11-23) done; phase 2.7b complete; 2.10 refined (the sample game "Lamplight", 2.10a–2.10d, 2026-11-23); 2.10a (2026-11-23) done · Depends on: M1, M3's Studio track (done) · Detail level: refined at the milestone level; each phase is refined again at its start, against the code as built · Estimated effort: ~29–36 phase-weeks (see the refinement)

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
| 10 | **2.7 Audio and in-game UI** (2.7a audio, ADR-0066; 2.7b UI, ADR-0060; split by the 2.7a refinement) | audio stack and cues; retained ECS UI, taffy plus anchors, parley and swash (ADR-0060; P1 replaced cosmic-text) | old 2.7 (Godot import and sound bank out) | sound-designer, composer (synth in M3 3.7), ui-ux |
| 11 | **2.10 Sample game and gate** (refined 2026-11-23: "Lamplight", 2.10a–2.10d) | an original sample game (D2), the deterministic gate, an optional live Studio gate | D2 | |

The VFX artist stays disabled: particles are not in M2 (open question, unchanged). M11 (Sparx, ADR-0015) enables it.

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
  - ~~`crowd_of_10k_agents_avoids_overlap` (perf on the reference machine)~~ stays in M7 7.7 (2.5 refinement); 2.5c proves overlap at 1,000 agents and determinism at 10,000;
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

### Phase 2.4 refinement (2026-11-21)
**Status: author decisions A1 and A2 taken (2026-11-21); 2.4.1–2.4.5 done: Phase 2.4 is done.**

Refined against the code as built after 2.3d. The old 2.4 text (under *Phases*) is superseded where this section differs.

**What exists (checked in code):**
- **Asset types** (`concordia-assets/src/data.rs`): `SkinData` (`joint_names`, `joints` as the prefab's entity ids, column-major `inverse_bind`), `ClipData` (`name`, `duration`, `channels`), `Channel` (`target` entity id, `path` T/R/S/weights, `interpolation` linear/step/cubic spline, `times`, flattened `values`), and `MeshData::joints`/`weights` (four influences, normalized by `reduce_influences`).
- **glTF** (`concordia-import-gltf/src/anim.rs`, version 2) writes `skin/<index>` and `clip/<name>` with the file's interpolation and times as authored; `duration` is the latest key time. **FBX** (version 1) writes the same types, baked to linear keys from 0 at the file's rate.
- **Prefabs:** bone nodes are entities with `Name`, `Transform` and `Parent`, ids UUIDv5(asset, `entity/node/<name>`). `prefab.instantiate` gives every entity a fresh id and remaps `Parent` only.
- **Terra:** phases `PrePhysics`, `Physics`, `PostPhysics`, `FrameEnd`, `Presentation` (ADR-0063). There is no `Sim` phase: the old plan's "Sim set" means the four simulation phases.
- **Render:** the scene cache already interpolates transforms between ticks (`scene_cache.rs`, `thread.rs`); vertices are pulled per triangle corner from storage buffers, so a vertex-shader skinning path has no fixed-function layout to fight.
- **Fixtures:** Khronos `SimpleSkin` and `Fox` (fetched by `just gltf-assets`); our Blender-made `skinned.fbx`, `anim_stacks.fbx`, `mixamo_run.fbx`.
- **Studio:** the animator is disabled (`team.toml`: "animation arrives in M2"); it has no skill of its own, and its Blender workflow needs the 3.7 bridge.

**Gaps:**
| # | Gap | Resolution | Sub-phase |
|---|---|---|---|
| A-G1 | Nothing says which skin a mesh uses: the prefab's renderer entity has only `MeshRenderer` | A `Skinned { skin }` component on the renderer entity, written by both importers | 2.4.1 |
| A-G2 | Clip channels name bones only by their own file's entity ids, which `prefab.instantiate` replaces | Bind by bone name (A1) | 2.4.1 |
| A-G3 | No skeleton type: `SkinData` has no parents or rest pose | `Skeleton` built from `SkinData` plus the prefab's `Parent`/`Transform` | 2.4.1 |
| A-G4 | No sampling, no skinning, no `concordia-anim` | 2.4.1, 2.4.2 | |
| A-G5 | FBX's start-time trim is untested (Blender always rebases stacks to 0) | A hand-written ASCII FBX fixture with a stack starting at 1 s (ufbx reads ASCII FBX) | 2.4.1 |
| A-G6 | `ClipData` validity is never checked (lengths, ordering, finiteness) | `validate_clip` at load, descriptive errors | 2.4.1 |
| A-G7 | Godot `AnimationTree` import and its corpus test were in 2.4 | Moved to M5 (D1): `animation_tree_imports_to_animgraph_ron` and `corpus_slice_rigs_import_without_unmapped_anim_nodes` are dropped here; M2 builds the native graph they will map onto | n/a |
| A-G8 | The done criteria named the port's characters | An original fixture character from our Blender script (D2 style) | 2.4.5 |

**Settled without a question (my calls; say if you disagree):**
- **Sub-phase names: 2.4.1–2.4.5.** Letters would collide with 2.4b (Terra LOD, its own phase), and "2.4.n" reads as part of 2.4. Branches are `m2/2.4.<n>-<slug>`, each off main with one commit and its own PR (not stacked).
- **Where animation runs:** a `FrameEnd` processor at the fixed dt (scaled by `SimClock::time_scale`). By then gameplay in `PostPhysics` has set this tick's parameters, so they take effect the same tick; events are Terra signals, delivered next tick (2.0b's rule). The pose is simulation state and hashed; the renderer interpolates the previous and current pose as it does transforms.
- **glTF clip times stay as authored** (a first key after 0 holds, as three.js and Godot play them); FBX keeps its trim to 0, now tested.
- **Morph targets and root motion are not in 2.4:** `Weights` channels are skipped by the sampler and reported, glTF morph targets aren't imported, and the old plan already ruled root motion out.
- **ADR-0074 (animation runtime):** pose storage, the phase, binding by name and the import changes, written in 2.4.1. `docs/design.md` doesn't change (it already says "own animation system").

**Sub-phases:**

**2.4.1 Skeletons, clips and sampling (CPU)** (~1 week). New crate `concordia-anim`; importer changes; no GPU.
- *Deliverables:* `Skeleton` (bone names, parent indices with parents first, rest local TRS, inverse binds); `Pose` (local transforms; where it lives is A2) and model-space evaluation; `sample(clip, t)` per the glTF 2.0 spec (linear, slerp, step, cubic Hermite with tangents scaled by key spacing); clip binding by bone name with unmatched channels reported; `AnimPlayer` component (clip, time, speed, loop) and the `anim.sample` processor in `FrameEnd`; `AnimModule`. Importers: `Skinned` component, plus A1's clip fields; glTF v3, FBX v2.
- *Tests first:*
  - `skeleton_orders_parents_before_children` (and a cycle or missing joint is an error naming it);
  - `skeleton_bind_pose_matches_inverse_bind_matrices` (`SimpleSkin` and `skinned.fbx`: rest model-space × inverse bind = identity, 1e-4);
  - `clip_sampling_interpolates_trs_linear_slerp_and_step`;
  - `cubic_spline_tracks_sample_per_gltf_spec` (hand-computed Hermite values, unequal key spacing);
  - `sampling_outside_the_keys_holds_the_end_values`;
  - `looping_wraps_time_and_clamped_clips_hold_the_last_pose`;
  - `clip_binds_to_another_skeleton_by_bone_name` (`mixamo_run.fbx` onto the character; unmatched channels listed, not errors);
  - `malformed_clip_is_a_descriptive_error` (length mismatch, unsorted or non-finite times, non-unit rotations normalized);
  - `player_advances_at_fixed_dt_and_time_scale` (scale 0 freezes);
  - `pose_is_part_of_the_world_hash`;
  - `anim_in_fixed_step_is_deterministic_across_thread_counts` (100 characters, 1 and 8 threads, hashes at ticks 1, 60, 300);
  - importers: `gltf_skinned_node_gets_a_skinned_component`, `fbx_skinned_mesh_gets_a_skinned_component`, A1's field tests, `fbx_stack_with_a_late_start_is_trimmed_to_zero`;
  - bench: sampling 1,000 characters × 65 bones (baseline recorded).
- *Studio:* none yet (2.4.5).

**2.4.2 Skinning: CPU reference and GPU** (~1 week).
- *Deliverables:* linear blend skinning on the CPU (`jointMatrix = inverse(meshWorld) · jointWorld · inverseBind`, glTF §3.7.3), the reference; GPU skinning in `concordia-render`'s vertex path from a joint palette per skinned draw; shadows skinned too; poses interpolated between ticks; skinned bounds.
- *Tests first:* `cpu_skinning_matches_reference_vertices` (hand-computed 2-bone fixture), `cpu_skinning_keeps_normals_unit`, `rest_pose_skinning_is_the_bind_mesh`, `gpu_skinning_matches_cpu_skinning` (vertex readback within 1e-4), `gpu_skinning_golden` (new golden, reviewed before blessing), `skinned_meshes_cast_skinned_shadows`, `rendered_pose_interpolates_between_ticks`, every existing render golden unchanged.

**2.4.3 Blend graphs and state machines (`.animgraph.ron`)** (~1–1.5 weeks). Godot `AnimationTree` semantics as the compatibility target, from its MIT docs and source.
- *Deliverables:* `AnimGraph` asset and importer (canonical RON, reflected schema); nodes clip, blend2, blendN, one-shot (fades, bone filter), time-scale, transition (cross-fade, reset), state machine (switch modes immediate/sync/at-end, advance modes disabled/enabled/auto, advance conditions on parameters, `travel` by shortest path); an `AnimGraphPlayer` component with parameters.
- *Tests first:* the old list's `blend2_at_half_weight_is_normalized_average_pose`, `one_shot_overlays_then_fades_out_on_completion`, `time_scale_node_scales_playback_rate`, `transition_node_switches_input_with_xfade_and_reset`, `state_machine_travel_finds_shortest_transition_path`, `transition_xfade_blends_over_configured_time`, `at_end_switch_mode_waits_for_clip_end`, `transition_allowed_is_not_transition_taken`; plus `animgraph_roundtrips_canonical` (insta), `invalid_graph_errors_name_the_node` (unknown clip, unknown state, cycle), `graph_parameters_set_by_component_patch_take_effect_next_tick`, `graph_evaluation_is_deterministic_across_thread_counts`.

**2.4.4 Events, attachments, tweens, tools and the skeleton view** (~1 week).
- *Deliverables:* clip events in seconds (frames shown through `frame_rate` where known) raised as Terra signals; `BoneAttachment { bone }` (Godot's `BoneAttachment3D`); property tweens with easing over reflected fields; `anim.*` methods and MCP tools (first list: `anim.inspect` for pose, bone transforms, graph state and parameters; `anim.travel` and `anim.set_param` on the play world, not journaled, like `input.inject`); the engine-drawn skeleton overlay (D7).
- *Tests first:* `animation_events_fire_once_per_crossing_including_loops_and_large_steps`, `events_arrive_as_signals_next_tick_in_entity_order`, `bone_attachment_follows_bone_world_transform`, `tween_reaches_target_with_easing_and_dies_with_its_entity`, `tween_holds_at_time_scale_zero`, `anim_methods_have_generated_schemas_and_mcp_tools`, `agent_inspects_a_pose_over_mcp`, `skeleton_overlay_golden` (reviewed).

**2.4.5 The animator role and the done criteria** (~0.5 week; D8).
- *Deliverables:* the animator enabled in `team.toml`; its scope (`anim_*`, `asset_import`, `sim_*`, `capture_screenshot`, `component_get`, `assets/anim/**`) under the 50k budget; a new skill `concordia-animation-graphs` with no "not available yet" note (Blender clip authoring stays with the 3.7 bridge, said plainly); sync and roster snapshots; an original fixture character made by our Blender script (idle, walk, attack, hit, death).
- *Tests first:* `animator_is_enabled_with_anim_tools_under_budget`, `animator_skill_is_available`, the sync and roster snapshots (reviewed), `fixture_character_plays_its_states_through_the_graph` (verified by events and a golden).

**Open items from 2.3d, settled here:**
- **`ClipData::frame_rate`:** see A1.
- **Retargeting by bone name:** see A1. Name binding needs only `Name`s: `SkinData::joint_names` and the instance's bone entities, found under the skinned entity's root.
- **The FBX start trim:** tested in 2.4.1 with a hand-written ASCII fixture (A-G5).

**Author decisions (2026-11-21):**
- **A1. One re-import carries three clip and skin changes:** the `Skinned` link from a mesh to its skin, each channel's bone name (`Channel::target_name`), and `ClipData::frame_rate` (`Some` for FBX, `None` for glTF). Clips then bind to any skeleton without their source file, and events can be shown in frames. glTF goes to version 3, FBX to 2.
- **A2. One pose per character:** a hashed `Pose` component on the skinned entity holds every bone's local transform (Godot's `Skeleton3D` model, Gregory's pose arrays). The imported bone entities stay as rest-pose handles and name anchors; items follow a posed bone through `BoneAttachment` (2.4.4), and `anim.inspect` reports live bone transforms.

### Phase 2.4.1 As built (2026-11-21, ADR-0074)
**Reconciled with the code as built:**
- **`Pose` is one `Vec<Transform>`** (an array of structs), not a structure of arrays as first drafted: `Transform` is already reflected, and at 65 bones the layout doesn't matter. It is still one component per character (A2).
- **Skeletons come from the skin's own prefab, not the instance.** Every instance shares one `Skeleton` per skin. Non-joint nodes between joints (Blender's `Armature` with its ×100 scale) become each joint's fixed `offset`, so clips keep driving each bone's local transform. Model space is the prefab root's.
- **Phase:** `FrameEnd`, as settled. `anim.pose` gives a rest pose to players spawned during play (through the command buffer, so they start sampling the next tick); `insert_rest_poses` does the same for the edit world's players when play starts.
- **Library at play start:** every skin of a `Skinned` entity, and every clip in each player's clip file, bound to that player's skeleton. Failures are logged and listed in `LoadReport`, never fatal; those players hold their pose. Lazy loading was rejected because it would make `sim.step 10` differ from ten `sim.step 1` calls (ADR-0074).
- **Engine seams, extended rather than duplicated:**
  - `EngineModule::start_play_in` now takes `&mut Engine` (the module list is cloned first), so a module can load assets; the replay, scripting and test implementations changed only their signatures.
  - `Engine::registry_and_state_mut` sits beside the existing `registry_and_play_mut`.
  - Prefab text was read in two places (`prefab.instantiate` and `concordia_import_model::load_prefab_doc`). Both now use `AssetDb::load_prefab_doc` / `prefab_doc`, which the anim library uses too.
- **The FBX trim:** ufbx resamples curved keys at the rate but keeps keys that are already linear, so the hand-written `late_start.fbx` (two linear keys) imports as two keys from 0 to 1 s. The crate's AGENTS.md now says so.
- **`[anim]`** is a manifest section with no keys, added after `[assets]` in `Project::modules`. Without an asset database the module logs a warning, and nothing animates.

**Engine:**
- `concordia-anim` (new): `Skeleton`, `Joint` and `model_matrix`; `Pose`; `validate_clip` and `BoundClip`; `AnimPlayer` and `advance`; `AnimLibrary` and `LoadReport`; `insert_rest_poses` and `add_processors` (`anim.pose`, `anim.sample`, 64-entity chunks); `AnimModule`; `AnimError`.
- `concordia-assets`: `Skinned`, `Channel::target_name`, `ClipData::frame_rate`, `AssetDb::load_prefab_doc` and `prefab_doc`.
- `concordia-import-model`: `MeshPart::skin` → `Skinned`.
- `concordia-import-gltf` v3 and `concordia-import-fbx` v2 (A1).
- `concordia-api`: `start_play_in(&mut Engine, ..)` and `registry_and_state_mut`. `concordia-app`: `[anim]`, `AnimSection`.

**Tests:**
- `concordia-anim/tests/skeleton.rs` (4): `skeleton_orders_parents_before_children`, `bad_skeletons_are_errors_naming_the_joint` (cycle, out-of-range parent, duplicate and empty names, short inverse binds, a joint missing from the prefab), `model_space_chains_parents_offsets_and_locals`, and `skeleton_bind_pose_matches_inverse_bind_matrices` (`SimpleSkin` and `skinned.fbx`: every rest skinning matrix is the identity within 1e-4).
- `sampling.rs` (6):
  - `clip_sampling_interpolates_trs_linear_slerp_and_step`: slerp at a quarter is 22.5°, where nlerp would give 21.6°; the shortest path is taken; a step key holds from its own time.
  - `cubic_spline_tracks_sample_per_gltf_spec`: hand-computed Hermite values over unequal spacing; rotations normalized.
  - `sampling_outside_the_keys_holds_the_end_values`, including cubic splines and a single key.
  - `looping_wraps_time_and_clamped_clips_hold_the_last_pose`: wrapping, large steps, negative speed, clamping, empty clips, NaN.
  - `malformed_clip_is_a_descriptive_error`: value count, order, finiteness, no keys, a zero rotation, duration; non-unit rotations are normalized.
  - `binding_reports_unknown_bones_and_skips_morph_weights`.
- `binding.rs` (1): `clip_binds_to_another_skeleton_by_bone_name`. `mixamo_run.fbx`'s clip on `skinned.fbx`'s skeleton: at a key time every one of the 7 joints has exactly that key's rotation.
- `play.rs` (6):
  - `library_loads_every_skin_and_the_clip_files_clips`: sibling clips load, and a missing clip is reported.
  - `player_advances_at_fixed_dt_and_time_scale`: 15 ticks at 60 Hz reach 0.25 s, and the pose equals the clip sampled there; time scale 0 freezes playback; scale 0.5 with speed 2 matches normal speed.
  - `pose_is_part_of_the_world_hash`.
  - `anim_in_fixed_step_is_deterministic_across_thread_counts`: 100 characters with mixed clips, speeds, offsets and loop modes; hashes at ticks 1, 60 and 300 are equal for 1, 3 and 8 threads.
  - `players_spawned_during_play_get_a_rest_pose_then_animate`.
  - `switching_clips_returns_bones_the_new_clip_leaves_alone_to_rest` (added after a mutation survived, below).
- Importers:
  - `skinned_parts_get_a_skinned_component_naming_their_skin` (model);
  - `gltf_skinned_node_gets_a_skinned_component` and `gltf_clip_channels_carry_bone_names_and_no_frame_rate` (`SimpleSkin`, `Fox`, `Box`);
  - `fbx_skinned_mesh_gets_a_skinned_component`, `fbx_clip_channels_carry_bone_names_and_the_file_frame_rate` (24 fps) and `fbx_stack_with_a_late_start_is_trimmed_to_zero`.
- App: `anim_module_plays_an_imported_clip_through_sim_step`. `[anim]` and `[assets]`; `prefab.instantiate` of `skinned.fbx`, then `component.set AnimPlayer` with a clip from `anim_stacks.fbx` (a second file, bound by name); 30 ticks reach 0.5 s with a 7-bone `Pose`, the pose moves on, and the edit world never gets a `Pose`.
- Bench: `concordia-anim/benches/sample.rs` (`BASELINE.md`). 1,000 characters × 65 bones: 2.64 ms raw, 2.80 ms per tick on one thread and 0.66 ms on eight. Hashing 1,000 poses takes 43 ms, and a faster float hash is a follow-up.

**Red run:** every test failed first for the right reason:
- the skeleton stubs said `not implemented yet`, so the sampling and binding tests failed building their skeletons;
- `validate_clip` accepted everything, and `advance` returned its input;
- the library loaded nothing, so there was no skeleton and no pose;
- the importers wrote empty bone names, no frame rate and no `Skinned`;
- the app test found no skinned entity.

Changed deliberately during red (my test mistakes):
- `anim_stacks.fbx` holds only the rig and its actions, with no mesh or skin. The play and app tests now play its clips on `skinned.fbx`, which also covers binding by name.
- `entity.get` takes `id`, not `entity`.
- The trim test expected 25 keys, one per frame, but ufbx keeps already-linear keys. It now checks the trim and the endpoints.

These passed at once:
- `gltf_clip_channels_…`'s frame-rate half (`None` was the stub's value); its name half failed.
- The trim itself: the shim already trimmed. It is mutation-checked below.
- `switching_clips_…`: written after the implementation to close a surviving mutation; it fails with that mutation.

**Mutation checks:** 24 mutations, each run against its tests. Caught:
- **Sampling:** nlerp instead of slerp; slerp without the shortest path; cubic tangents not scaled by the segment; holds reading the in-tangent; the key search off by one; rotations not normalized at bind; `%` instead of `rem_euclid`.
- **Skeletons:** the offset dropped; inverse binds transposed; parent joints ignored; joints in index order.
- **Players and the library:** the time scale ignored; speed ignored; no rest pose for spawned players; only the named clip loaded; a missing clip not reported; channels bound by index instead of name; validation of key order off.
- **Importers:** extra parts left unskinned; glTF bone names dropped; FBX frame rate dropped; FBX skin link dropped; no FBX start trim.

One survived: not resetting to the rest pose before sampling. Every fixture clip drives every bone, so nothing noticed. `switching_clips_returns_bones_the_new_clip_leaves_alone_to_rest` now catches it.

**Goldens:** none (nothing renders a pose until 2.4.2). **Snapshots:** none changed. **Editor schemas:** `Skinned` added (`Skinned.ts`, `Skinned.schema.json`); `Pose` and `AnimPlayer` appear only in projects with `[anim]`. **Studio:** no change; the animator unlocks in 2.4.5.

**Checklist:** `just lint`, Tauri clippy, `CONCORDIA_REQUIRE_GPU=1 just test` (1,291 tests and the doc tests), `just gate-tests M1` and `M3S`, `just deny` (and the editor's), `just editor-schemas` (`Skinned` added, committed) and `just editor-test` (118 unit, 22 end-to-end) all pass. The editor's `Cargo.lock` gains `concordia-anim`.

**Not in 2.4.1:** skinning and rendering poses (2.4.2); graphs (2.4.3); events, attachments, tweens and `anim.*` tools (2.4.4); morph targets and root motion (later).

### Phase 2.4.2 refinement (2026-11-21, against 2.4.1 as built)
**Status: no author questions; my calls below (say if you disagree). Done (see 2.4.2 As built).**

**Settled without a question:**
- **Which pose deforms a draw:** the entity's own `Pose`, or else the nearest ancestor's whose `Skinned` names the same skin (an import's extra mesh parts are child entities with `Skinned` but no player). It is found when the entity is extracted. A skinned entity with no pose (the edit world, or no `[anim]`) draws its bind mesh through the unskinned path, as today.
- **The joint palette is built on the CPU, on the render side,** once per posed entity per frame, after interpolation: `K_j = inverse(M) · jointModel_j · inverseBind_j`, glTF §3.7.3's `inverse(meshGlobal) · jointGlobal · inverseBind` with both globals taken relative to the prefab root. `M`, the bind mesh's model matrix, is fixed per skin (`Skeleton::mesh`, the model matrix of the prefab's topmost entity using the skin), so the rest pose's palette is the identity. Every draw the pose deforms is placed at the posed entity's world matrix, and a part's own offset from it is ignored. glTF says the same: the skinned mesh node's transform is ignored, and the joints place the mesh. So the parts and every instanced copy share one palette. The renderer reaches the skeletons through a play-world resource, `Arc<AnimLibrary>`, which the `[anim]` module inserts at play start. Resources other than the clock and the RNG are not hashed. 65 joints cost about 130 matrix products, so a GPU palette pass waits until crowds need it.
- **Skinning runs in the vertex shaders** of the existing vertex-pulling path: `vs` (forward, wireframe and overdraw) and `shadow_vs`, through one `skin.slang` module that both import. `Instance` gains a `uint4 skin` row (first palette matrix, joint count; a count of 0 means unskinned), the palette buffer's index takes the free fourth slot of `Frame.buffers`, and `DrawPush` / `ShadowPush` gain the joint and weight streams. Skinned copies of one mesh still draw as one instanced call. A compute pre-skin pass would pay off only with more passes over the same vertices (M4's visibility buffer and motion vectors), so it is left for then. Normals use the blended matrix's cofactor (exact under non-uniform scale), and tangents the matrix itself; both are normalized. The shaders keep `float4` / `uint4` rows, and there is no `discard` in a vertex shader.
- **Pose interpolation:** an entity's extracted items carry its pose. `RenderScene` blends bones between the last two ticks (lerp, slerp, lerp), as it already does for transforms, and the palette is built from the blend. Rendering only reads: the world and its hash never change.
- **Skinned bounds:** at upload, each skinned mesh keeps one bind-space box per joint, holding the vertices it influences. Each frame a draw's bounds are the union of `K_j(box_j)`. A skinned vertex is a weighted average of `K_j · v` over its joints, so it always lies inside, and the bounds never pop. They are used for culling, cascade fitting and blend sorting. The shadow cache's key includes the palette, so a new pose redraws the shadows.
- **Validation:** a skinned mesh must have joint and weight streams of its vertex count. Every joint index must be below the palette's length, since the shader must never read past it. Otherwise the draw falls back to the bind mesh, with a warning logged once.
- **Vertex readback stays in the test:** `tests/shaders/skin_readback.slang` is a compute kernel that imports the same `skin.slang` the vertex shaders use. The test runs it on the device and reads the results back, with no production API. An image test covers the wiring: a skinned draw and the CPU-skinned mesh drawn unskinned give the same image.
- **The 43 ms pose hash stays a follow-up.** It is not on the render path, since replays hash every 60 ticks. The skinned-crowd measurement is in the render bench notes.
- No ADR is needed: `docs/design.md` doesn't change, and ADR-0074 already says the renderer interpolates poses. This phase adds a consequence to it.

### Phase 2.4.2 As built (2026-11-21, ADR-0074)
**Reconciled with the code as built:** the refinement above held. Details it left open:
- **The bind mesh is per skin.** `bind_mesh` takes the model matrix of the prefab's topmost entity whose `Skinned` names the skin (ties go to the lowest id). `Skeleton::with_mesh` refuses a singular or non-finite matrix.
- **Palettes are built in `RenderWorld::assemble`,** so full extraction (captures) and the scene cache's interpolated worlds share one path. A pose of the wrong size gives no palette, and its draws fall back to the bind mesh (`drop_missing_poses`).
- **The palette-length check covers joints with weight 0.** The shader reads all four joints, so an out-of-range index with weight 0 would still read past the palette. A mutation found this (below).
- **The golden is at the widest stride, not mid-clip.** The fixture's Walk swings by `sin(t·τ)`, so at 0.5 s every rotation is back at zero and the pose is nearly the bind pose. The test captures at 0.25 s (15 ticks).
- **The golden lives with `concordia-app`** (`gpu_skinning_golden`, `tests/golden/anim/gpu_skinning_walk.png`). Only there do import, `prefab.instantiate`, `[anim]` and `capture.screenshot` run end to end, so it also proves the module shares its library with the renderer.

**Engine:**
- `concordia-anim`:
  - `Skeleton::{with_mesh, mesh, palette}` and `bind_mesh`;
  - `Pose::lerp`;
  - `skin_vertices` / `SkinnedVertices`, `joint_bounds` / `JointBounds` and `skinned_bounds` (`skin.rs`);
  - `AnimLibrary::shared_skeleton`;
  - `AnimError::Mesh`;
  - the library's skeletons carry their bind mesh;
  - `AnimModule` inserts `Arc<AnimLibrary>` into the play world.
- `concordia-render` (now depends on `concordia-anim`):
  - `DrawItem::skin`, `EntityItems::pose` (`PoseView`) and `RenderWorld::skins` (`SkinView`);
  - pose blending in `RenderScene`;
  - `skinning.rs` (`SkinFrame`, `Placement`): the palette buffer, each draw's world, bounds and skin row, and fallbacks logged once;
  - `GpuMesh` gains joint and weight streams and a joint count, and `GpuAssets::joint_bounds` holds the per-joint boxes;
  - `Instance` gains a `skin` row, `Frame.buffers.w` holds the palette buffer, and `DrawPush` / `ShadowPush` gain `joints` and `weights`;
  - `shaders/skin.slang`, imported by `forward.slang`'s `vs` and `shadow_vs`;
  - the shadow cache key includes palettes;
  - bench `skinned_crowd`.
- No new methods, components, error codes or MCP tools. Role scopes and editor schemas are unchanged.

**Tests:**
- `concordia-anim/tests/skinning.rs` (7):
  - `cpu_skinning_matches_reference_vertices`: a hand-computed two-bone fixture plus a bone scaled 2× along X. A tip-only vertex goes to (0, 2, 0). A 0.25 / 0.75 blend gives (−0.5, 2, 0). Under the scaled bone the normal follows the inverse transpose, (0.5, 1, 0) normalized, where the matrix itself would give (2, 1, 0). Tangents stay perpendicular, and the bitangent sign is kept.
  - `palette_is_in_the_bind_meshs_space`;
  - `cpu_skinning_keeps_normals_unit` (non-uniform scales and mixed weights; the FBX rig mid-Walk);
  - `rest_pose_skinning_is_the_bind_mesh` (`SimpleSkin` and `skinned.fbx`, within 1e-4 of the mesh's size, and the library shares the same skeleton);
  - `skinning_refuses_what_it_cannot_skin`;
  - `skinned_bounds_contain_every_skinned_vertex` (at rest the bounds are the mesh's);
  - `poses_interpolate_bone_by_bone`.
- `concordia-render/tests/skinning.rs` (8):
  - `skinned_draws_find_their_pose_on_themselves_or_an_ancestor`: itself, a parent, a grandparent, another skin, no pose, the edit world, and a pose of the wrong size;
  - `rendered_pose_interpolates_between_ticks` (alphas 0, 0.25, 0.5 and 1; the world moves too);
  - `presentation_pose_never_changes_world_hash` (the resource, extraction, interpolation and rendering);
  - `gpu_skinning_matches_cpu_skinning`: vertex readback within 1e-4 for three poses, one with non-uniform scale;
  - `skinned_draws_match_cpu_skinned_meshes` (lit, normals and wireframe views): two arms in one instanced call with different palettes, plus a sleeve part with a 3 m offset that is ignored. At most 12 pixels may differ by more than 2 steps from CPU-skinned meshes drawn unskinned.
  - `skinned_meshes_cast_skinned_shadows`: only the floor is in view. The skinned shadow matches the CPU-skinned mesh's, the rest pose's matches the bind mesh's, and a new pose redraws the cached maps.
  - `skinned_bounds_keep_posed_parts_in_view`: only the bent tip is in view, and the draw isn't culled.
  - `skinned_mesh_with_joints_past_the_palette_draws_its_bind_mesh`, with a weighted and a weight-0 out-of-range joint.
- `layout.rs`: `shadow_push_matches_forward_slang` (new); `draw_push_matches_forward_slang` now checks the two new fields.
- `concordia-app/tests/anim.rs`: `gpu_skinning_golden`. The play world's capture at 0.25 s differs from the edit world's bind pose in more than 500 pixels, and it matches the golden.
- Every existing render golden is unchanged: none was re-blessed, and the full render suite passes.

**Red run:**
- `concordia-anim`: all 7 failed. The stubs said `palette: not implemented yet` and `skinning: not implemented yet`, `joint_bounds` returned nothing, `lerp` returned its target, and `with_mesh` ignored its matrix.
- `concordia-render`: 7 of 8 failed. Nothing was posed or skinned, the bind box culled the tip, and the CPU reference had no palette.
- `concordia-app`: the golden test found 0 changed pixels, because the bind mesh was drawn.
- **Passed at once:** `skinned_mesh_with_joints_past_the_palette_draws_its_bind_mesh`, because the stubs drew every mesh unskinned. It guards a fallback and is mutation-checked below. `shadow_push_matches_forward_slang` was written before the push changed and was not seen to fail; deleting `joints` from the Slang struct fails it.

**Changed deliberately during green (my test mistakes):**
- The image tests reused a mesh name across their GPU and CPU variants on one renderer. The renderer keeps meshes by reference, so the "CPU reference" drew stale meshes. Each variant now gets its own name.
- Unlit white is 202 under the default tonemap, not 255, so the culling test now checks for a bright pixel against the black clear color.
- The arms camera moved closer: the poses covered too few pixels for the test's own "the poses show" check.
- The golden moved from 0.5 s to 0.25 s (above).

**Mutation checks:** 26 mutations, each run against its tests. All were re-run after I fixed a stale-build bug in my mutation script: it restored files with their old mtimes, so cargo could keep a mutated build. Caught:
- **CPU:** palette order, the inverse bind dropped, weights ignored, normals moved by the matrix instead of its cofactor, normals or tangents not normalized, the library skipping the bind mesh, bounds not moved by the palette, the pose blend ignoring alpha, and nlerp in place of slerp.
- **Shaders:** joints swapped, weights ignored, normals by the matrix, the palette offset ignored, and the shadow pass not skinned.
- **Renderer:** interpolation ignoring alpha, the pose's world not blended, the shadow key without palettes, culling with the bind box, parts placed at their own transform, no ancestor search, failed poses keeping their draws skinned, the palette index missing from the frame block, and `joints` missing from Slang's `ShadowPush`.
- **Module:** `[anim]` not sharing its library (`gpu_skinning_golden`).
- **One survived:** removing the palette-length check, because the bounds code also refuses a weighted joint with no matrix. The test now has the weight-0 case, which only that check guards, and it catches the mutation.

**Goldens:** one new, `anim/gpu_skinning_walk` (FBX character at 0.25 s of Walk, 320×240), approved by the author before blessing (2026-11-21). No existing golden changed. **Snapshots:** none. **Editor schemas:** unchanged (no reflected type changed).

**Perf** (`concordia-render` AGENTS.md):
- `many_lights` shows no regression: three runs each against main the same day are within noise.
- `skinned_crowd`: 1,000 characters × 65 bones in one instanced call take 6.5–6.6 ms per frame, against 2.8–2.9 ms unskinned.
- The 43 ms pose hash stays a follow-up (not on the render path).

**Checklist:** all of these pass:
- `just lint` and Tauri clippy;
- `CONCORDIA_REQUIRE_GPU=1 just test` (1,308 tests and the doc tests);
- `just gate-tests M1` and `M3S`;
- `just deny` and the editor's;
- `just editor-schemas` (no change);
- `just editor-test` (118 unit, 22 end-to-end).

Both `Cargo.lock`s gain `concordia-anim` under `concordia-render`.

**Not in 2.4.2:** dual-quaternion skinning, morph targets, a GPU palette or compute pre-skin pass (M4, with the visibility buffer), and posed selection boxes in the editor overlay, which still use the bind box.

### Phase 2.4.3 refinement (2026-11-21, against 2.4.2 as built)
**Status: one author decision (B1, 2026-11-21); my other calls below (say if you disagree). Done (see 2.4.3 As built).**

**What exists (checked in code):** `AnimPlayer` and `anim.sample` (one clip per entity; rest pose, then the clip); `BoundClip::sample`; `Pose::lerp` (lerp, slerp, lerp); `AnimLibrary::load` at play start (skins, and every clip of each player's clip file); `Material` and `read_material` / `write_material` (a flat map of named nodes, canonical RON through `concordia_scene::{read_document, write_document}`, validation naming the node) as the pattern for a node-graph file; `MaterialImporter` registered by `AssetDb::new`. `component.set` / `component.patch` write only the edit world (journaled), and Terra views have no filtered queries.

**Author decision B1 (2026-11-21): play-world writes.** `component.set` and `component.patch` take an optional `world: "play"`. It writes the running play world directly (it must exist, or the call is `NotPlaying`), isn't journaled or undoable, is logged with the actor, and takes effect on the next tick. This is the general way agents tweak a running game, and 2.4.4's `anim.set_param` becomes a convenience over it. ADR-0075 records it (it amends ADR-0032's "the play world cannot be edited through the API"). Replay v0 doesn't record these writes, so a recording that uses them fails its checkpoints. The ADR says so.

**Settled without a question (my calls):**
- **Where the format lives:** `AnimGraph` and its reader, writer, validation and importer are in `concordia-assets` (`anim_graph.rs`, `anim_graph_importer.rs`, `AssetKind::AnimGraph`), beside `Material` and `ClipData`. Evaluation is in `concordia-anim`, as material evaluation is in `concordia-material`. `AnimGraphImporter` (`animgraph`, version 1, `animgraph.ron`) is registered by `AssetDb::new` like `MaterialImporter`. `AnimGraph` is registered for `schema.get`.
- **The format** is a flat map of named nodes plus a root, like materials (reflection has no `Box`, and names make errors and agent edits easy):
  - `parameters: {name: Float(default) | Int(default) | Bool(default) | Trigger}`;
  - `nodes`:
    - `Clip(clip: "asset:<id>#clip/<name>", looping)`;
    - `Blend2(a, b, amount: <float param>, filter: [bone names])`;
    - `BlendN(inputs: [...], weights: [<float params>])`;
    - `OneShot(base, shot, request: <trigger>, fade_in, fade_out, curve, filter)`;
    - `TimeScale(input, scale: <float param>)`;
    - `Transition(inputs: [(node, reset)], select: <int param>, xfade, curve)`;
    - `StateMachine(states: {name: node}, start, transitions: [(from, to, switch, advance, condition, xfade, curve, reset, priority)])`.
  - `root`.
  - Knobs are always named parameters, as in Godot, so agents and gameplay drive them the same way. Fade curves are `Linear` or `Smooth` (smoothstep), standing in for Godot's unit `Curve` resources. Canonical text comes from `write_document`. Clips are asset references and bind to the player's skeleton by bone name, as players' clips do.
- **Validation in `AnimGraph::validate`, each error naming the node:**
  - names are `[A-Za-z0-9_-]+`;
  - the root and every referenced node, state and parameter exist, and each parameter has the right type;
  - there are no cycles;
  - every node is reachable from the root, and each node has one parent (node state is per node, so a node can't be ticked twice);
  - every state is reachable from `start` through transitions that aren't disabled; there are no self-transitions;
  - defaults are finite, `Blend2` amounts are in 0..=1, `BlendN` weights are ≥ 0 and there is one per input, fades are ≥ 0, and a transition's `select` default is in range.

  Clips are checked when the library loads (`AnimError::Graph` names the graph and the node). Filter bone names the skeleton lacks are reported, not errors, like unmatched clip channels.
- **Players:** a new `AnimGraphPlayer { graph, floats, ints, bools, triggers, travel }` beside `AnimPlayer`, which is unchanged. If an entity has both, the graph plays, `AnimPlayer` is left alone, and the library's report says so.
  - Missing parameter values fall back to the graph's defaults, and unknown names are ignored.
  - `triggers` (names) and `travel` (state machine node → target state) are requests. `anim.sample` clears both after every evaluation, whether or not they were used, so they never carry over (Godot clears `OneShot`'s `request` the same way).
  - Gameplay that sets parameters in `PostPhysics` is seen the same tick; `component.patch` with `world: "play"` between ticks is seen on the next tick.
- **Graph state** is its own component, `AnimGraphState { nodes: [NodeState] }`, one entry per graph node in name order: clip time, one-shot active and elapsed, the transition's or state machine's current and previous input, fade progress, the transition being faded, the travel path, and whether the current state ended last tick.
  - It is ordinary simulation state, so it is hashed. The player is the input (gameplay writes it), and the state is the output (only `anim.sample` writes it).
  - It is inserted at play start (and by `anim.pose` for players spawned during play), as rest poses are. A state of the wrong size is reset.
- **Library:** at play start, for each (`AnimGraphPlayer`, `Skinned`) pair, the graph is loaded and every clip it names is bound to that skeleton (the graph names every clip it can play, as ADR-0074 point 6 foresaw). Failures are reported, and those entities hold their pose. Graph files edited during play apply at the next play (the library is fixed at play start; hot reload re-imports the file, so the next play sees it).
- **Evaluation** (`BoundGraph::evaluate`, recursive from the root) runs at the tick's dt. Every node writes a full pose: a clip node resets to the rest pose and samples. Each node also returns its playback (position, remaining time, ended this tick).
  - `Blend2` / `OneShot` / cross-fades blend bone by bone: lerp for translation and scale, shortest-path slerp for rotation. Bones outside a non-empty filter keep the first input.
  - `BlendN` clamps weights to ≥ 0 and normalizes them by their sum (all zero gives the first input). Translation and scale are weighted sums. Rotation is the normalized weighted sum, with each quaternion flipped into the first input's hemisphere (order-independent beyond that choice, cheap, and the usual N-way blend).
  - Every input of a blend advances every tick, even at weight 0 (Godot's `sync`), so feet stay in phase. Inactive states and inputs don't advance.
  - Playback passes through: from `Blend2`'s `b` when the amount is > 0.5, otherwise from `a` (Godot's rule), from `BlendN`'s heaviest input, and from the base, input or current state otherwise.
- **One-shot (Godot's `AnimationNodeOneShot`):** its trigger fires the shot (restarting it if it is already active). The shot is reset, then advanced with everything else.
  - Its weight is the curve of min(elapsed / fade_in, remaining / fade_out), so the fade-out ends at the shot's end.
  - It ends when the shot's playback ends, or when a looping shot first wraps (Godot's break-loop-at-end). Then the output is the base.
- **Transition node (Godot's `AnimationNodeTransition`):** when the int parameter names another input, the node switches to it. A `reset` input restarts, and the previous one fades out over `xfade`. An out-of-range value is ignored. There is no transition to self.
- **State machine (Godot's `AnimationNodeStateMachine`):**
  - Each tick, a travel request is resolved first: a path of transitions, or a teleport (reset, no fade) when the target can't be reached from the current state, as in Godot.
  - At most one transition is then taken, decided from this tick's parameters and from whether the current state ended last tick. The next travel step is preferred. Otherwise the first `Auto` transition from the current state is taken whose condition holds (a true bool, a fired trigger, or no condition), in (priority, file order).
  - `Immediate` and `Sync` may switch at any time, and `AtEnd` only when the current state ended last tick. `Enabled` transitions are used only by travel, and `Disabled` ones never.
  - Taking a transition makes the old state the fading one. The destination is reset (`reset`), seeked to the old state's position (`Sync`), or left where it was. The machine then evaluates the new state at this tick's dt.
  - A transition taken during a fade interrupts it: the old fade's source is dropped, as in Godot 4.
  - `travel` takes the fewest transitions. Ties go to the path that comes first in (priority, file order) at each step: a breadth-first search with sorted edges. Godot weights its A* by where the states sit in the editor graph, which our text format doesn't have.
- **Processors:** `anim.pose` also gives graph players a rest pose and an initial state. `anim.sample` runs two passes in fixed 64-entity chunks. First, graph players are evaluated. Second, clip players are sampled, skipping entities that have a graph (a sorted id set; views have no filtered queries). The library's clip pass is unchanged, and the graph pass reuses `BoundClip::sample`, `advance` and the pose blend.
- **Performance:** extend `benches/sample.rs` with 1,000 characters through a state machine over a `Blend2` of two clips plus a one-shot, at 1 and 8 threads. The 43 ms pose hash stays a follow-up: replays hash every 60 ticks, and graph state adds only a few numbers per character.
- **Docs:** ADR-0074 gains a consequence (graphs as it foresaw). ADR-0075 is B1. `docs/design.md` doesn't change.

### Phase 2.4.3 As built (2026-11-21, ADR-0074, ADR-0075)
**Reconciled with the code as built:** the refinement held. Details it left open:
- **Int parameters** join float, bool and trigger: a `Transition` node picks its input by index, as Godot's does.
- **Inside `BoundGraph`:** `bind` validates, indexes nodes in name order and binds clips through a callback. The library passes clips it bound once per skin, so a graph and an `AnimPlayer` share them.
- **Turn order:** a state machine decides its transition at the start of the tick, from this tick's parameters and whether the current state ended last tick. It then evaluates the new state at this tick's dt, so `AtEnd` switches on the tick after the end. A fade drops its source on the tick it completes, without advancing it.
- **Hand-edited state:** `evaluate` resets a state of the wrong size, and any node entry with out-of-range indices or non-finite times (agents can now patch the play world), so evaluation never indexes out of range.
- **Godot features not in 2.4.3 (none was in the plan's list):**
  - one-shot abort and fade-out requests, autorestart and additive mix;
  - transition-node auto-advance and transitions to self;
  - advance expressions and `break_loop_at_end` (a looping state "ends" each time it wraps);
  - `Curve` resources (`Linear` and `Smooth` stand in for them).
- **Playback pass-through** (`Blend2` reports its `b` input's playback above 0.5, `BlendN` its heaviest input's) follows Godot. No test covers it yet; it matters only for `AtEnd` over a blend.
- **The editor overlay's selection box** still uses the bind box (from 2.4.2). Fixing it needs the posed bounds in the overlay's pick path, which isn't trivial, so it is left as is.

**Engine:**
- `concordia-assets`:
  - `AnimGraph`, `AnimNode`, `Parameter`, `TransitionInput`, `StateTransition`, `SwitchMode`, `AdvanceMode`, `FadeCurve` and `AnimGraphError` (`anim_graph.rs`);
  - `read_animgraph` / `write_animgraph`;
  - `AnimGraphImporter` (`animgraph` v1, registered by `AssetDb::new`) and `AssetKind::AnimGraph`;
  - `AnimGraph` registered for `schema.get`;
  - the crate now has a README.
- `concordia-anim`:
  - `BoundGraph` (`graph.rs`) and its evaluator (`graph_eval.rs`);
  - `AnimGraphPlayer`, `AnimGraphState` and `NodeState` (`graph_player.rs`);
  - `AnimLibrary::{insert_graph, graph}`, with graphs and their clips loaded at play start and entities with both players reported;
  - `insert_rest_poses`, `anim.pose` and `anim.sample` extended (graph pass, then clip pass);
  - `Pose::blend_toward` sharing `blend_bone` with `Pose::lerp`;
  - `AnimError::{Graph, GraphFile}`.
- `concordia-api`: `component.set` / `component.patch` with `world: "play"` (B1, ADR-0075). `WorldSel`'s description is now "Which world: the scene being edited or the running simulation."
- Docs:
  - ADR-0075 (new);
  - ADR-0074 (a 2.4.3 consequence) and ADR-0032 (an update);
  - `docs/guides/agents-engine-api.md` (graphs, the player, play writes and the method rows);
  - the AGENTS.md of `concordia-anim`, `concordia-assets` and `concordia-api`;
  - the READMEs of `concordia-anim` and `concordia-assets`;
  - `plans/README.md` (the crate table, the index and the ADR registry; `.animgraph.ron` was already in the conventions).

**Tests:**
- `concordia-anim/tests/graph.rs` (16, on a hand-built two-bone skeleton with constant and ramp clips at dt = 1/8 s, so every expected value is hand-computed):
  - `blend2_at_half_weight_is_normalized_average_pose`: x = 2 between 1 and 3; slerp at a quarter is 22.5° (nlerp would give 21.6°); amounts are clamped.
  - `blend_n_normalizes_weights_and_aligns_hemispheres`: weights 1:3 give x = 2.5; zero or negative weights give the first input; the rotation is (0, 0, 0.562 095, 0.827 070), also for a negated input quaternion.
  - `one_shot_overlays_then_fades_out_on_completion`: x = 3, 5, 5, 5, 4, 3, 2, then 1; restart on refire; smoothstep values 4.375 and 1.625; an arm-only filter.
  - `time_scale_node_scales_playback_rate`: scales 2, 0.5, 0 and −1.
  - `transition_node_switches_input_with_xfade_and_reset`: half-way blends, reset and non-reset inputs, out-of-range picks ignored.
  - `state_machine_travel_finds_shortest_transition_path`: a disabled shortcut is skipped, priority breaks a tie, file order breaks the next, unreachable targets teleport, unknown targets are ignored.
  - `transition_xfade_blends_over_configured_time`: 1.5, 2, 2.5, 3; smoothstep back; an interrupted fade drops its source.
  - `at_end_switch_mode_waits_for_clip_end`: clamped, looping and immediate.
  - `sync_switch_mode_seeks_to_the_current_position`: 100.5 against reset's 100.125; a state without reset continues where it was.
  - `transition_allowed_is_not_transition_taken`.
  - `triggers_and_travel_requests_clear_after_every_evaluation`.
  - `invalid_graph_errors_name_the_node`: an unknown clip, an unknown state, a cycle.
  - `filter_bones_the_skeleton_lacks_are_reported`.
  - `a_state_of_the_wrong_size_is_reset` and `a_corrupt_state_is_reset_not_trusted`.
- `concordia-anim/tests/graph_play.rs` (6, the FBX rig with clips from `anim_stacks.fbx`):
  - `library_loads_graphs_and_the_clips_they_name`: a missing clip is reported naming the node; an entity with both players is reported.
  - `graph_player_shares_the_clip_players_pose_path`: a one-clip graph poses exactly as an `AnimPlayer`; the state is hashed.
  - `graph_parameters_set_in_post_physics_take_effect_the_same_tick`.
  - `graph_players_spawned_during_play_get_a_state_then_animate`.
  - `graph_evaluation_is_deterministic_across_thread_counts`: 100 characters driven by gameplay in `PostPhysics` (bools, triggers, travel); hashes at ticks 1, 60 and 300 are equal for 1, 3 and 8 threads, and every character is posed and animates.
  - `an_entity_with_both_players_plays_its_graph`.
- `concordia-assets/tests/anim_graph.rs` (7):
  - `animgraph_roundtrips_canonical` (insta `character_canonical`);
  - `omitted_fields_take_their_defaults` (Godot's transition defaults);
  - `graph_validation_names_the_node`, `state_machine_validation_names_the_machine` and `bad_weights_and_values_are_errors`, which cover every validation rule;
  - `animgraph_imports_through_the_asset_db` (sidecar id, cache, an invalid file's error names the node);
  - `animgraph_is_registered_for_schema_get`.
- `concordia-api/tests/sim.rs`: `component_set_and_patch_write_the_play_world_unjournaled`. It covers `NotPlaying`, nothing moving until a tick, the edit world untouched, `previous`, a refused bad value, no journal entries, and the log records with actor, method and component.
- `concordia-app/tests/anim.rs`:
  - `anim_graph_plays_an_imported_character_through_sim_step`: an imported `.animgraph.ron` on `skinned.fbx` with clips from a second file; the walk state's pose equals a twin `AnimPlayer`'s.
  - `graph_parameters_set_by_component_patch_take_effect_next_tick`: travel and a bool patched with `world: "play"` change nothing until the next `sim.step` tick, then switch the state; the request is consumed, and the edit world keeps its value.
- Bench: `graph_1000_characters_{1,8}_threads` in `benches/sample.rs` (`BASELINE.md`): 9.30 ms and 2.08 ms for three blended clips per character (a load average of about 17, so upper bounds). The 43 ms pose hash stays a follow-up.

**Red run:**
- `graph.rs`: all 14 first tests failed for the right reason. The stub wrote no pose, accepted every graph and never cleared requests.
- `anim_graph.rs`: 6 of 7 failed on the reader's "not implemented yet".
- `graph_play.rs` and the app tests first failed on the reader stub (the wrong reason), so I ran them again once the reader worked. Then they failed for lack of loaded graphs, poses and states.
- The API test failed with `InvalidParams` (`world` was an unknown field).
- **Passed at once:**
  - `animgraph_is_registered_for_schema_get`: I registered the type along with the stub.
  - `graph_evaluation_is_deterministic_across_thread_counts`, vacuously: gameplay changes the player components every tick, so the hashes differed with nothing animating. It now also requires every character to be posed and 90 or more poses to change, and it failed on the stub.
  - `an_entity_with_both_players_plays_its_graph` and `a_corrupt_state_is_reset_not_trusted` were written after the implementation, to cover code no test reached. Both are mutation-checked below.

**Changed deliberately (my test mistakes):**
- The first `blend2` test was built awkwardly from a bigger graph; it is now two direct graphs.
- The `BlendN` quaternion constants were off in the sixth digit.
- The reset test left `go` set, so it would have faded at once.
- The test clip lookup returned a graph error instead of a clip error.
- The broken-file edit didn't match the canonical text.
- The corrupt-state test pointed at the wrong node index (10, not 8), so its second half tested nothing; caught while checking it.

**Mutation checks:** 29 mutations, each run against its tests, with restored files given a fresh mtime. All were caught. One first failed to build (an unused binding) and was rewritten as a zero fade-in.
- **Blending:** weights not normalized, no hemisphere flip, nlerp instead of slerp.
- **Fades:** a linear smooth curve, the fade-out ignored, the fade-in ignored, the bone filter ignored, a finished fade still advancing its source.
- **Switch modes:** at-end switching at once, sync resetting instead, the reset flag ignored.
- **Travel:** priority ignored, depth-first search, disabled transitions used, no teleport.
- **Advance:** enabled transitions taken as auto, conditions ignored.
- **Timing:** transitions decided after evaluating (one tick late), parameter defaults ignored.
- **Requests:** triggers not cleared, travel not cleared.
- **Robustness:** the state not sanitized.
- **Processors:** the clip pass not skipping graph players, spawned graph players getting no state.
- **Library:** graph clips not loaded, entities with both players not reported.
- **Validation:** reachability skipped, shared nodes allowed.
- **API:** play writes not logged.

**Goldens:** none new or changed. **Snapshots** (each shown to and approved by the author before blessing, 2026-11-21):
- new: `concordia-assets` `anim_graph__character_canonical`;
- changed: `concordia-api` `method_index_snapshot` (the set and patch descriptions);
- changed by the `WorldSel` description only: `concordia-physics` `physics_raycast_validates_its_ray`, `concordia-render` `capture_screenshot_schema_is_reflect_generated` and the M1 gate's `capture_screenshot_input_schema`.

**Editor schemas:** `AnimGraph`, `AnimNode`, `Parameter`, `StateTransition`, `TransitionInput`, `SwitchMode`, `AdvanceMode` and `FadeCurve` are added. `world` is added to the set and patch params, and the `WorldSel` description changes everywhere. All committed.

**Studio:** no change; the role tool budgets still pass with the longer descriptions.

**Checklist:** all of these pass:
- `just lint` and Tauri clippy;
- `CONCORDIA_REQUIRE_GPU=1 just test` (1,339 tests and the doc tests);
- `just gate-tests M1` (4) and `M3S` (13);
- `just deny` and the editor's;
- `just editor-schemas` (changes committed);
- `just editor-test` (118 unit, 22 end-to-end).

**Not in 2.4.3:** events, attachments, tweens and `anim.*` tools (2.4.4); the Godot features listed above, which M13 (Advanced Godot features, ADR-0017, [`M13-advanced-godot-features.md`](M13-advanced-godot-features.md)) takes up; recording play-world writes in replays (ADR-0075).

### Phase 2.4.4 refinement (2026-11-21, against 2.4.3 as built)
**Status: no author questions; my calls below (say if you disagree). Done (see 2.4.4 As built).**

**What exists (checked in code):** Terra signals (`CommandBuffer::signal`, delivered at the start of the next tick by entity then raise order, hashed while pending, `par_for_each_deferred` gives each fixed chunk its own buffer, applied in chunk order); `World::get_json` / `set_json` (by name, through the registry); `concordia_scene::world_transform`; the render scene cache interpolates each entity's world matrix as TRS and poses bone by bone; `Overlays` (selection boxes from the bind `aabb`, gizmos, grid) drawn as screen-space triangles; `SkinFrame` already computes each draw's posed bounds; module methods through `EngineModule::methods` become MCP tools with generated schemas; import sidecars hold per-importer `settings` that are part of the cache key; artifacts are postcard (not self-describing), so a new `ClipData` field needs importer version bumps.

**Settled without a question (my calls):**
- **Where events live:** `ClipData::events: Vec<ClipEvent { time, name }>`, in seconds, sorted by time (authored order kept for ties). Neither format carries them in practice (glTF has no standard for clip events; ufbx exposes no markers), so they are authored in the model's import sidecar, Godot's "import settings" analog: `settings: {"clip_events": {"<clip name>": [{"time": 0.25, "name": "step"}]}}`. `concordia-import-model` parses and validates them once for both importers (finite, `0..=duration`, non-empty names, every named clip exists; errors name the clip and event). glTF goes to v4 and FBX to v3. Events then work for `AnimPlayer` and every graph alike, and a clip shared by many graphs carries its events once. `anim.inspect` shows `frame = time × frame_rate` where the clip has a rate (FBX), else none.
- **Crossing rule** (one pure function, `crossed_events`): a step covers **`[from, to)` forward and `(to, from]` backward**, so an event fires on the tick whose step starts at it or passes it. A clamped clip that reaches its end (or 0, backward) also covers that end. Looping steps are unwrapped (`e + k·duration`), so each crossing fires once, and a step longer than the clip fires once per lap. A zero step (speed 0, time scale 0, a clamped clip at rest) fires nothing. A seek (setting `time`, `Sync`, reset, teleport, a one-shot fire) fires nothing for the jump; the next step starts at the new position, so an event exactly there fires, which is Godot's "a seek fires the key it lands on". This differs from Godot's `(from, to]` only at exact boundaries (one tick earlier there), and needs no hidden "started" state: an event at 0 fires when playback starts and again at each wrap, never twice.
- **Clips inside graphs (Godot's `AnimationMixer`: every animation with a non-zero blend fires its method keys, unscaled):** a clip node fires when it advances and its share of the output pose is > 0. Shares multiply down the tree: `Blend2` gives `b` the amount and `a` the rest; `BlendN` its normalized weights; a one-shot gives the shot its fade weight and the base the rest; cross-fades (transition nodes and state machines) give the destination the shaped fade and the fading-out source the rest; a **filtered** blend or one-shot leaves its first input at full share (it still drives every bone outside the filter, so footsteps under an upper-body attack keep firing). Weight-0 inputs still advance (2.4.3's sync rule) but don't fire; inactive states and inputs don't advance or fire. The share is in the payload, so gameplay can ignore faint events (Godot users dedupe blended footsteps the same way).
- **Order within a tick:** per entity, by when in the step the crossing happened (`(crossing − from) / step`, so blended clips interleave chronologically), then node (name order), then the clip's event order. Across entities, Terra's delivery order (entity id).
- **Signal:** `AnimEvent { name, clip: AssetRef, node: Option<String>, time, weight }`, raised on the skinned entity through each chunk's command buffer in `anim.sample`, delivered at the start of the next tick in entity order (2.0b's rule). It is hashed while pending, like every Terra signal. `World::pending_signals::<S>()` (new, read-only) lets `anim.inspect` show events raised last tick before they are delivered.
- **`BoneAttachment { bone }` (Godot's `BoneAttachment3D`, which must be a child of its `Skeleton3D`):** on an entity whose `Parent` is the posed (`Pose` + `Skinned`) entity, `anim.attach` (in `FrameEnd`, after `anim.sample`) writes its `Transform` to the bone's transform in the posed entity's space, `inverse(bindMesh) · jointModel`, the same space the renderer draws the skin in (`Skeleton::entity_space`, now shared by palettes, attachments, `anim.inspect` and the overlay). Items are children of the attachment entity, as in Godot, and follow through the ordinary hierarchy, so physics, rendering and `component.get` all see them. A missing bone, a parent without a pose or an unknown skeleton leaves the `Transform` alone, logged once per entity. Rendering interpolates the attachment's world matrix like any transform, so between ticks it can differ slightly from the bone (which is interpolated bone by bone); at tick boundaries they agree.
- **Tweens:** a `Tweens { tweens: [PropertyTween] }` component on the entity whose field it animates, so a tween dies with its entity. `PropertyTween { component, field (JSON pointer), to, from (captured when the delay ends, unless given), duration, delay, elapsed, transition, ease }`; values are a number or an array of numbers, interpolated element by element (a 4-element field named `rotation` is normalized). Easing is Godot's `Tween` set: transitions linear, sine, quint, quart, quad, expo, elastic, cubic, circ, bounce, back, spring; eases in, out, in_out, out_in (Penner's equations as Godot writes them). It is **simulation state** (hashed), run by `anim.tween`, an exclusive system in `FrameEnd` before `anim.pose` (so a tweened graph parameter is seen the same tick), at the scaled dt: time scale 0 holds every tween. Several tweens on one field all run, in list order, and the last wins (Godot's behavior). A finished tween writes `to` exactly and is removed; the component goes when empty; a tween whose component or field is gone or isn't numeric is dropped, logged. Writes go through a new `JsonComponent` (a registry-free component codec: `TypeRegistration::json_builder` plus `ReflectComponent`), which `World::set_json` now uses too.
- **Methods:** only `anim.inspect` (play world; pose, every bone's local, model and world transform, the clip player or the graph's node states with state names and effective parameters, the clip's events with frames, and pending events). `anim.set_param` and `anim.travel` are dropped: `component.patch` with `world: "play"` already does both (`{"floats": {"speed": 0.5}}`, `{"travel": {"moves": "idle"}}`), is tested (2.4.3), and is already in every role that steers a game. Each would add about one average tool (≈600 tokens) to the animator's 2.4.5 scope for no new ability (numbers in As built).
- **Skeleton overlay (D7):** `Overlays::skeleton` (`viewport.overlays`, drawn by `capture.screenshot` with `overlays: true`): every posed skeleton as bone lines from each joint to its parent and a dot per joint, on top of the image (no depth test, like Godot's skeleton gizmo). The edit world has no poses, so it draws none there. `SkinView` gains the skeleton and its entity-space joint matrices. **The selection box now uses posed bounds** (the placements' world and bounds, already computed per frame), closing 2.4.2's open item.
- **Performance:** `benches/sample.rs` gains 1,000 characters with four events per second and one attachment each, at 1 and 8 threads. **The 43 ms pose-hash follow-up stays open:** events don't need more frequent checkpoints.
- **No ADR:** nothing changes `docs/design.md` or an ADR's contract. ADR-0074 already says events are Terra signals delivered next tick and items follow bones through `BoneAttachment`; it gains a 2.4.4 consequence (the rules above, importer versions). ADR-0075 already foresaw that `anim.set_param` and `anim.travel` might not be needed.

### Phase 2.4.4 As built (2026-11-21, ADR-0074, ADR-0075)
**Reconciled with the code as built:** the refinement held. Details it left open:
- **A one-shot's shares on the tick its shot ends** are the blend it computed that tick (the shot's fade weight; the base the rest, or 1 when filtered), although the output pose is the base alone. A shot ending with `fade_out` 0 keeps its end event; with a fade-out its share there is 0.
- **Tweens capture `from` in list order**, so a tween that starts in the same tick as an earlier one on the same field sees that write (as in Godot, where tweeners start one after another).
- **`anim.inspect` on an entity with a skin but no player** reports the rest pose (that is what is drawn).
- **One model-space pass per posed character per frame:** `Skeleton::entity_space` feeds both the palette (`skin_matrices`) and the overlay's joints.
- **No ADR:** ADR-0074 gains a 2.4.4 consequence. The 43 ms pose-hash follow-up stays open.

**Engine:**
- `concordia-assets`: `ClipEvent`, `ClipData::events`, `validate_events`.
- `concordia-import-model`: `ModelSettings` (`clip_events` from the sidecar). glTF importer version 4, FBX version 3.
- `concordia-reflect`: `TypeRegistration::json_builder` (`JsonBuilder`). `concordia-terra`: `JsonComponent` (used by `World::set_json`) and `World::pending_signals`.
- `concordia-anim`:
  - `crossed_events` / `Crossing` and the `AnimEvent` signal (`events.rs`), raised by `anim.sample` through chunk command buffers;
  - graph evaluation records crossings and shares (`BoundGraph::evaluate` returns the events);
  - `BoundClip::{events, frame_rate}`;
  - `BoneAttachment` and `anim.attach` (`attach.rs`);
  - `Skeleton::{entity_space, entity_matrix, skin_matrices}`;
  - `Tweens`, `PropertyTween`, `Transition`, `Ease`, `ease`, `add_tweens` (`anim.tween`, `tween.rs`);
  - `anim.inspect` (`methods.rs`), served by `AnimModule::methods`.
- `concordia-render`: `Overlays::skeleton`; `SkinView::{joints, skeleton}`; overlay bones; selection boxes from each draw's placement (posed bounds).
- Docs: `docs/guides/agents-engine-api.md` (events, attachments, tweens, `anim.inspect`, the overlay flag, the method row), ADR-0074's consequence, the AGENTS.md of `concordia-anim`, `-assets`, `-import-model`, `-import-fbx`, `-import-gltf`, `-terra`, `-reflect` and `-render`, `concordia-anim`'s README and `BASELINE.md`, and `plans/README.md`.

**Tests (24 new):**
- `concordia-anim/tests/events.rs` (4):
  - `animation_events_fire_once_per_crossing_including_loops_and_large_steps`: hand-computed crossings and fractions for starts, wraps (landing exactly on one), a 2.5-lap step, clamped ends, backward steps and wraps, zero and NaN steps, and ties in authored order;
  - `events_arrive_as_signals_next_tick_in_entity_order`: entities spawned out of id order; pending after the raising tick, delivered at the next in entity then crossing order; time scale 0 fires nothing;
  - `event_payload_names_the_clip_and_its_time` (and pending events change the hash);
  - `malformed_events_are_descriptive_errors`.
- `graph_events.rs` (2), dt = 1/8 s:
  - `events_from_clips_inside_graphs_follow_the_refined_rule`: `Blend2` shares 0.75 / 0.25 and none at weight 0, a filtered blend keeping its base at 1, chronological order across a `BlendN` (1:3), a one-shot's first tick (0.5 / 0.5), a 0.5 s cross-fade (0.25 / 0.75, then 0.5), an inactive state that never fires, and the payload;
  - `time_scaled_graph_nodes_fire_backward`.
- `attach.rs`: `bone_attachment_follows_bone_world_transform` (a bind mesh 2 m off the root, a 90° hip: the sword at (−1, 0, −2), the blade at (8.5, 0, −2); a missing bone and a parent without a pose hold; a new pose moves it next tick).
- `tween.rs` (5): `easing_curves_match_godots_equations` (32 hand-computed values, every curve's ends), `tween_reaches_target_with_easing_and_dies_with_its_entity` (quad-in 0.25, 1, 2.25, 4; delay capture; despawn; normalized rotations; an expo tween landing exactly), `tween_holds_at_time_scale_zero`, `several_tweens_on_one_field_apply_in_order_and_the_last_wins`, `a_tween_whose_field_is_gone_or_not_numeric_is_dropped`.
- `events_determinism.rs`: `events_and_attachments_are_deterministic_across_thread_counts` (100 characters with clip and graph players, attachments and tweens; hashes at ticks 1, 60 and 300 equal at 1, 3 and 8 threads; over 500 events delivered and every item moved).
- `inspect_schema.rs`: `anim_inspect_schemas_are_reflect_generated` (insta).
- Importers: `fbx_clip_events_come_from_the_import_settings`, `bad_clip_events_are_descriptive_import_errors`, `gltf_clip_events_come_from_the_import_settings`.
- `concordia-render/tests/skinning.rs`: `skeleton_overlay_draws_posed_bones`, `selection_box_follows_the_posed_bounds`.
- `concordia-app/tests/anim.rs`:
  - `imported_character_raises_an_event_and_moves_an_attached_item`: an event authored in the sidecar of the moves file, played on the FBX character through `sim.step`; `anim.inspect` shows it in frames (6 at 24 fps) and pending on tick 15 or 16; a test module's `PostPhysics` processor receives it the next tick; a sword attached in the play world sits on its bone's world transform and its blade moves;
  - `inspected_bones_at_rest_sit_on_the_imported_bone_entities` (all 7, on a moved instance, within 1e-3);
  - `skeleton_overlay_golden`.
- `concordia-mcp/tests/anim_tools.rs`: `anim_methods_have_generated_schemas_and_mcp_tools` (and no `anim_set_param` / `anim_travel`), `agent_inspects_a_pose_over_mcp`.

**Red run:**
- The stubs found no crossings, raised nothing, accepted every event, ignored import settings, moved no attachment, eased linearly, ran no tween, drew no skeleton and boxed the bind mesh, and `anim.inspect` said "not implemented yet". Every test above failed on those, for the right reason, except:
- **Passed at once:** `anim_methods_have_generated_schemas_and_mcp_tools`, because the method was registered with its stub types (as 2.4.3's schema test); the reviewed snapshot backs it.
- **Failed on a stub it depends on:** `imported_character_…` first failed on the `anim.inspect` stub. By the time `anim.inspect` worked, events and attachments were done, so its own assertions were never seen red; mutations (pending events, frames, world space) cover them below.
- **Written after the implementation:** `inspected_bones_at_rest_…`, to check the overlay's bone positions against the imported bone entities; it catches the world-space mutation.

**Changed deliberately (my test mistakes):** `viewport.overlays` takes no `actor`; the world-matrix helper asked `component.get` for a missing `Parent` (an error) and now uses `entity.get`; `query.run` returns `rows`; Godot's expo curves end 0.001 short of 1, so the endpoint check allows that for expo (a finished tween still writes its target exactly); the several-tweens test gives explicit starts. Two found by surviving mutations: the chronological-order case used node names that already sorted chronologically, and the cross-fade was exactly half-way, so swapped shares were invisible.

**Mutation checks:** 41 mutations, each run against its tests with restored files given a fresh mtime; 40 caught, one equivalent:
- **Crossings:** `(from, to]` instead of `[from, to)`, no unwrapping at wraps, the clamped end not covered, backward treated as forward, laps ignored at ties.
- **Order and timing:** node before time, no node tie-break, crossings computed from the new time (a tick late), pending signals unsorted.
- **Graph shares:** weight-0 events kept, a filtered base at 1 − w, cross-fade shares swapped, the shot's share ignored, no events from graphs.
- **Attachments:** the bind mesh offset ignored, the chain multiplied child first, nothing written.
- **Easing:** quad-out, elastic period, bounce constant, back in-out's 1.525, expo in-out's halved offsets, elastic in-out's 0.45, spring exponent, out-in as in-out.
- **Tweens:** finished tweens kept, an empty component kept, unscaled time, the start captured before the delay, the target not exact at the end, broken tweens kept.
- **Import and validation:** events outside the clip accepted, events unsorted, unknown clips accepted.
- **Tools and overlay:** frames ignoring the rate, world bones without the bind mesh's inverse, no pending events, the skeleton not drawn, the selection on the bind box.
- **Equivalent:** removing the explicit zero-step guard in `crossed_events` changed nothing (both helpers already cover an empty range), so the guard was removed.
- Four mutation patterns first missed after rustfmt reflowed lines, and one (an unscaled `dt` through a method that doesn't exist) didn't build; each was retargeted and re-run.

**Goldens:** one new, `anim/skeleton_overlay_walk` (the FBX character at 0.25 s of Walk with the skeleton overlay and its posed selection box, 320×240), approved by the author before blessing (2026-11-21, on a review page beside `gpu_skinning_walk`). No existing golden changed. **Snapshots:** new `concordia-anim` `inspect_schema__anim_inspect_params` and `__anim_inspect_result`, approved by the author before blessing (2026-11-21); none changed. **Editor schemas:** `viewport.overlays` params and result gain `skeleton`; committed.

**Studio and budgets:** no role gains a tool in 2.4.4 (the animator unlocks in 2.4.5), and `every_enabled_default_role_stays_under_its_tool_budget` passes. Measured served sizes: `anim_inspect` ≈ 2,234 tokens (mostly its output schema); `component_patch` ≈ 531, so `anim_set_param` and `anim_travel` would have cost about 1,060 tokens together for no new ability. 2.4.5's planned animator scope (`anim_inspect`, `asset_import`, `sim_*`, `capture_screenshot`, `component_get` and the task and team tools) comes to roughly 13k, under the 50k budget.

**Perf** (`benches/BASELINE.md`): events and attachments add about 1.3 ms per tick for 1,000 characters at one thread (3.88 against 2.59 ms) and 1.2 ms at eight (1.90 against 0.68 ms); the attachment pass is sequential and the bench's bone is the worst case, 64 parents deep.

**Checklist:** all of these pass:
- `just lint` and Tauri clippy;
- `CONCORDIA_REQUIRE_GPU=1 just test` (1,363 tests and the doc tests);
- `just gate-tests M1` (4) and `M3S` (13; `m3s_live_checker_accepts_a_scripted_session_and_rejects_violations` failed once in seven runs, a Studio checker flake unrelated to animation, and passed on every re-run);
- `just deny` and the editor's;
- `just editor-schemas` (changes committed);
- `just editor-test` (118 unit, 22 end-to-end).

**Open items:** the selection box now uses posed bounds (2.4.2's open item, closed). The Metal repro has no FB number yet. 2.4.3's "Not in 2.4.3" line points at M13 (its plan landed after this phase, with M10–M12).

**Not in 2.4.4:** the animator role and its skill (2.4.5); recording play-world writes in replays (ADR-0075); events from glTF extras or FBX markers (no standard; authored in the sidecar); tween loops and sequences (Godot's `set_loops`, chained tweeners); drawing the bind skeleton in the edit world.

### Phase 2.4.5 refinement (2026-11-21, against 2.4.4 as built)
**Status: three author decisions (C1–C3, 2026-11-21); my other calls below (say if you disagree). Done (see 2.4.5 As built).**

**What exists (checked in code):** the animator's role file (disabled; 10 tools, Blender-bridge workflow, `assets/anim/**` and `assets/blender/**`); the roster tests already apply ADR-0073's rule to every role file (`every_scoped_tool_is_used_by_the_role_prompt_or_its_skills`, `every_tool_a_role_skill_tells_it_to_use_is_in_scope`, `enabled_role_prompts_name_only_tools_in_their_scope`); a roster whose scope names a tool the project doesn't offer fails to load, so every Studio project then needs `[anim]` (as `[scripts]` and `[replay]` before it); the server checks `scene.new`, `scene.save`, `asset.import` and `material.set` paths against the role's write paths, `asset.import` by its source path although import writes only the sidecar (and the cache); `viewport.overlays` (the skeleton overlay's switch) is editor-only, so no agent can draw bones; graphs load when play starts, so an `AnimGraphPlayer` must be in the edit world before `sim.step`; scripts resolve engine types by name, but no test shows a guest module receiving an engine signal such as `AnimEvent`.

**Author decisions (2026-11-21):**
- **C1. Preview scenes.** The animator tries graphs in its own scenes under `assets/anim/previews/`: it instantiates the character, sets `AnimGraphPlayer`, saves, plays, steers, inspects and captures. Game scenes stay the level designer's; wiring a graph into one is a handoff.
- **C2. The animator edits model sidecars.** It may write `assets/models/**/*.import.ron` (declared shared with the modeler), never the model. The server's `asset.import` check passes if the role may write the source or the sidecar import writes; existing roles behave the same.
- **C3. Skeletons in captures.** `capture.screenshot` gains `skeleton: true`: every posed skeleton drawn on that image, whatever the editor's overlay state.

**Settled without a question (my calls):**
- **Scope (ADR-0073: every tool the prompt or skills use, nothing else), 22 tools:** the seven task and team tools; `asset_import` (graphs, re-imports after sidecar edits), `asset_list` (clip ids of models others imported), `schema_get` (`AnimGraph`'s shape); `scene_new`, `scene_open`, `scene_save`, `prefab_instantiate`, `query_run` (find the `Skinned` entity), `component_set` (`AnimGraphPlayer`) for previews; `sim_step`, `sim_stop` (graphs reload at the next play), `component_patch` (steering with `world: "play"`), `anim_inspect`, `capture_screenshot` (with `skeleton`), `log_tail` (load failures are logged under `concordia::anim`). Not taken: `component_get` (the plan's; `anim_inspect` reports the pose, states and parameters), `sim_play`/`sim_pause` (stepping is exact), `viewport_overlays` (editor-only; C3 replaces it), `txn_*` (each preview edit is one step), `entity_create` (previews need no light: `view_mode: "normals"` reads the shape). M12's 12.14 widens it later (`rig_*`, `anim_key_*`).
- **Paths:** `assets/anim/**` and `assets/models/**/*.import.ron` (C2). `assets/blender/**` and the `concordia-generate-mesh-blender` skill leave the animator until the 3.7 bridge: it couldn't run a Blender script, and the prompt says so plainly.
- **`[anim]` everywhere Studio is:** `concordia new` and `concordia studio init` add `[anim]` (needs `[assets]`; `studio init` adds `[assets]` too if missing), as 2.6d did for `[scripts]`.
- **Skill `concordia-animation-graphs`:** the graph format (parameters, every node, transitions), wiring and previews, play-world steering (`travel`, `floats`/`bools`, `triggers`), clip events in sidecars (frames at the clip's rate, the crossing rule, shares in graphs), `BoneAttachment`, `Tweens`, `anim_inspect` and `skeleton: true`; Blender clip authoring waits for the 3.7 bridge; example calls parsed as the real params (as the material and replay skills).
- **Other roles:** none gains a skill section. The gameplay programmer receives `AnimEvent` in Rust processors (`receives::<AnimEvent>()`, tested in 2.4.4); receiving it in a guest module is untested, so it stays out of `concordia-scripting` (open item).
- **The fixture character** (`tests/fixtures/character/`, our own content): `character.fbx` from `make_fbx.py` (a new `character` target; the script takes optional target names so other fixtures aren't regenerated); 17 bones with original names (`pelvis`, `spine`, `chest`, `neck`, `head`, `upper_arm`/`forearm`/`hand`, `thigh`/`shin`/`foot` `.L`/`.R`), a box per bone weighted 1 to it, 1.85 m tall; five actions at 24 fps: Idle (24 frames, looping), Walk (24, looping), Attack (15), Hit (9), Death (27, held). `character.fbx.import.ron` fixes its id and holds the events; `character.animgraph.ron` is a five-state machine.
- **Events on odd frames:** at 24 fps an odd frame is half-way between 60 Hz ticks, so each event has one exact tick (no rounding at boundaries). Idle `breathe` frame 11; Walk `step_left` 5, `step_right` 17; Attack `swing` 7; Hit `flinch` 1; Death `fall` 13 and `land` 27 (the end: a held clip reaching its end covers it).
- **The graph:** states idle (start), walk, attack, hit, death. Travel (`Enabled`): idle↔walk (0.1 s), idle/walk→attack (0), idle/walk/attack→death (0). `Auto` on the trigger `hurt`: idle/walk/attack→hit (0). `Auto` + `AtEnd`: attack→idle and hit→idle (0.1 s).
- **`fixture_character_plays_its_states_through_the_graph`** (`concordia-app/tests/anim.rs`, at 60 Hz, one tick per `sim.step`; tick *n* covers state time [*j*/60, (*j*+1)/60) where *j* is ticks since entering): requests are made before the tick named; `anim.inspect`'s `pending_events` after each tick give the raising tick; a listener module checks delivery the next tick:

  | Tick | Request | State after | Raised (node, share 1) |
  |---|---|---|---|
  | 28 | | idle | `breathe` (27.5 ticks) |
  | 31 | travel walk | walk (fade to 36) | |
  | 43 | | walk | `step_left` (31 + 12) |
  | 73 | | walk | `step_right` (31 + 42) |
  | 81 | travel attack | attack | |
  | 98 | | attack | `swing` (81 + 17) |
  | 118 | | attack (clip ends: 38/60 ≥ 0.625) | |
  | 119 | | idle (`AtEnd`) | |
  | 146 | | idle | `breathe` (119 + 27) |
  | 150 | trigger `hurt` | hit | |
  | 152 | | hit | `flinch` (150 + 2) |
  | 173 | | idle (hit ended at 172) | |
  | 180 | travel death | death | |
  | 212 | | death | `fall` (180 + 32) |
  | 247 | | death (held) | `land` (180 + 67) |

  No other events through tick 300 (faded-out sources are clamped or don't cross; a looping death would fall again at 280). **The golden** `anim/fixture_character_states`: five 160×240 panels side by side, one per state at its event's tick (28, 43, 98, 152, 247), lit by a sun, with `skeleton: true`.
- **Open items the done criteria need:** none. Posed-bounds selection is done (2.4.4); attachments between ticks interpolate as transforms (2.4.4's note) and nothing here captures between ticks. **The 43 ms pose-hash follow-up stays open:** this phase hashes no more often than before.
- **No ADR:** C1–C3 change no design or ADR contract. ADR-0073 gains a consequence (the animator's numbers), and ADR-0080's path check is refined (C2) as a consequence there.

### Phase 2.4.5 As built (2026-11-21)
**Reconciled with the code as built:** the refinement held, with these additions:
- **Multi-part models:** the fixture has four materials, so four mesh parts, each with `Skinned`; the parts below the first are its children. The topmost one plays (the bind mesh, 2.4.2), and a player on a child part poses that part alone. The test helper first picked a child part (only one part moved, seen in the first render); it now picks the topmost, and the skill, the prompt, the guide and `concordia-anim`'s AGENTS.md say to play the topmost (`query_run` with `select: ["Name", "Parent"]`).
- **Facing:** Blender's front (−Y) imports as +Z through the FBX exporter's defaults, found by rendering the head from four sides; the golden's camera and sun are in front.
- **The fixture FBX is about 500 KB** (simplified keys; 660 KB without), because each of the five stacks carries curves for all 17 bones.
- **The run goes to tick 300**, not 260, so a death clip that looped (falling again at 280) would be seen.
- **`[anim]` comment:** `concordia new`'s manifest comment avoids the text `[assets]` (a test counts sections by that text).
- **Shared scenes (author's request, 2026-11-21):** the tech artist's prompt saves the scenes it assigns materials in, but it had no `scenes/` path, so the server refused (reproduced with a throwaway test). Scenes are now shared in `team.toml` (`scenes/**`): the level designer edits them but doesn't own them alone, and they aren't locked down yet. The tech artist gets `scenes/**/*.scene.ron`, not all of `scenes/`: the existing `server_rejects_material_writes_outside_the_role_paths` caught that `scenes/**` would also let it write `scenes/x.material.ron`. Tests: `tech_artist_saves_the_scene_it_assigned_materials_in`, `scenes_are_shared_by_the_level_designer_and_the_tech_artist` (both red first; three mutations caught).
- **No ADR.** ADR-0073 and ADR-0080 gain consequences.

**Engine and tools:**
- `concordia-render`: `CaptureScreenshotParams::skeleton` (adds the skeleton overlay to that capture; the editor's overlays are kept). Editor schemas regenerated.
- `concordia-mcp`: `asset.import` passes the path check if the role may write the file or its sidecar (`import_decision`).
- `concordia-cli`: `concordia new` writes `[anim]`; `concordia studio init` adds `[assets]` and `[anim]` when missing.
- `concordia-studio`: the animator enabled (22 tools; `assets/anim/**`, `assets/models/**/*.import.ron`; `concordia-task-workflow`, `concordia-animation-graphs`, `concordia-team-comms`), the new skill, `team.toml` (animator on, model sidecars shared).
- `tools/fixtures/make_fbx.py`: the `character` target, and target names on the command line. `tests/fixtures/character/`: `character.fbx`, its sidecar (fixed id, clip events), `character.animgraph.ron` and a README.
- Docs: `docs/guides/agents-engine-api.md` (the `skeleton` flag, multi-part models, the animator), ADR-0073 and ADR-0080 consequences, `plans/README.md`, the AGENTS.md of `concordia-studio`, `-mcp`, `-render`, `-cli` and `-anim`, the READMEs of `concordia-studio`, `-render` and `-anim`, and the fixture READMEs.

**Tests (11 new, 4 extended):**
- `concordia-studio`: `animator_skill_is_available`; `animation_skill_examples_are_real_params_and_a_valid_graph` (its calls parsed as `InspectParams`, `CaptureScreenshotParams`, prefab params and the animation components, its graph read and validated, its sidecar line parsed as `ModelSettings`); `animator_previews_and_steers_graphs_with_its_tools_and_skill` (the exact 22 tools, paths and skills); `animator_writes_only_anim_files_and_model_sidecars` (the hook); `scenes_are_shared_by_the_level_designer_and_the_tech_artist`. The existing ADR-0073 rule tests now cover the enabled animator.
- `concordia-mcp`: `animator_is_enabled_with_anim_tools_under_budget`; `server_holds_the_animator_to_its_paths` (imports a model through its sidecar, refuses other folders, previews in `assets/anim/previews/`, never game scenes, no `material_set`); `tech_artist_saves_the_scene_it_assigned_materials_in`; the budget test prints every role's size.
- `concordia-cli`: `studio_init_adds_anim_when_missing_for_the_animator`; `[anim]` in the new-project tests.
- `concordia-app`: `capture_skeleton_draws_posed_bones_without_the_editor_overlays` (equal to the editor's overlay, added to the selection box, never replacing it); `fixture_character_plays_its_states_through_the_graph` (the table above through tick 300: 15 state checks, 8 events with node, clip and share 1, delivery the next tick, the graph's canonical text, and the golden).
- `concordia-import-fbx`: `fixtures_hold_no_user_paths` also checks `tests/fixtures/character/`.

**Red run:** every new test failed for the right reason (no animator, no skill, the hook and server refusing its paths, no `[anim]`, the unknown `skeleton` field), except:
- **Failed on a missing fixture, then passed at once:** `fixture_character_plays_its_states_through_the_graph` first failed because the fixture didn't exist. Once it did, every state and event assertion passed on the first run, since the engine features are 2.4.1–2.4.4's; the hand-computed table held. Mutations back it (below).
- **Changed deliberately:** `director_agent_tools_restrict_spawnable_roles` lists the animator; `Stage::with` split into `Stage::empty` and `instance()` picks the topmost skinned entity (above).

**Mutation checks:** 24 mutations, each run against its tests with the restored file given a fresh mtime; all 24 caught: scope (a tool dropped, one added, `component_patch` dropped), paths (sidecars dropped, all of `assets/models/`), `team.toml`'s shared sidecars, the server (source-only import, a sidecar denial allowed), the graph (attack and hit returning immediately, hit without its trigger, a 0.3 s fade, a looping death), event ticks (step on a tick boundary, swing two frames late, land a frame early), the capture flag (ignored, replacing the editor's overlays), the CLI (`studio init` and `new` without `[anim]`), a misnamed skill example, and shared scenes (the tech artist without scene files, with all of `scenes/`, scenes undeclared).

**Goldens:** one new, `anim/fixture_character_states` (five 160×240 panels at ticks 28, 43, 98, 152 and 247, with `skeleton: true`), shown to the author on a review page and committed at the author's go-ahead (2026-11-21). No existing golden changed. **Snapshots:** new `agent_animator`; changed `agent_director` (the animator in its `Agent(...)` list), `agent_tech-artist` (scene files), the CLAUDE.md block (Studio and M3S), M3S's agents and skills, and `capture.screenshot`'s schema (render and M1). **Editor schemas:** `MethodCaptureScreenshotParams.ts` gains `skeleton`.

**Studio and budgets** (served definitions, 4 chars a token): animator 22 tools ≈16.7k, level designer 35 ≈21.5k, tech artist 23 ≈20.0k, QA 22 ≈18.4k, director 13 ≈12.5k, gameplay programmer 16 ≈12.4k, writer 11 ≈9.6k; all under 50k.

**Checklist:** all of these pass: `just lint` and Tauri clippy; `CONCORDIA_REQUIRE_GPU=1 just test` (1,374 tests and the doc tests); `just gate-tests M1` (4) and `M3S` (13; the checker that flaked once in 2.4.4 passed); `just deny` and the editor's; `just editor-schemas` (committed); `just editor-test` (118 unit, 22 end-to-end).

**Open items:** guest modules receiving `AnimEvent` are untested, so `concordia-scripting` doesn't mention it (Rust processors receive it, 2.4.4). The 43 ms pose-hash follow-up stays open. The Metal repro has no FB number yet.

**Not in 2.4.5:** Blender clip authoring (the 3.7 bridge); the animator wiring graphs into game scenes (a handoff, C1); M12's `rig_*` and `anim_key_*` tools.

### Phase 2.4b refinement (2026-11-21, against the code after 2.4.5)
**Status: author decisions E1–E3 taken (2026-11-21); my other calls below (say if you disagree). 2.4b.1 and 2.4b.2 done (see their As built).**

Refined against the code as built. The first test list (under *New-phase tests*) and M7's 7.4 and 7.6 notes are the starting point.

**What exists (checked in code):**
- **No LOD code anywhere.** Terra has processors in five phases, deterministic parallel waves, shared, chunk and tag storage, command buffers, signals (delivered at the start of the next tick, then cleared) and observers. `SystemCtx` carries one `dt` for the whole tick.
- **Chunk components are logical (`CHUNK_ROWS` = 1024) and never hashed;** the rule is that simulation may not depend on them unless they are rebuilt from hashed state. 2.0b left "processor access to chunk components" for 2.4b's first consumer.
- **Instancing already exists:** extraction gives one `DrawItem` per `MeshRenderer` (in entity order), and the renderer merges consecutive instances of one mesh and material into one instanced call (skinned copies too, each with its own palette). So "instanced representation" means switching an entity to a cheaper, unskinned mesh, not new instancing machinery.
- **Costs at scale:** extraction builds an `EntityItems` per drawn entity and the scene cache interpolates every entity's matrix; 2.4.2's bench drew 1,000 skinned characters in one call (6.5 ms at 1080p). Nothing has measured 100k entities.
- **`terra.stats`** reports counts, archetypes, columns, shared values, skipped commands and per-phase waves; no LOD buckets.
- **The animation processors** (`anim.sample`, `anim.attach`, `anim.tween`) run every tick for every player; the pose is hashed simulation state.

**Gaps:**
| # | Gap | Resolution |
|---|---|---|
| L-G1 | Nothing marks where detail matters | `LodViewer`, a hashed component on simulation entities (the player, a camera rig that is part of the scene); never the render camera (M7 7.4) |
| L-G2 | Hysteresis needs memory, and chunk components aren't hashed | Each entity's level and last-tick time are a hashed component (`SimLod`), rebuilt every tick from hashed inputs |
| L-G3 | One `dt` per tick can't serve entities that ticked 4 ticks ago | LOD-aware processors get each entity's elapsed scaled time since it last ticked |
| L-G4 | A signal to an entity skipped this tick would be cleared unseen | Rule below |
| L-G5 | No cheaper mesh to switch to | `Representation` (presentation-only): the cheap mesh and material and the distances |
| L-G6 | No way to see LOD | `view_mode: "lod"` (engine-drawn, D7) and LOD buckets in `terra.stats` |

**Author decisions (2026-11-21):**
- **E1. Staggered ticks.** An entity at rate r ticks when (tick + phase(id)) mod r = 0, with phase(id) a fixed hash of its id, so each tick does an even share of the far work. Each entity gets its exact elapsed scaled time since it last ticked.
- **E2. Representation per frame, no memory.** The renderer picks full, instanced or hidden by distance from the camera drawing that frame; every capture and viewport is right for its own camera. Hysteresis is added later only if flicker shows.
- **E3. Animation is the first consumer.** `anim.sample` opts in: far characters pose at their LOD rate, advancing clips and graphs by their elapsed time, so events and attachments stay correct. Measured on the 1,000-character bench.
- **Also:** the stale open questions (moved to M5 or answered by 2.0b) are cleaned up in 2.4b.1's commit.

**Settled without a question (my calls; say if you disagree):**
- **Levels and settings:** levels 0 (full) to 3. A shared component `LodProfile { distances: [d1, d2, d3], rates: [1, r1, r2, r3], hysteresis }`, so a template's trait sets it once per archetype; entities without a profile are always level 0.
- **No viewer means full detail:** with no `LodViewer` in the world every entity is level 0, so headless tests and scenes without a player behave exactly as today.
- **Level storage:** `SimLod { level, last_time }` is a dense hashed component (not chunk data, which isn't hashed; not a shared component, whose archetype moves would make every transition a structural change). A `terra.lod` exclusive system at the start of `PrePhysics` computes it: nearest viewer distance (squared, from world transforms), hysteresis as a margin on the way back up, then the due rule.
- **Opt-in per processor:** `ProcessorDecl::lod()` makes a processor LOD-aware. Its queries visit only entities due this tick, and `ViewQuery` gives each one its elapsed scaled time (`clock.time − last_time`), exact under time-scale changes. Others run every tick as now.
- **Signals (L-G4):** receivers see every signaled target whatever its LOD; LOD throttles only the regular query. `signals_survive_lod_changes` covers a level change on the tick a signal is raised and on the tick it is delivered.
- **Representation (L-G5):** `Representation { instanced: Option<MeshRenderer>, instanced_from, hidden_from }`. Full uses the entity's own `MeshRenderer` and pose; instanced draws the cheaper mesh unskinned; hidden draws nothing. It never touches the world hash.
- **ADR-0076 (Terra LOD):** the viewer rule, the hashed level and elapsed time, the opt-in and the signal rule are a new determinism contract next to ADR-0063 and ADR-0069; `docs/design.md` doesn't change.
- **Sub-phases (one PR each, off main):** **2.4b.1 Simulation LOD** (viewers, profiles, `SimLod`, the due rule, opt-in processors, signals, `terra.stats` buckets, ADR-0076) and **2.4b.2 Representation LOD** (`Representation`, extraction, the `lod` view mode, the 100k instanced golden and a perf baseline). Branches `m2/2.4b.<n>-<slug>`.
- **Tests:** the first list, plus `no_viewer_means_full_detail`, `lod_processors_get_exact_elapsed_time_under_time_scale_changes`, `non_lod_processors_still_run_every_tick`, `lod_is_deterministic_across_thread_counts` (100k entities, 1 and 8 threads, hashes at ticks 1, 60 and 300), `terra_stats_reports_lod_buckets` and a `lod` view-mode golden; hand-computed due ticks and levels throughout.
- **Studio:** no role change; the level designer already has `terra_stats` and `capture_screenshot`, and its skill gains a short LOD section (`LodProfile` in templates, `LodViewer`).

### Phase 2.4b.1 refinement (2026-11-21, against the code)
**Status: no author questions; my calls below (say if you disagree). Done (see 2.4b.1 As built).**

**Checked in code:** `View` hands processors disjoint column borrows, and `ViewQuery` cuts each archetype into fixed `chunk`-row pieces; `for_each_of` reaches signaled entities by location; `SystemCtx` has one `dt`; the clock adds `dt_exact` to `time` after the schedule runs; transforms (and `world_transform`) live in `concordia-scene`, above Terra; the core module's `headless_schedule` is the first schedule of every play world; `anim.sample` uses `ctx.dt` for every player; `terra.stats` has no LOD fields.

**Settled without a question (my calls):**
- **Due rule (E1):** `phase(id)` is SplitMix64 of the id's two 64-bit halves XORed, top 32 bits; due when `(tick + phase) mod rate = 0`, computed without overflow. **An entity is also due on any tick its level changes** (both ways), so something walking closer updates at once and something walking away takes its last fine step. A new `SimLod` starts at level 0, so its first tick is always due (a changed level, or level 0's rate of 1).
- **Elapsed:** when due, `elapsed = end − last_time` and `last_time = end`, with `end = clock.time + dt_exact`, the same sum the clock makes after the tick, so it is exact under time-scale changes (time scale 0 gives 0).
- **`SimLod { level, due, elapsed, last_time }`** (dense, hashed). `due` and `elapsed` are stored rather than recomputed in views, so a view needs only the `SimLod` column, not the profile or the clock.
- **`terra.lod`** is an exclusive system in Terra, built by `lod_system(position)`. Terra doesn't know transforms, so the scene supplies `lod_position` (the world translation; a parented entity goes through `world_transform`). `headless_schedule` adds it first, so it runs in `PrePhysics` right after `terra.timers`, before every module's processors. It adds and removes `SimLod`s, then computes levels in parallel (1,024-entity pieces).
- **Levels:** the nearest viewer by squared distance (no square roots). A boundary belongs to the far side (d ≥ dᵢ is level *i*); detail comes back only inside dᵢ − h. **h must be below d₁** (otherwise the inner level could never be regained); a profile breaking a rule in a world (one that skipped `LodProfile::new`) means level 0. Losing or gaining several levels at once jumps straight there.
- **Profile shape:** `rates: [r1, r2, r3]` for levels 1–3 (level 0 is always every tick), not the four-element array the refinement sketched; distances and rates ascending (equal allowed).
- **Opt-in API:** `ProcessorDecl::lod()` adds a read of `SimLod`. All of that processor's queries skip rows that aren't due; `for_each_due`, `par_for_each_due` and `par_for_each_deferred_due` hand each entity `Due { dt, dt_exact, level }`, and the plain methods are those with the step ignored. Pieces are still cut every `chunk` rows, skipped rows included, so chunking depends on neither LOD nor thread count. Entities without a `SimLod` are due every tick with the tick's `dt` at level 0; so is every entity in a processor that didn't opt in.
- **Signal rule:** `View::signals` and `ViewQuery::for_each_of` ignore LOD.
- **Animation (E3):** `anim.sample` opts in; clips and graphs advance by `Due::dt`, and events use the crossing rule on the whole step since the last pose. `anim.pose`, `anim.attach` and `anim.tween` stay full-rate: rest poses must appear at once, a held pose places an attachment the same way every tick, and tweens animate arbitrary entities that have no profile. **Rendering is unchanged:** the scene cache still interpolates the pose tick to tick, so a far character moves over the tick it poses on and holds between. Spreading that over its interval is a representation question for 2.4b.2.
- **`terra.stats`:** `lod: { viewers, levels: [4 counts], due }`. The level designer is the role with `terra_stats`; its budget impact is reported in As built.
- **Templates:** a built-in `Lod` trait (`distances`, `rates`, `hysteresis`, checked by `LodProfile::new`) adds the shared `LodProfile` and requires `terra.lod`.
- **ADR-0076** records the viewer rule, the hashed level, elapsed time, the opt-in and the signal rule.

### Phase 2.4b.1 As built (2026-11-21, ADR-0076)
**Reconciled with the code as built:** the refinement and my calls above held. Details they left open:
- **No sentinel level:** a new `SimLod` starts at level 0. Its first tick is due anyway: either its level changes, or it stays at level 0, which ticks every tick.
- **Invalid profiles in a world** (hand-edited, bypassing `LodProfile::new`) give level 0; `validate` runs per entity in `terra.lod`.
- **`terra.lod` runs in wave 1 of `PrePhysics`**, alone, right after `terra.timers` (an exclusive system conflicts with everything), as `terra.stats` shows.
- **`concordia-terra` builds at `opt-level = 1` in dev** (`Cargo.toml`): the 100k-entity, 300-tick determinism test took 38 s unoptimized, too close to nextest's 120 s guard under a parallel `just test`; it now takes 13 s.

**Engine:**
- `concordia-terra`: `lod.rs` (`LodViewer`, `LodProfile`, `SimLod`, `Due`, `lod_phase`, `is_due`, `lod_system` / `LOD`, `LodPosition`, `lod_stats` / `LodStats`, `register_lod`); `ProcessorDecl::lod` / `is_lod`; `ViewQuery::{for_each_due, par_for_each_due, par_for_each_deferred_due}`, with every view iteration going through one row visitor that applies the due filter; `View` carries the tick's step.
- `concordia-scene`: `builtin::lod_position`; `headless_schedule` adds `terra.lod` first.
- `concordia-terra-sim`: the built-in `Lod` trait; `register` also registers Terra's LOD types.
- `concordia-anim`: `anim.sample` is LOD-aware and advances by `Due::dt`.
- `concordia-api`: `terra.stats` gains `lod` (`LodBuckets`: `viewers`, `levels`, `due`).
- Studio: the level designer's `concordia-terra-templates` skill gains a "Simulation LOD" section (the `Lod` trait, `LodViewer`, `terra_stats`' `lod`).
- Docs: ADR-0076; `docs/guides/agents-engine-api.md` (the templates section and the `terra.stats` row); the AGENTS.md of `concordia-terra`, `-terra-sim`, `-scene`, `-anim` and `-api`; the READMEs of `concordia-terra`, `-terra-sim` and `-anim`; both `BASELINE.md`s; `plans/README.md` (status, ADR registry); this plan's open questions, with the port ones moved to M5's (11–14).

**Tests (19 new, 2 extended):**
- `concordia-terra/tests/lod.rs` (13), at 64 Hz so every step is exact; the six test ids' phases (mod 2, 4 and 8) were computed separately and are in the file's header:
  - `lod_phase_is_a_fixed_hash_of_the_id` (six pinned phases; `is_due` at both ends of the ranges);
  - `far_entities_tick_at_their_lod_rate` (levels 0–3 plus an unprofiled entity over 16 ticks: due ticks [0, 2, 4, …], [0, 4, 8, 12], [0, 2, 6, 10, 14] (the same level, another phase) and [0, 4, 12]; elapsed 1, 4 and 8 steps; `last_time`; `lod_stats` at tick 15: levels [1, 1, 2, 1], 1 due);
  - `lod_transitions_are_hysteresis_damped` (15 ticks of hand-picked distances around 10/20/40 m with a 2 m margin: boundaries belong to the far side, exactly d − h still stays coarse, jumps of several levels, and due on every change, odd ticks included);
  - `level_rule_uses_squared_distances_with_a_margin`, `malformed_profiles_are_descriptive_errors` (eight cases, including a margin as wide as the first distance), `invalid_profiles_in_the_world_mean_full_detail`;
  - `simulation_lod_depends_only_on_deterministic_inputs` (reverse insertion order, archetype churn, chunk data and a presentation-only "camera" moving elsewhere: equal levels, visits and hashes; a different viewer path differs);
  - `signals_survive_lod_changes` (raised on ticks 1, 6 and 8 at an entity due on [0, 4, 6, 8, 9, 12]; delivered on 2, 7 and 9, two of them not due, one a level change on the raise tick and one on the delivery tick; `for_each_of` reaches every target);
  - `no_viewer_means_full_detail` (every tick, `dt`, level 0; a viewer appearing, then going (level 0 and due at once); a removed profile removes the `SimLod`);
  - `lod_processors_get_exact_elapsed_time_under_time_scale_changes` (scales 1, 0.5, 0, 2, 1: elapsed 2, 5, 1, 12 and 10 half-steps at level 2, every tick's exact `dt` at level 0, and both sum to the clock's time exactly);
  - `non_lod_processors_still_run_every_tick` (a non-LOD processor reading `SimLod` beside a LOD one: 16 visits each with the tick's `dt` at level 0);
  - `the_nearest_viewer_sets_the_level` (two viewers);
  - `lod_is_deterministic_across_thread_counts` (100,489 entities, two moving viewers, hashes at ticks 1, 60 and 300 equal at 1 and 8 threads; every level holds over 1,000 entities and under half are due).
- `concordia-anim/tests/lod.rs`: `far_characters_fire_events_once_on_the_right_tick_and_match_full_rate_poses` (8 Hz, near and far clip and graph players: the far clip player is due on [0, 1, 5, 9, 13] and the far graph on [0, 2, 6, 10, 14]; at those ticks their poses equal full rate's, and they hold between; events `left`/`right` raised on ticks 5 and 13 (clip) and 2 and 10 (graph), once each, against 2 and 10 at full rate).
- `concordia-scene`: `lod_position_is_the_world_translation` (a scaled parent), `headless_schedule_runs_simulation_lod_from_world_positions`.
- `concordia-terra-sim`: `lod_trait_sets_a_shared_profile` (one shared value for two entities; requires `terra.lod`; a bad profile names the rule).
- `concordia-api`: `terra_stats_reports_lod_buckets` (distances 5, 15, 30, 50 and 60: levels [1, 1, 1, 2], 5 due on the first tick; `terra.lod` in `PrePhysics` wave 1; the edit world's buckets).
- `concordia-studio`: `terra_templates_skill_lod_examples_are_real_params_and_a_valid_template` (the `Lod` block compiled as a template, `LodViewer` and `terra_stats` calls parsed).
- Extended: `template_schemas_come_from_reflection` lists the built-in `Lod` (expected knock-on); `non_lod_processors_still_run_every_tick` now also reads `SimLod` (see the mutations).

**Red run:** against stubs (no filtering, phase 0, level 0, no validation, no `SimLod`, no `Lod` trait output, no `lod_position`, `terra.stats` without `lod`, `anim.sample` at full rate), all 12 first Terra tests and the 5 cross-crate ones failed for the right reason. Not seen red:
- **Written after the implementation:** `the_nearest_viewer_sets_the_level` (no first test told nearest from first viewer; it catches the farthest-viewer mutation) and the skill-example test (the skill section was written with the code; a guard that its examples parse).
- **Changed deliberately (my test mistakes):** entities that already have a viewer get their real level on their first tick, not 0; the noisy world spawned one entity more than the plain one (entity existence is hashed), so both now spawn the camera holder; a hysteresis margin at least as wide as the first distance would make the inner level unreachable, so it became a validation error instead of a level-rule case.

**Mutation checks:** 22 mutations, each run against its tests with the restored file given a fresh mtime; all 22 caught:
- **The due rule:** phase ignored, off by one, other hash bits, one half of the id, not due on a level change.
- **Hysteresis and levels:** no margin, the margin applied going out too, boundaries exclusive.
- **The viewer rule:** the farthest viewer, no viewer measured from the origin, invalid profiles used anyway.
- **Elapsed time:** missing this tick's step, `last_time` not advanced, the tick's `dt` handed out instead.
- **Views:** no due filter; filtering processors that didn't opt in (caught only once the non-LOD test read `SimLod`).
- **The signal rule:** signal targets filtered by due.
- **Bookkeeping:** a stale `SimLod` kept, due not counted in `lod_stats`.
- **Animation:** the tick's `dt` instead of the elapsed step, for clips and graphs; `anim.sample` not LOD-aware.

**Goldens and snapshots:** none new or changed. **Editor schemas:** `MethodTerraStatsResult.ts` gains `lod`; new `Lod`, `LodProfile`, `LodViewer`, `SimLod` and `F64` types and schema fixtures; committed.

**Studio and budgets** (served definitions, 4 chars a token): the level designer is the only role with `terra_stats`; 35 tools, about 21.5k → 21.7k. Animator 22 tools ≈16.7k, tech artist 23 ≈20.0k, QA 22 ≈18.4k, director 13 ≈12.5k, gameplay programmer 16 ≈12.4k, writer 11 ≈9.6k (unchanged); all under 50k.

**Perf** (both `BASELINE.md`s): `terra.lod` costs about 65 ns per profiled entity on one thread (6.55 ms at 100k; 2.02 ms on eight), three quarters of it position lookups (a follow-up for M7's scale: positions in query order). For animation it pays well: 1,000 characters 2.72 → 1.90 ms per tick on one thread, and 100k characters 280 → 70 ms (one thread) and 52 → 23 ms (eight). Existing rows are unchanged. **The 43 ms pose hash is unchanged by LOD:** held poses are hashed like any other, and the cost follows the number of poses; the follow-up stays open.

**Checklist:** all of these pass: `just lint` and Tauri clippy; `CONCORDIA_REQUIRE_GPU=1 just test` (1,393 tests and the doc tests); `just gate-tests M1` (4) and `M3S` (13; `m3s_live_checker_accepts_a_scripted_session_and_rejects_violations` passed, no flake seen); `just deny` and the editor's; `just editor-schemas` (committed); `just editor-test` (118 unit, 22 end-to-end).

**Open items:** guest modules receiving `AnimEvent` are still untested (this phase doesn't touch scripting). The renderer interpolates a far character's pose tick to tick, so it moves over its due tick and holds between; whether to spread that over the interval is 2.4b.2's call. Processor access to chunk components still has no consumer (ADR-0069 expected LOD to be it; `SimLod` is dense instead).

**Not in 2.4b.1:** representation LOD, the `lod` view mode and the 100k instanced golden (2.4b.2); LOD for `anim.attach` and `anim.tween`; per-processor rates; a query-order position path.

### Phase 2.4b.2 refinement (2026-11-21, against the code after 2.4b.1)
**Status: no author questions; my calls below (say if you disagree). Done (see 2.4b.2 As built).**

**Checked in code:** extraction (`RenderWorld::extract`, `EntityItems::extract`) and the scene cache are camera-agnostic, and the camera is known only inside `Renderer::render` (a world camera or the editor's `View`); `assemble` builds a joint palette for every posed character; consecutive instances of one mesh and material already merge into one instanced call (shadows too); the overlay draws selection boxes from each draw's placement and skeletons from `RenderWorld::skins`; `viewport.pick` tests a `View`'s ray against `RenderWorld::draws`; view modes are one switch in `forward.slang` (`ViewMode` order); presentation-only types are registered per world (`World::register_presentation`), and no module does it for its own types.

**Measured first** (`benches/crowd_100k.rs`, 1080p, a shadow-casting sun, cheap cubes, one draw call):

| Stage | 1,000 | 100,000 |
|---|---|---|
| Full extraction | 1.2 ms | 134 ms |
| Scene-cache update after every entity moved | 1.4 ms | 178 ms |
| Interpolation (`world_at`) | 0.2 ms | 26 ms |
| The frame (CPU, submit, readback) | 2.1 ms | 26 ms |

Extraction is dominated by `world_transforms` (34 ms: a chain `Vec` and a `BTreeSet` per entity, then `BTreeMap` inserts) and by about ten `World::get` calls per entity (two or three hash lookups each). And `assemble` builds a palette for every posed character, about 3 µs each, so 100k far characters would cost about 300 ms whatever they are drawn as.

**Settled without a question (my calls):**
- **`Representation { instanced: Option<MeshRenderer>, instanced_from, hidden_from }`** in `concordia-render`, `#[reflect(presentation)]`: a new reflect flag, so a world treats the type as presentation-only the first time it stores it. It is never hashed and off limits to simulation, in edit and play worlds alike, with no per-world registration. Distances are finite, at least 0, and `instanced_from ≤ hidden_from`. An invalid one is drawn full and logged once.
- **Chosen per frame from the drawing camera (E2)**, in the renderer, not in extraction or the scene cache, which stay camera-agnostic. `RenderWorld::represented(eye)` gives the world as that camera draws it: distance from the eye to the owner's world position, with boundaries on the far side (d ≥ `instanced_from` is instanced, d ≥ `hidden_from` is hidden). No memory between frames. Captures, viewports and picks each use their own camera.
- **Multi-part characters:** a draw takes its own entity's `Representation`, else its pose owner's (the bind mesh, 2.4.2), measured from the owner. Instanced: the owner's draw becomes the cheap mesh (unskinned, its own material and `cast_shadows`), and the other parts aren't drawn. Hidden: no part is drawn.
- **What follows from a draw being gone or unskinned:** a hidden entity casts no shadow, isn't picked and has no selection box. An instanced one is picked and boxed by the cheap mesh's bounds and casts its shadow, and it has no palette and no skeleton overlay. **Palettes are built per frame, after selection, only for draws drawn skinned**; `RenderWorld` keeps the extracted poses (`poses`), and `represented` builds `skins` from them.
- **The `lod` view mode (D7)**, last in `ViewMode`, flat-shaded: hue from simulation LOD when the entity has a `SimLod` (play world; level 0 green, 1 yellow, 2 orange, 3 red), white without one (the edit world, unprofiled entities); instanced draws at half brightness; hidden ones aren't drawn. The legend is in `ViewMode`'s docs, the agent guide and the editor's view-mode list ("LOD").
- **G8 and 100k: what changes here.** The one pass already works (one draw call per mesh and material). For a usable frame I fix what is local, with no new architecture:
  - palettes only for drawn skinned characters (above), so 100k instanced characters need none of 2.4.2's palette follow-ups (3×4 rows, parallel or GPU palettes);
  - world transforms without the per-entity chain allocations for entities without a parent;
  - one entity lookup per extracted entity instead of about ten (a read-only `World::entity(id)` view in Terra).
  
  A columnar, incremental extraction for crowds that move every tick stays M7's large-scale work. The numbers after are in As built.
- **Templates** carry `Representation` as a plain component value (`components: {"Representation": ...}`). No trait: it is presentation-only and needs no processor. The level designer's skill gains a short section.
- **No ADR:** ADR-0076's consequences gain the representation rules. The reflect flag is a small extension of ADR-0063's "presentation-ness belongs to the type", recorded there as a 2.4b.2 consequence.

### Phase 2.4b.2 As built (2026-11-21, ADR-0076)
**Reconciled with the code as built:** the refinement held. Details it left open:
- **Ownership is carried, not recomputed:** `DrawItem::representation` holds `(owner, Arc<Representation>)`. The `Arc` matters at 100k: copying the value (two string asset refs) on every draw clone cost about 10 ms a frame.
- **`represented` copies only what it keeps.** A first version cloned the whole world and then replaced the draws, copying 100k draws twice (46 ms frames).
- **`World::get` does one lookup of each kind** (type, then entity) and returns at once for a type the world never stored. Before, a miss looked up both twice through `get_shared`. Extraction's ten gets per entity were most of its cost.
- **`world_transforms`** composes roots (no `Parent`) directly, without the chain and cycle-set allocations, and gives bit-identical matrices (the identity times M is exactly M).
- **`RenderWorld::assemble_owned`**: `world_at` and `extract` assemble from owned items instead of cloning them again.
- **The 100k golden uses unlit materials** at 0.9 m: lit 0.6 m spheres are under a pixel at 480×270 and read as near-black.

**Engine:**
- `concordia-reflect` (and the derive): `#[reflect(presentation)]`, `TypeInfo::presentation` / `with_presentation`.
- `concordia-terra`: worlds learn presentation-only types from reflection when they first store them; `World::get` with one lookup of each kind.
- `concordia-scene`: the root fast path in `world_transforms` and `world_transform`.
- `concordia-render`: `Representation` (`representation.rs`: `validate`, per-frame choice, `RenderWorld::represented`); `DrawItem::{representation, instanced, sim_lod}`; `RenderWorld::poses`; palettes built in `represented`; `RenderWorld::assemble_owned`; `RenderError::InvalidRepresentation`; the renderer resolves each frame for its camera and logs invalid ones once; `pick` resolves for its view; `ViewMode::lod` (`forward.slang`, the instance row's `skin.z`); `benches/crowd_100k.rs`.
- Editor: "LOD" in the view-mode list; schemas regenerated.
- Studio: the level designer's skill gains "Representation LOD".
- Docs: ADR-0076 and ADR-0063 updates; `docs/guides/agents-engine-api.md`; the AGENTS.md of `concordia-render`, `-reflect` and `-terra`; `concordia-render`'s README; `plans/README.md`.

**Tests (10 new, 4 changed):**
- `concordia-render/tests/representation.rs` (9):
  - `representation_switches_between_full_instanced_and_hidden`: distances 19.99, 20, 49.99, 50, 30 and 60 against 20/50, an entity without one at 1,000 m, the cheap mesh's material and shadow flag, a second camera, and back again (no memory), and a 3D distance.
  - `instanced_and_hidden_characters_build_no_palette` (two-part characters at 10, 30 and 70 m: one palette, the cheap mesh unskinned, the part dropped, nothing when hidden).
  - `malformed_representations_are_descriptive_errors_and_draw_full`.
  - `representation_lod_follows_the_camera_without_affecting_world_hash` (inserting it leaves the hash unchanged, two cameras give two pictures, and a simulation processor reading it is refused).
  - `picking_follows_the_representation`.
  - GPU: `hidden_entities_are_not_drawn_and_cast_no_shadow` (byte-equal to the scene without it), `instanced_entities_draw_the_cheap_mesh_unskinned` (byte-equal to drawing the cube directly; 2 draw calls), `instanced_representation_draws_100k_in_one_pass` (100,489 spheres: draws = full + instanced counted independently, 0 culled, 2 draw calls, golden), `lod_view_mode_colors_simulation_lod_and_representation` (eight cells, each color checked, golden).
- `concordia-render/tests/templates.rs`: `templates_carry_a_representation`.
- Changed deliberately: three `skinning.rs` tests read palettes from `represented(..)` (they are built per frame now); `capture_screenshot_schema_is_reflect_generated`'s snapshot gains `lod`; the editor's view-mode test counts 15; `terra_templates_skill_..._a_valid_template` also compiles the skill's `Representation` example and parses its `lod` capture.

**Red run:** against stubs (`represented` a plain clone, no validation, no presentation flag in worlds, the `lod` mode falling into the feature views), all 10 failed for the right reason. `templates_carry_a_representation` first failed on my template text (maps instead of RON structs), then for the right reason. The two GPU goldens' structural checks all passed before their images existed; their first green failure was the missing golden.

**Mutation checks:** 13 mutations, each run against its tests with the restored file given a fresh mtime; all 13 caught:
- **The distance thresholds:** the hidden boundary exclusive, the instanced boundary exclusive.
- **Hidden skipping shadows:** a hidden entity drawn anyway, its shadow included.
- **Instanced skipping skinning:** the instanced draw keeping its skin, palettes built for every pose, other parts kept.
- **Validation:** an invalid one used by its numbers.
- **The hash staying untouched:** worlds not learning presentation types.
- **The renderer and picking:** the renderer ignoring representation, measuring from the origin instead of the camera, picking ignoring it.
- **The view mode:** instanced not flagged, the level off by one.

**Goldens:** two new, `render/representation_100k` (480×270) and `render/view_mode_lod` (400×100), reviewed by the author and blessed locally. Every existing render golden is unchanged. **Snapshots:** `capture.screenshot`'s schema (`view_mode` gains `lod`), reviewed and accepted. **Editor schemas:** `MethodCaptureScreenshotParams`, `MethodRenderStatsResult` and `MethodViewportOverlays{Params,Result}` gain `lod`; new `Representation`.

**Studio and budgets** (served definitions, 4 chars a token; `capture_screenshot`'s schema gained `lod`): level designer 35 tools ≈21.7k (21,694 → 21,757), tech artist 23 ≈20.1k (19,996 → 20,059), QA 22 ≈18.4k (18,384 → 18,447), animator 22 ≈16.8k (16,746 → 16,777), director 13 ≈12.5k, gameplay programmer 16 ≈12.4k, writer 11 ≈9.6k (unchanged); all under 50k. No role gains a tool.

**Perf** (100k cubes, 1080p, a shadow-casting sun, one draw call; `concordia-render`'s AGENTS.md):

| Stage | Before | After |
|---|---|---|
| Full extraction | 134 ms | 47 ms |
| Scene-cache update after every entity moved | 178 ms | 113 ms |
| Interpolation | 26 ms | 27 ms |
| The frame | 26 ms | 37 ms |

A capture at 100k now takes about 84 ms (160 before). A playing 100k crowd is still above a 60 Hz frame: re-extracting and interpolating every moved entity is M7's columnar work. **Open items answered:** 100k instanced characters need none of 2.4.2's palette follow-ups (they build no palette), and the 43 ms pose hash is unchanged by representation (it is presentation-only and never hashed).

**Checklist:** `just lint`, Tauri clippy, `CONCORDIA_REQUIRE_GPU=1` full suite (1,403 passed, 8 skipped; 14 doc tests), `just gate-tests` M1 (4) and M3S (13), `just deny` and the editor's, `just editor-schemas` (committed), `just editor-test` (118 unit, 22 end-to-end): all clean.

**Phase 2.4b is done** (2.4b.1 and 2.4b.2).

**Not in 2.4b.2:** hysteresis for representation (E2: only if flicker shows); a columnar, incremental extraction for crowds that move every tick (M7 phase 7.7, with tests and a target); spreading a far character's held pose over its interval in rendering.

### Phase 2.5 refinement (2026-11-22, against the code after 2.4b.2)
**Status: author decision N1 taken (2026-11-22); my other calls below (say if you disagree). 2.5a done (see its As built); 2.5b done (2.5b.1, 2.5b.2a, 2.5b.2b); 2.5c done (2.5c.1, 2.5c.2; see their As builts); 2.5d done (N6, N7; see its As built). Phase 2.5 is complete.**

Refined against the code as built. The old Phase 2.5 section, this refinement's *2.5 (added)* tests, M7's 7.7 and 2.4b's As built are the starting point. **This section wins wherever the old Phase 2.5 text disagrees.**

**What exists (checked in code):**
- **No navigation code:** no `recast-sys`, `concordia-nav` or `concordia-terra-crowd`, and no `third_party/recastnavigation`. `plans/README.md` already names `concordia-nav` (+ `recast-sys`) and `concordia-terra-crowd` for M2.
- **Physics** collides only primitives (`Box`, `Sphere`, `Capsule`, `Cylinder`; no mesh or heightfield colliders), and static bodies are `RigidBody { kind: Static }` plus a `Collider`. `PhysicsRuntime::raycast` and `physics.raycast` exist; input gives pointer rays (`pointer.rs`).
- **The `-sys` pattern** (`jolt-sys`, `ufbx-sys`): vendored sources under `third_party/` with `VENDOR.md`, a narrow shim built with `cc` or CMake, a counted allocator (`live_allocations`), a header hash test, and the safe wrapper in the consuming crate (`concordia-physics/src/jolt.rs`, `concordia-import-fbx/src/ffi.rs`).
- **Terra:** processors in five phases (`PrePhysics`, `Physics`, `PostPhysics`, `FrameEnd`, `Presentation`; the old `Sim` set became `PostPhysics`, 2.0 As built) with deterministic parallel waves, command buffers, signals, templates and traits, and simulation LOD (`ProcessorDecl::lod`, `Due`, ADR-0076).
- **DDC:** `concordia_core::ddc::{Ddc, DdcKey}`, content-addressed blobs.
- **Overlays:** screen-space triangles over the tonemapped image (selection, gizmos, skeletons). `capture.screenshot` takes `overlays` and `skeleton`; nothing lets another module add debug geometry.

**Author decision (2026-11-22):**
- **N1. Crowds are per-agent Terra processors.** The crowd's state (corridor, velocities, neighbours, target) lives in hashed components. Detour's crowd steps (path request, corridor update, neighbour gathering, steering, obstacle avoidance, integration, collision resolution) are rewritten in Rust as processors over Detour's navmesh queries, following DetourCrowd's zlib source. Agents run in parallel and LOD thins far ones. `dtCrowd` itself is not wrapped.

**The questions in the brief, settled (my calls; say if you disagree):**
- **Scheduling (one rule).** Navigation and crowd processors are ordinary Terra processors in Terra's deterministic parallel scheduler (2.0, ADR-0063). The old "single-threaded inside the `Sim` set" constraint is gone. `Sim` is `PostPhysics`. Determinism comes from the scheduler's rules: each processor writes only its own agents, reads neighbours' last-tick state, and stable entity order is used wherever results combine. It is tested with 1 and 8 threads. The old section's implementation note and the risk line are updated to say so.
- **The 10k test lives in M7.** 2.5c proves correctness at 1,000 agents (`crowd_agents_separate_and_do_not_overlap`) and determinism at 10,000 (`crowd_is_deterministic_across_runs_and_threads`, 1 and 8 threads: a hash test, not a perf target). It also records a `crowd_10k` bench in a `BASELINE.md` with no pass/fail line. `crowd_of_10k_agents_avoids_overlap`, a perf test on the reference machine (`tests/perf/reference.ron`, which arrives with M4), stays in M7 7.7 with its 100k target. M7's 7.7 note is updated.
- **The Godot mapping stays out.** D1 moved `NavigationAgent3D`/`NavigationRegion3D` to M5 (its plan lists it), so `navigation_agent3d_maps_to_nav_agent_component` leaves 2.5. The component fields keep the semantics that M5's mapping needs (agent radius and height, target and path desired distances). That's semantics only, from Godot's MIT docs.
- **Area marks come from engine components, not a game.** `NavArea` is a box volume (`Blocked`, or a travel `cost` ≥ 1). `NavAreaGrid` is a cell grid with three marks per cell: `.` keep what the geometry says, `#` blocked, `o` walkable again (overriding a volume). It is stored as text rows so agents can read and diff it. Grid marks apply after volumes. M5 maps the reference port's grid onto `NavAreaGrid`; no port names appear in tracked files.
- **What the navmesh is built from:** the static colliders inside a `NavMeshVolume`, the same geometry characters walk on. Primitives are tessellated with fixed tables, so there is no trigonometry at build time. Render meshes are not a source until physics gains mesh colliders. A level whose ground isn't collidable isn't walkable either, so the two stay consistent.
- **Simulation LOD for far agents (ADR-0076):** the crowd's per-agent processors opt in with `.lod()` and advance by `Due::dt`, so far agents step less often over a longer interval. Neighbours are read from their last stored state whatever their LOD. Path requests and replans run at full rate but are budgeted per tick in entity order. Signals (`NavArrived`, `NavPathFailed`) ignore LOD (the 2.4b.1 signal rule). Feature cuts by level (no avoidance when far) wait for M7.
- **Where things live:** `recast-sys` holds the vendored Recast and Detour, the shim, **and its safe wrapper**. That differs from `jolt-sys` and `ufbx-sys`, so that no `unsafe` exists outside `recast-sys`; `concordia-nav` and `concordia-terra-crowd` are `#![forbid(unsafe_code)]`. `concordia-nav` covers build, cache, queries, agents, `nav.*` and the debug layer. `concordia-terra-crowd` covers crowds and traffic as Terra processors over `concordia-nav`.
- **ADR-0077 (navigation)** records the sources, the marks, tiles and the cache, determinism, N1 and the unsafe boundary. `docs/design.md` doesn't change (Recast/Detour are already listed).

**Sub-phases (each its own branch off main, one commit, PR to main; each refined again at its start):**
| Sub-phase | Content | Effort |
|---|---|---|
| **2.5a Navmesh: recast-sys, build and queries** (`m2/2.5a-navmesh`) | vendored Recast/Detour v1.6.0 and the shim; `concordia-nav`'s components, gathering, tiled parallel build, DDC cache, islands, nearest point, path and raycast; `nav.build`/`nav.path` and MCP tools; a generic debug-layer hook and the `navmesh` layer, with a golden; ADR-0077; level-designer tools and skill | 1–1.5 weeks |
| **2.5b Agents and click-to-move** (split into 2.5b.1 and 2.5b.2 at its refinement) | `NavAgent` (Godot-like semantics: radius, height, target/path desired distance) following a path through `CharacterMotion` in `PrePhysics`; replanning; `NavArrived`/`NavPathFailed` signals; click-to-move (pointer ray → physics raycast → `nearest_point`); the carried-over guest-signal test (below) | 0.5–1 week |
| **2.5c Crowds as Terra processors (N1)** | `concordia-terra-crowd`: `CrowdAgent` components and a `Crowd` trait, a deterministic spatial grid, processors for paths (budgeted), corridor, neighbours, steering, avoidance (Detour's sampled velocity obstacles), integration on the navmesh surface and collision resolution (Jacobi, last-tick state); LOD opt-in; the `crowd` debug layer | 1.5 weeks |
| **2.5d Lane-based traffic** | a lane graph (lanes as polylines with speed limits and connections, junctions with priorities) as text; vehicles following lanes with a car-following model (the Intelligent Driver Model, Treiber, Hennecke and Helbing 2000); deterministic yielding at junctions; the `lanes` debug layer; rule-based only (AI-driven behavior stays M7/M4) | 1 week |

**2.5a in detail:**
- **Vendored:** `third_party/recastnavigation`, tag **v1.6.0** (commit `6dc1667f580357e8a2154c28b7867bea7e8ad3a7`), zlib (`License.txt`). Only `Recast/` and `Detour/` (`Include` and `Source`), unmodified, with the SHA-256s in `VENDOR.md`. DetourCrowd, DetourTileCache, DebugUtils and the demo aren't vendored: N1 rewrites the crowd, and the debug draw is ours.
- **Shim** (`shim/concordia_recast.{h,cpp}`, C ABI, C++17 via `cc`): counted allocations through `rcAllocSetCustom` and `dtAllocSetCustom`; `cr_build_tile` (one tile from triangles and marks to Detour tile data, or an error code); navmesh create, add tile, free and stats; poly walking (vertices, area, links) for islands and drawing; query create and free, nearest poly, find path plus straight path (partial flag) and raycast, with a filter of area costs. Every entry point validates its input and returns an error code instead of crashing, and nothing calls back into Rust. Bindings are hand-written and checked by a header blake3 and struct-size test, as in `ufbx-sys`.
- **Determinism:** each tile is built independently from the triangles that overlap it (plus the border) in entity order. Tiles build in parallel and are added in (y, x) order. The serialized navmesh (a header plus each tile's bytes) is byte-identical for any thread count and any entity insertion order.
- **Cache:** a `DdcKey` of a format tag, the Recast version, the canonical config and the gathered input (vertices, triangles, marks). A hit skips the build. Play worlds build (or hit the cache) at play start. The navmesh is a world resource, not hashed, because it is derived from hashed state.
- **API:** `nav.build` (`world`, optional `volume`) gives per volume the tiles, polygons, islands, bounds, whether it was cached, and the key. `nav.path` (`from`, `to`, optional `volume` and `world`) gives the waypoints, `partial` and `length`. MCP tools `nav_build` and `nav_path`. A missing or empty navmesh is a descriptive error with suggestions.
- **Debug layers:** `EngineModule` gains a default-empty hook through which a module names debug layers and supplies world-space lines and triangles. `Overlays.debug` and `capture.screenshot`'s `debug` name the layers to draw, and the renderer projects them like the other overlays. Nav provides `navmesh` (polygons tinted by island, blocked areas absent, edges outlined). It is extended in 2.5c (`crowd`) and 2.5d (`lanes`) without editing the renderer.
- **Studio:** the level designer gains `nav_build` and `nav_path` and a new skill, `concordia-navigation`. Budgets for every role are reported in As built.
- **Tests (first list):**
  - `recast-sys`: `recast_builds_navmesh_for_flat_plane`, `ffi_handles_are_released`, `shim_rejects_bad_input`, `header_hash_and_struct_sizes_match`.
  - `concordia-nav`: `navmesh_build_is_deterministic_byte_identical`, `static_colliders_are_the_navmesh_source`, `blocked_area_marks_carve_unwalkable_cells`, `grid_walkability_reachability_equivalence_fixture`, `area_costs_steer_paths`, `path_query_returns_straight_path_on_open_floor`, `path_avoids_blocked_cells`, `nearest_point_snaps_click_to_navmesh`, `unreachable_target_returns_partial_path_to_closest_point`, `raycast_stops_at_the_navmesh_edge`, `large_level_tiles_without_silent_islands`, `malformed_nav_settings_are_descriptive_errors`, `navmesh_is_cached_in_the_ddc`.
  - API, MCP and render: `nav_build_over_api_reports_tiles_polygons_and_islands`, `nav_path_over_api_returns_waypoints`, `nav_tools_have_generated_schemas_and_mcp_tools`, the GPU golden `navmesh_debug_layer_draws_a_fixture_level`; the level designer's scope test.
- **References:** the Recast/Detour documentation and source (zlib) and Mikko Mononen's talks and blog posts (implementation); Godot's navigation docs and source (MIT) for semantics only; Snook, "Simplified 3D Movement and Pathfinding Using Navigation Meshes" (*Game Programming Gems*). No Unreal or other non-open source.

**Carry-over items:**
- **Guest `AnimEvent` is still untested.** 2.5b raises the first new gameplay signals since 2.4.4, so it adds `guest_receives_anim_and_nav_signals` (a WASM guest sees `AnimEvent` and `NavArrived`), closing the item.
- **ADR-0069's chunk components:** nav and crowds aren't a natural first user. Crowd agents in one 1,024-row chunk are close in spawn order, not in space, so per-chunk data is a poor broad phase, and the neighbour grid is a resource rebuilt from positions. The natural first user is M7 7.7's columnar extraction ("did anything in this chunk move?" flags, rebuilt each tick). The item moves there.

### Phase 2.5a As built (2026-11-22, ADR-0077)
**Reconciled with the code as built:** the refinement held. Details it left open, and what the tests taught:
- **The safe wrapper is in `recast-sys`** (the brief's rule: no `unsafe` outside it), unlike `jolt-sys` and `ufbx-sys`. The shim is `cr_*`: build a tile, create a navmesh and add tiles, walk polygons and links, and query (nearest, find path, straight path, raycast). `cr_closest_point` was written, then removed: Detour's `findStraightPath` already clamps a partial corridor's end to its last polygon, and a mutation showed the extra clamp changed nothing.
- **Marking is the shim's own step:** marks apply in order with a temporary blocked id. Blocked becomes Recast's null area before erosion, so agents keep their radius from blocked cells. RecastDemo marks after erosion; Recast's `rcMarkBoxArea` skips null spans and can't revive ground.
- **Marks are half-open.** `rcMarkBoxArea` includes the cell an upper bound lands on, so a grid cell [2, 3) marked [2, 3.25). `push_mark` now insets upper bounds by 0.001 of a cell.
- **`NavMeshVolume.edge_max_error`** (cells; default Recast's 1.3) was added. At 1.3, or even 0.9, simplification cut corner cells diagonally. Below 0.7, outlines are exact to the cell, which fixtures and grid-shaped levels use.
- **Recast rules the hand-computed tests now state** (each one first caught as my mistake, then fixed deliberately in the tests' headers):
  - `rcFilterLedgeSpans` removes a floor's outer ring of cells (the cell next to a drop) before erosion.
  - A span is at least one cell tall, so a floor at y = 0 has its walkable surface at y = 0.2 (one `cell_height`).
  - A face exactly on a cell boundary belongs to one side only. A floor edge on a boundary is walkable to −9.5 on one side and +9.75 on the other, so fixture floors put their edges 0.1 m inside a cell.
  - Erosion is a chamfer distance transform (2 orthogonal, 3 diagonal), so the cell diagonal to a blocked corner stays walkable.
  - Detour's A* runs over portal midpoints, so a corridor across tile seams can bend at a seam (a bend at a tile corner showed it). Path-shape tests use one tile, and seams are covered by reachability.
- **Tiles copy their own vertices** (each gathered triangle owns three). Passing the level's whole vertex array made every tile re-validate it.
- **Debug layers are a generic `EngineModule` hook** (`debug_layers`, `debug_geometry`), with unique names checked by `Engine::new`. `capture.screenshot` gains `debug` and `Overlays` gains `debug`; the renderer projects layers under the selection and gizmos (premultiplied triangles, then 1.5-point lines).
- **`concordia_physics::static_solids`** reuses the step's own body rules (static, non-sensor, valid), so nav and physics agree on what is solid.
- **`[nav]`** follows the `[anim]` precedent: `concordia new` writes it, and `concordia studio init` adds it when missing for the level designer. Studio test projects that list their modules gained it (expected knock-on).

**Engine:**
- `third_party/recastnavigation` (v1.6.0, `6dc1667f`, zlib; `Recast/` and `Detour/` only, SHA-256s in `VENDOR.md`).
- `recast-sys`: the shim, hand-written bindings (header blake3 and struct-size checks), counted allocators, `build_tile`, `DtNavMesh`, `DtQuery`, `AreaCosts`, `RecastError`.
- `concordia-nav`: `NavMeshVolume`, `NavArea`, `NavAreaGrid`; `NavInput::gather` / `key`; `build_tiles` (rayon, per-tile buckets); `NavMeshData` (`CNAV` v1); `NavMesh` (islands by union-find over links, pooled queries); `NavCache`; `NavModule` with `nav.build` / `nav.path`, the `NavMeshes` play resource and the `navmesh` layer.
- `concordia-api`: `DebugGeometry`, `DebugLine`, `DebugTriangle`; `EngineModule::{debug_layers, debug_geometry}`; `Engine::{debug_layers, debug_geometry}`.
- `concordia-render`: `RenderRequest::debug`, `Overlays::debug`, `capture.screenshot` `debug`, layer validation (`InvalidParams` listing the layers).
- `concordia-physics`: `static_solids`, `StaticSolid`.
- `concordia-app` (`[nav]`, `NavSection`), `concordia-cli` (`new`, `studio init`).
- Studio: the level designer gains `nav_build` and `nav_path`, the `concordia-navigation` skill, and workflow and verification steps.
- Docs: ADR-0077; the AGENTS.md and README.md of `recast-sys` and `concordia-nav`; the AGENTS.md of `concordia-api`, `-render`, `-physics`, `-app`, `-cli` and `-studio`; `concordia-render`'s README; `docs/guides/agents-engine-api.md` (a Navigation section, the `nav.*` rows, `debug`); `plans/README.md`; `third_party/README.md`; this plan's refinement and M7's 7.7 note.

**Tests (35 new, 4 extended):**
- `recast-sys/tests/smoke.rs` (5):
  - `header_hash_and_struct_sizes_match`;
  - `recast_builds_navmesh_for_flat_plane` (one quad at ±4.25, y = 0.2, no links; the derivation is in the header);
  - `queries_work_on_the_plane` (snap, an off-mesh miss, a one-polygon corridor, a two-point path, a raycast at t = 3.25 / 9 with normal −x, a free ray);
  - `ffi_handles_are_released` (counts back to the start after navmeshes, queries, failed builds and refused tiles);
  - `shim_rejects_bad_input` (nine bad configs, empty input, an out-of-range index, NaN, empty bounds, four bad navmesh params, zero nodes, bad costs, a taken slot).
- `concordia-nav/tests/build.rs` (9):
  - `navmesh_build_is_deterministic_byte_identical` (1, 3 and 8 threads and reversed insertion, equal blake3; a 3 × 3 grid);
  - `static_colliders_are_the_navmesh_source` (dynamic, kinematic and sensor bodies don't carve; a box, a box turned 45°, a 6 m box turned 90°, a cylinder and an offset collider do);
  - `blocked_area_marks_carve_unwalkable_cells` (area 19.5² − (4.5² − 4 × 0.25²) = 360.25 exactly);
  - `grid_walkable_marks_reopen_a_blocked_volume` (2 islands, then 1, and a 10 m path);
  - `grid_walkability_reachability_equivalence_fixture` (a 24 × 16 maze with 3 parts: navmesh islands equal flood-fill components over every pair of open cells);
  - `large_level_tiles_without_silent_islands` (1 km, 15,625 pillars, 32 × 32 tiles, over 10,000 polygons, 1 island, a corner-to-corner path);
  - `malformed_nav_settings_are_descriptive_errors` (nine volume fields, six mark errors, 62 costs, not a volume);
  - `navmesh_data_roundtrips_bytes`; `navmesh_is_cached_in_the_ddc` (a hit, a changed key, a corrupt entry rebuilt and stored again).
- `concordia-nav/tests/query.rs` (7):
  - `path_query_returns_straight_path_on_open_floor` (√185);
  - `path_avoids_blocked_cells` (six exact waypoints around the eroded wall end and its kept corner cells; 2√50.0625 + 2√0.125 + 2);
  - `area_costs_steer_paths` (cost 1 straight; cost 10 round the strip corners, 2√58 + 2);
  - `nearest_point_snaps_click_to_navmesh` (on-mesh, 1.5 m up, inside a blocked cell to the kept corner (2, 3) at 0.566 m, beyond reach, `island_at`);
  - `unreachable_target_returns_partial_path_to_closest_point` (ends at (−1.25, 0.2, 0), 4.75 m);
  - `raycast_stops_at_the_navmesh_edge`; `queries_from_many_threads_agree` (8 threads × 50).
- `concordia-nav/tests/api.rs` (3): `nav_build_over_api_reports_tiles_polygons_and_islands`; `nav_path_over_api_returns_waypoints` (across a seam, then around a wall spawned without `nav.build`, then off the navmesh, then the play world and its `NavMeshes`); `navmesh_debug_layer_draws_every_polygon` (n edges and n − 2 triangles per polygon, lifted 2 cm).
- `concordia-api/tests/debug_layers.rs` (2), `concordia-app/tests/nav.rs` (3: `nav_section_adds_the_nav_module`, `capture_rejects_unknown_debug_layers`, the GPU golden `navmesh_debug_layer_draws_a_fixture_level`, which also checks the viewport's `debug` gives the same image), `concordia-mcp/tests/nav_tools.rs` (2), `concordia-studio` (`level_designer_checks_reachability_with_nav_tools_and_the_navigation_skill`, `navigation_skill_examples_are_real_params_and_components`), `concordia-cli` (`studio_init_adds_nav_when_missing_for_the_level_designer`).
- Extended: `the_default_roster_ships_the_planned_skills` (13 skills), `new_project_enables_modules_and_studio` and the `--minimal` case (`[nav]`), the role-budget project (`[nav]`).

**Red run:** written against stubs (a shim returning errors, `gather` returning `NoVolume`, empty builds and queries, methods returning `Internal`, no layers), every `recast-sys`, `concordia-nav`, debug-layer, app (`capture.screenshot` without `debug`), Studio and CLI test failed for the right reason. Not seen red:
- **`shim_rejects_bad_input`'s bad-config loop** passed vacuously against the stub, which rejected everything; the test went red at its first valid-input check.
- **The debug-layer hook's two tests** were written in the same step as the hook. I then stubbed the Engine side, saw both fail (no layers, no uniqueness error) and restored it with a fresh mtime.
- **`nav_tools.rs` (2)** were written after the methods existed: MCP serves module methods generically, so they guard the wiring.
- **`nav_section_adds_the_nav_module`** first failed on my fixture (`MeshRenderer` and `Camera` need `[assets]` and `[render]`), with `[nav]` already wired, so it guards the manifest.

**Changed deliberately (my mistakes, each explained in the tests' headers):**
- The plane's walkable edge is ±4.25 (the ledge filter), not ±4.5, and its surface is y = 0.2, not 0.
- Fixture floors moved off cell boundaries.
- The blocked square's area keeps its four corner cells.
- The maze has 3 parts, not 4.
- The snap test's click moved from (2.5, 2.6), a tie, to (2.4, 2.6).
- The API's invalid-volume case uses `half_extents` (reflect ranges already reject `cell_size: 0` at the boundary, which is better).
- The 90° box moved into the right test after my edit put it in `busy_level`.

**Mutation checks:** 20 mutations, each run against its tests with the restored file given a fresh mtime; all 20 caught after two fixes:
- **The shim:** blocked left walkable, erosion before marks, a query not freed.
- **Marks:** an inclusive upper bound, `o` blocking, costs ignored.
- **Sources:** rotation ignored, offset ignored, sensors carving, dynamic bodies carving.
- **Queries:** no islands from links, an inverted raycast hit, `island_at` without its sideways check.
- **Data and cache:** trailing bytes accepted, a hit reported as a build, the cache never read.
- **Validation:** the slope unchecked.
- **Render and API:** the layer not drawn, unknown layers accepted, the play world without navmeshes.

The two fixes: **rotation ignored** first survived (the 45° box's test points also lay inside its axis-aligned footprint), so a 6 m box turned 90° was added. **The partial-path clamp** survived because it was redundant, so it was removed (above). One corrupt-cache mutation didn't compile; it was replaced by the two cache mutations above.

**Perf** (release, M3 Pro; the 1 km fixture: 15,625 pillars, 0.5 m cells): gather 17 ms; build 1.37 s on one thread, 0.22 s on eight; 1,024 tiles, 89,195 polygons, one island, 14.1 MB; load 18 ms; a corner-to-corner path 4.4 ms (125 waypoints, 1,426.8 m). The `navmesh` layer re-gathers the level on each draw so it follows edits (fine at these sizes; a scene change counter would make it free, later).

**Studio and budgets** (served definitions, 4 chars a token; `capture_screenshot` gained `debug`): level designer 35 → 37 tools ≈21.8k → 23.1k (21,757 → 23,078), tech artist 23 ≈20.1k (20,059 → 20,106), QA 22 ≈18.5k (18,447 → 18,494), animator 22 ≈16.8k (16,777 → 16,824), director 13 ≈12.5k (12,453), gameplay programmer 16 ≈12.4k (12,363), writer 11 ≈9.6k (9,576), all unchanged in tools except the level designer; all under 50k.

**Carry-over:**
- Guest `AnimEvent` is still untested; 2.5b's `guest_receives_anim_and_nav_signals` closes it.
- ADR-0069's chunk components have no user yet; the first natural one is M7 7.7's columnar extraction (refinement above).

**Not in 2.5a:** agents, click-to-move and nav signals (2.5b); crowds (2.5c); traffic (2.5d); runtime navmesh rebuilds for moving obstacles; render meshes as sources (waits for mesh colliders); a scene change counter for the debug layer.

**Goldens and snapshots:** one new golden, `nav/navmesh_debug_layer` (400 × 300), reviewed by the author and blessed locally. Every existing golden is unchanged. Six snapshots changed, all reviewed and accepted by the author:
- `capture.screenshot`'s schema, in both the render and M1 gate snapshots (`debug`);
- the level designer's agent file and the CLAUDE.md team block, in both the Studio and M3S gate snapshots;
- the M3S agents-and-skills list (`concordia-navigation`).

The second M3S snapshot only appeared once the first was accepted, because the gate test stops at its first mismatch. **Editor schemas:** `MethodCaptureScreenshotParams` and `MethodViewportOverlays{Params,Result}` gain `debug`; committed.

**Checklist:** all of these pass:
- `just lint` and Tauri clippy;
- `CONCORDIA_REQUIRE_GPU=1` full suite with `--no-fail-fast` (1,437 tests: 1,437 pass after the review, 8 skipped; 14 doc tests);
- `just gate-tests M1` (4) and `M3S` (13);
- `just deny` and the editor's, both `Cargo.lock`s, fresh advisory DB (the editor's lock gains `concordia-nav` and `recast-sys`; its deny prints an existing "no crate matched" warning for an ignore entry);
- `just editor-schemas` (committed);
- `just editor-test` (118 unit, 22 end-to-end).

### Phase 2.5b refinement (2026-11-22, against the code after 2.5a)
**Status: author decisions N2 and N3 taken (2026-11-22); my other calls below (say if you disagree). 2.5b is split: 2.5b.1 agents (`m2/2.5b.1-agents`, done; see its As built), then 2.5b.2 click-to-move (refined and split into 2.5b.2a and 2.5b.2b; see the Phase 2.5b.2 refinement).**

Refined against `main` after #58: the 2.5 refinement (its 2.5b row and carry-over items), the 2.5a As built, ADR-0077, ADR-0076 and the old Phase 2.5 test list. **This section wins wherever they disagree.**

**What exists (checked in code):**
- `NavMesh` answers `nearest_point` (2 m sideways), `path` (snapped ends; a partial corridor ends at the closest reachable point) and `raycast`, from any thread. The play world's navmeshes are the `NavMeshes` resource (by volume id), built at play start. Processors can read it with `reads_resource`.
- `CharacterController` (capsule `height` 1.8, `radius` 0.3) is moved in `Physics` by `physics.step` from `CharacterMotion.velocity` (y ignored); feet are at `Transform.translation`, and physics owns the position once the character exists. The default `NavMeshVolume` is sized for that capsule (`agent_radius` 0.3, `agent_height` 1.8).
- Terra processors declare reads, writes, received signals and resources; `commands.signal` raises a signal that arrives next tick; `.lod()` processors see only due entities, and signal receivers see every signal (ADR-0076).
- Scripts resolve component and signal names against the whole registry, so a guest can read, write and receive engine types by name (wire-encoded from their shapes, ADR-0062).
- **Nothing in the simulation knows the screen.** `pointer_ray(cursor, viewport, inv_view_proj)` exists, but the camera's `Projection` lives in `concordia-render`, the window size in the app, no gameplay code reads `ActionStates` yet, and physics raycasts are reachable only through `physics.raycast` (the runtime is play state, not a resource).

**Author decisions (2026-11-22):**
- **N2. The pointer ray is an input value.** Input gains a world-space ray kind. The app (and `input.inject`) turns the pointer into a ray using the camera it draws with, and replays record the ray, so a replay never depends on window size or the renderer. An engine processor does ray → physics raycast → `nearest_point` → `NavAgent.target`. Because this touches input, replay, physics and the app, it is its own sub-phase, **2.5b.2**.
- **N3. NavAgent keeps its own radius and height (Godot semantics), and the body wins.** Physics moves the `CharacterController`'s capsule, so collision always follows the controller. When an agent's radius or height differs from its controller's (by more than 1 mm), or its radius is larger than its navmesh's `agent_radius`, the engine logs a warning once per agent and still moves it. 2.5c's avoidance uses the agent's radius.

**My calls on the brief's other questions (say if you disagree):**
- **`NavAgent`** (scene data, written by gameplay, never by the engine): `target: Option<Vec3>` (none is idle), `max_speed` (4 m/s), `radius` (0.3), `height` (1.8), `path_desired_distance` (0.5 m: a corner counts as reached), `target_desired_distance` (0.5 m: arrival), `path_max_distance` (2 m: farther than this from the path is off it), `volume: Option<EntityId>` (none: the first volume by id whose box holds the agent's feet). Every number must be finite and above 0. Semantics follow Godot's `NavigationAgent3D` docs (MIT); M5 maps onto it.
- **`NavAgentState`** (hashed, written only by nav's processors, the "corridor state" of N1's spirit): `status` (`Idle`, `Waiting`, `Moving`, `Arrived`, `Failed`), `planned_for` (the target its path is for), `waypoints`, `next` (the corner being walked to), `partial`, `volume`, `waiting_since` (the tick it started waiting for a plan). 2.5b.1 stores the string-pulled waypoints, not Detour's polygon corridor; the per-polygon corridor is 2.5c's crowd state.
- **How a path is followed:** two processors in `PrePhysics`, after `terra.lod`:
  - `nav.plan` reads `NavAgent`, `Transform`, `CharacterController` and the `NavMeshes` resource, and writes `NavAgentState`. It gives new agents a state and plans the agents that need one, up to the budget.
  - `nav.follow` reads `NavAgent` and `Transform`, writes `NavAgentState` and `CharacterMotion`, and raises the signals. It advances past corners, checks arrival, and sets the velocity toward the next corner at `max_speed`, slowed on the last segment so a tick never carries the agent past the final point.
  - Physics moves the body the same tick. Agents need a `CharacterController`; one without fails with `InvalidAgent`. An agent's position is its `Transform.translation`, the controller's own rule.
- **Arrival and heights:** distances are measured **sideways** (x and z), with a vertical tolerance of **half the agent's height**. The navmesh surface sits up to one `cell_height` above the floor the feet stand on (0.2 m on the default volume), well inside 0.9 m, and a floor 3 m up isn't mistaken for this one. Corners use `path_desired_distance`; the final point uses `target_desired_distance` (the snapped end, or the closest reachable point of a partial path). Once arrived, the velocity is zero.
- **Replanning** happens when a target is set and has no plan for it, when the target moves farther than `target_desired_distance` from `planned_for`, or when the agent is farther than `path_max_distance` sideways from its current segment (pushed off it). There's no timer: navmeshes don't change at runtime yet, so a timer would only re-find the same path. A failed target is retried only when the target moves.
  - **The budget:** `[nav] plans_per_tick` (default 32). The agents that need a plan are taken in `(waiting_since, id)` order, so the longest-waiting go first and nobody starves. The rest are `Waiting` and keep following their old path, if any. All of it uses hashed state only, so it is deterministic at any thread count.
- **Signals** (raised on the agent, delivered next tick, received whatever the agent's LOD; ADR-0069, ADR-0076):
  - `NavArrived { target, position, partial }`: the requested target, where the feet are, and whether it was only the closest reachable point.
  - `NavPathFailed { target, reason, detail }`, where `reason` is one of `NoNavmesh` (no volume holds the agent, or its navmesh didn't build), `StartOffNavmesh`, `TargetOffNavmesh` or `InvalidAgent`, and `detail` is a sentence. It fires once per target. An unreachable target isn't a failure: the agent walks to the closest point and arrives with `partial: true`.
- **Simulation LOD:** neither processor opts in. `nav.follow` costs a few vector operations per agent, and physics moves every character every tick anyway. A far agent that was skipped would hold its velocity and overshoot corners and its target by up to *r* − 1 ticks. Planning, the expensive part, is budgeted instead. Far agents behave exactly like near ones in 2.5b; LOD savings arrive with 2.5c's steering and avoidance, which opt in.
- **Size checks (N3)** run at an agent's first plan for each volume and warn once (`concordia::nav`).
- **Scripting:** the carried-over `guest_receives_anim_and_nav_signals` runs a WASM guest (a new fixture, `tests/fixtures/script/nav-guest`) in a project with `[assets]`, `[anim]`, `[physics]`, `[nav]` and `[scripts]`. The guest sets a `NavAgent`'s target by writing the component (no new import: guests write engine components by name), receives `AnimEvent` from a real clip event and `NavArrived` from the walk, and records both in a guest component the test reads. **Guests set targets exactly as Rust gameplay does.**
- **Debug layer:** `nav_paths` draws each moving agent's remaining path (feet → next corner → … → end), lifted 2 cm. It is tested by its geometry; the renderer's path is the `navmesh` golden's, so there's no new golden.
- **API:** no new methods. Gameplay and agents set `NavAgent` with `component.set` / `component.patch` (`world: "play"`, ADR-0075) and read `NavAgentState` with `component.get` or `query.run`.
- **Studio:** the gameplay programmer gains `nav_path` and the `concordia-navigation` skill, which gains an "Agents" section (`NavAgent`, `NavAgentState`, the signals, replanning, the budget). The level designer's tools don't change. No tool schema changes, so only the gameplay programmer's budget moves; every role's numbers are reported in As built.
- **ADR-0077** gains an update section (agents, N2, N3). 2.5b.2 records the input ray kind in an ADR of its own (or an update to ADR-0072, decided at its refinement).

**Sub-phases:**
| Sub-phase | Content |
|---|---|
| **2.5b.1 Agents** (`m2/2.5b.1-agents`) | `NavAgent`, `NavAgentState`, `nav.plan` (budgeted) and `nav.follow`; replanning; `NavArrived` / `NavPathFailed`; N3's warnings; `[nav] plans_per_tick`; the `nav_paths` layer; the guest test; Studio |
| **2.5b.2 Click-to-move** | Input's ray kind (`ActionKind::Ray`, a `PointerRay` binding and event, recorded by replays); the app's pointer-to-ray conversion from the drawing camera; `input.inject` rays; a read-only physics query resource for processors; `ClickToMove` (click and pointer action names, layers, reach) and `nav.click` before `nav.plan`; refined again at its start |

**2.5b.1 tests (first list).** Most agent tests use a rig with no physics: a test system in `Physics` moves each `Transform` by `CharacterMotion.velocity × dt`, so every tick is hand-computable. One test runs real Jolt characters.
- `concordia-nav/tests/agents.rs`:
  - `nav_agent_walks_a_straight_path_and_arrives_within_tolerance` (the exact arrival tick, position and signal tick);
  - `nav_agent_turns_corners_within_path_desired_distance` (2.5a's hand-derived wall path);
  - `nav_agent_replans_when_the_target_moves_past_its_tolerance`;
  - `nav_agent_replans_when_pushed_off_its_path`;
  - `unreachable_target_walks_to_the_closest_point_and_arrives_partial`;
  - `nav_path_failed_names_the_reason_once_per_target`;
  - `plans_are_budgeted_per_tick_longest_waiting_first`;
  - `far_agents_follow_at_full_rate_and_signals_ignore_lod`;
  - `malformed_nav_agents_are_descriptive_errors`;
  - `nav_paths_debug_layer_draws_remaining_paths`.
- `concordia-nav/tests/agents_physics.rs`:
  - `nav_agent_arrives_within_tolerance_using_character_controller` (the old 2.5 test; Jolt);
  - `nav_agents_are_deterministic_across_runs_and_threads` (16 agents chasing moving targets with physics: equal world hashes over runs and at 1 and 8 threads).
- `concordia-nav/tests/api.rs`: `nav_agent_moves_in_the_play_world_over_the_api` (set the target with `world: "play"`, step, read `NavAgentState`), and `nav_agent_size_mismatch_warns_once` (`log.tail`).
- `concordia-app/tests/nav.rs`: `nav_section_sets_plans_per_tick`, and `guest_receives_anim_and_nav_signals` (WASM).
- `concordia-studio`: `gameplay_programmer_checks_paths_with_nav_path_and_the_navigation_skill`, and the navigation skill's examples test extended to the agent components.

### Phase 2.5b.1 As built (2026-11-22, ADR-0077 update)
**Reconciled with the code as built:** the refinement held, with three changes:
- **Studio: a new skill, not the navigation skill.** `concordia-navigation` tells its reader to call `entity_spawn`, `component_set`, `nav_build` and `capture_screenshot`, which the gameplay programmer can't call. The roster guard `every_tool_a_role_skill_tells_it_to_use_is_in_scope` refused it. The gameplay programmer sets targets from code, so it preloads a new `concordia-nav-agents` skill instead. That skill uses only the role's tools (`nav_path`, `query_run`, `sim_step`) and shows the script mirror of `NavAgent`, which a test checks field by field against the real type. `concordia-navigation` gains one line naming it and the `nav_paths` layer.
- **Test locations:** `nav_paths_debug_layer_draws_remaining_paths` is in `tests/api.rs` (the layer needs an engine), and the guest test has its own file, `concordia-app/tests/nav_guest.rs`.
- **An entity's `NavAgent` owns its `CharacterMotion`:** `nav.follow` writes it every tick, zero when idle, arrived or failed. Gameplay that wants to steer a character itself removes the `NavAgent`. One writer per component keeps the scheduler's rule simple.

Details the refinement left open:
- **Volume choice:** `NavAgent.volume`, or the first volume by id whose box holds the feet (rotation ignored, as for building).
- **Failure order:** invalid settings, no controller (`InvalidAgent`), no volume or no built navmesh (`NoNavmesh`), the start then the target off the navmesh (`StartOffNavmesh`, `TargetOffNavmesh`). Detour's nearest-polygon search tests quantized polygon bounds, so a point just past the 2 m reach can still snap. The test puts its off-mesh agent 4.75 m out.
- **Corners:** the agent heads at its next corner and turns within `path_desired_distance` of it. Each stretch then stays within that distance of the path, a bound the corner test checks every tick.
- **New agents** get an idle `NavAgentState` at play start (`insert_agent_states`). Agents spawned during play get one through `nav.plan`'s commands and plan from the next tick.

**Engine:**
- `concordia-nav`:
  - `agent.rs`: `NavAgent` (`validate`), `NavAgentState`, `NavStatus`, `NavArrived`, `NavPathFailed`, `NavFailure`.
  - `plan.rs`: who needs a plan, one plan, applying it.
  - `follow.rs`: one tick of walking.
  - `agents.rs`: `nav.plan` and `nav.follow`, `AgentConfig`, `add_agent_processors`, `insert_agent_states`.
  - `NavMeshes::new`, `NavModule::with_plans_per_tick`, the `nav_paths` layer (`NAV_PATHS_LAYER`), and the four new types registered.
- `concordia-app`: `[nav] plans_per_tick` (a `NonZeroU32`, default 32).
- Studio: the gameplay programmer gains `nav_path` and `concordia-nav-agents` (14 skills).
- `tests/fixtures/script/nav-guest` (built by `just wasm`, linted by `just lint`).
- Docs: ADR-0077's update; the AGENTS.md of `concordia-nav`, `-app` and `-studio`; `concordia-nav`'s README; `docs/guides/agents-engine-api.md`; `plans/README.md`; this plan.

**Tests (18 new, 1 extended, 1 unit test):**
- `concordia-nav/tests/agents.rs` (9, on a rig without physics: a test system moves each `Transform` by its velocity):
  - `nav_agent_walks_a_straight_path_and_arrives_within_tolerance` (arrives on tick 195 at x = 4.75; with a 0.02 m arrival distance, slowed onto 5.23 and arrives on tick 205);
  - `nav_agent_turns_corners_within_path_desired_distance` (2.5a's wall; within 0.2 m of the path every tick, never in the wall, corners 1 to 5 in turn);
  - `nav_agent_replans_when_the_target_moves_past_its_tolerance` (0.4 m keeps the plan; 0.6 m and 1.5 m straight up replan; no target stops);
  - `nav_agent_replans_when_pushed_off_its_path` (1.5 m off keeps it, 2.5 m replans from there);
  - `unreachable_target_walks_to_the_closest_point_and_arrives_partial` (tick 86 at x = −1.73);
  - `nav_path_failed_names_the_reason_once_per_target` (all five reasons, once each, again for a new target);
  - `plans_are_budgeted_per_tick_longest_waiting_first` (2 a tick over 5 agents; 1 a tick with a replanning agent waiting behind earlier waiters);
  - `far_agents_follow_at_full_rate_and_signals_ignore_lod` (a level-3 agent arrives on the same tick; an LOD-aware receiver hears both);
  - `malformed_nav_agents_are_descriptive_errors`.
- `concordia-nav/tests/agents_physics.rs` (2, Jolt):
  - `nav_agent_arrives_within_tolerance_using_character_controller` (the old 2.5 test: Jolt's flat-ground step matches the rig, tick 195 at x = 4.75 within 1 mm);
  - `nav_agents_are_deterministic_across_runs_and_threads` (16 agents crossing past pillars, targets turned at tick 120; five hashes equal at 1 and 8 threads and run to run).
- `concordia-nav/tests/api.rs` (3 new; its engine rig moved to `tests/common/engine.rs` for both files):
  - `nav_agent_moves_in_the_play_world_over_the_api`;
  - `nav_agent_size_mismatch_warns_once`;
  - `nav_paths_debug_layer_draws_remaining_paths`.
- `concordia-nav` unit test: `off_path_measures_sideways_from_the_current_segment`.
- `concordia-app`: `nav_section_sets_plans_per_tick` (`tests/nav.rs`) and `guest_receives_anim_and_nav_signals` (`tests/nav_guest.rs`: the guest sets the target, hears `NavArrived` on tick 197 at x = 4.75, and hears Walk's `step@0.25`). This closes the carried-over `AnimEvent` item.
- `concordia-studio`:
  - `gameplay_programmer_checks_paths_with_nav_path_and_the_nav_agents_skill`;
  - `nav_agents_skill_examples_are_real_params_and_its_mirror_matches_nav_agent`;
  - extended: `the_default_roster_ships_the_planned_skills` (14).

**Red run:** written against stubs (no states inserted, `validate` accepting everything, processors doing nothing, no `nav_paths` layer, no `plans_per_tick`, no skill), every test above failed for the right reason. The exceptions:
- **Test mistakes I fixed deliberately:**
  - the budget test's closure captured the first rig's ids;
  - the first off-mesh point (2.15 m out) snapped (Detour's quantized bounds, above);
  - `component.patch` takes a merge patch, not a JSON Patch list;
  - a full `navg.Heard` value is required.
- **Passed immediately (written after the code they test):**
  - `agents_physics.rs` (2) and the guest test. Each was then shown red by mutation: "never plans" for the physics tests, and "arrival not signalled" for the guest test on its own.
  - The guest test's `AnimEvent` half passed immediately as expected: the item was "untested", not "broken".
  - The vertical-replan assertion, added during mutation planning (caught by "vertical tolerance ignored").

**Mutation checks:** 16 mutations, each run against its tests with the restored file given a fresh mtime. All 16 were caught:
- **Following:** no slow-down on the last stretch, arrival using the corner distance, a corner that never advances, the vertical tolerance ignored.
- **Replanning:** off-path never replanning, target moves never replanning, failed targets retried every tick.
- **Budget:** ordered by id instead of waiting time, and no budget at all.
- **LOD:** the follower opting in.
- **Signals and planning:** no arrival signal, never planning, no controller check.
- **Warnings:** a size warning on every plan.
- **Debug layer:** `nav_paths` drawn from the path's start instead of the feet.
- **Manifest:** `plans_per_tick` ignored.

**Studio and budgets** (served definitions, 4 chars a token; no tool schema changed):
- Gameplay programmer: 16 → 17 tools, ≈12.4k → ≈12.9k (12,363 → 12,946).
- Unchanged: level designer 37 tools ≈23.1k (23,078), tech artist 23 ≈20.1k (20,106), QA 22 ≈18.5k (18,494), animator 22 ≈16.8k (16,824), director 13 ≈12.5k (12,453), writer 11 ≈9.6k (9,576).
- All are under 50k.

**Carry-over:** the editor's deny warning and the `navmesh` layer's re-gather (a scene change counter) stand as before; ADR-0069's chunk components wait for M7 7.7.

**Not in 2.5b.1:** click-to-move (2.5b.2); crowds (2.5c); traffic (2.5d); a replan timer (waits for runtime navmesh changes).

**Goldens and snapshots:** no golden is new or changed (`nav_paths` is tested by its geometry; the renderer's path is the `navmesh` golden's). Two snapshots changed, both reviewed and accepted by the author: the gameplay programmer's agent file (Studio) and the M3S agents-and-skills list. They were collected with `INSTA_FORCE_PASS`, so the gate couldn't stop at its first mismatch and hide a second. **Editor schemas:** unchanged (`just editor-schemas` gives no diff; the schema project has no `[nav]`).

**Checklist:** all of these pass:
- `just lint` (now with `nav-guest`) and Tauri clippy;
- `CONCORDIA_REQUIRE_GPU=1` full suite with `--no-fail-fast` (1,456 tests: 1,456 pass, 8 skipped; 14 doc tests);
- `just gate-tests M1` (4) and `M3S` (13);
- `just deny` and the editor's, both `Cargo.lock`s, fresh advisory DB (the editor's lock is unchanged; its deny prints the existing "no crate matched" warning);
- `just editor-schemas` (no change);
- `just editor-test` (118 unit, 22 end-to-end).

### Phase 2.5b.2 refinement (2026-11-22, against the code after 2.5b.1)
**Status: author decision N4 taken (2026-11-22); my other calls below (say if you disagree). 2.5b.2 is split: 2.5b.2a click-to-move (`m2/2.5b.2a-click-to-move`, done; see its As built), then 2.5b.2b windowed play (done; see its As built).**

Refined against `main` after #59. **This section wins over the 2.5b refinement's 2.5b.2 row.**

**What exists (checked in code):**
- Input: `InputEvent` (`PointerMove` in logical pixels, buttons, keys, motion, scroll), `Binding::PointerPosition` (an `Axis2d`), `InputSampler::push` / `sample`, and `input.inject`. `ActionStates` is a play resource; replays (ADR-0072, `.replay.ron` version 0) record each tick's changed `ActionState`s, so a new value kind is recorded with no format change.
- **Nothing plays the game in a window.** `concordia run` opens the window and draws the *edit* world from the editor camera (`viewport.navigate` → `viewport.present`). Window input never reaches a play world: M1 1.1 deferred that wiring "to rendering (1.5)", and no plan picked it up. M2's gate (2.10) is headless, through `input.inject`.
- Cameras are `concordia-render`'s `Camera` (`Projection::matrix(aspect)`) on an entity; `capture.screenshot` picks a camera entity (default: the highest priority) or a `View`. `pointer_ray(cursor, viewport, inv_view_proj)` is in `concordia-input`.
- Physics raycasts exist only as `physics.raycast`. The runtime sits in the play state behind a mutex, so processors can't reach it.
- No Studio role has `input_inject`.

**Author decision (2026-11-22):**
- **N4. Windowed play is built too**, as its own sub-phase. N2 holds: the pointer becomes a world ray where the screen is known, and the simulation only ever sees rays. Two places know the screen: `viewport.pointer_ray` for agents and tests (2.5b.2a), and `concordia run`, which plays the scene in its window and turns cursor moves into rays from the camera it draws with (2.5b.2b). Both use one function in `concordia-render`.

**My calls (say if you disagree):**
- **The ray kind:** `ActionKind::Ray` and `ActionValue::Ray { origin, direction }` (direction normalized). `InputEvent::PointerRay { origin, direction }` must be finite with a non-zero direction (`input.inject` refuses it otherwise). `Binding::PointerRay` (only on a `Ray` action) holds the latest ray. A ray is "active" once one has arrived, so `pressed` is true from then on. Replays record ray changes like any action. `.replay.ron` stays version 0, and older recordings read unchanged.
- **The lens:** `concordia_render::pointer_ray_from_camera(world, camera, cursor, size)` uses the camera's world transform and `Projection::matrix(width / height)`, through `concordia_input::pointer_ray`. `viewport.pointer_ray {x, y, width, height, camera?, view?, world?}` → `{origin, direction}` takes the same camera choice as `capture.screenshot`. MCP: `viewport_pointer_ray`.
- **Physics queries for processors:** physics inserts a `PhysicsQueries` play resource (a handle to the same runtime; not hashed) with `raycast(ray, max, layers)`. `physics.raycast` and processors share one code path. A processor in `PrePhysics` sees last tick's bodies, which is deterministic.
- **`ClickToMove`** (on an agent; gameplay the engine ships, so it writes `NavAgent.target`): `click` (a `Button` action name), `pointer` (a `Ray` action name), `hold` (false: retarget on `just_pressed`; true: every tick while pressed, which the 2.5b.1 replan threshold keeps cheap), `layers` (empty: all) and `max_distance` (1,000 m).
  - **`nav.click`** (in `PrePhysics`, before `nav.plan`) reads `ActionStates`, `PhysicsQueries` and `NavMeshes`. It takes the ray, the nearest physics hit, then that point snapped onto the agent's navmesh (`nearest_point`, 2 m), and sets the target the same tick.
  - **A click on nothing** (no hit, or a hit with no navmesh near) changes nothing and raises nothing. UI feedback for it waits for 2.7, which also lets the HUD take a click first (`pointer_over_ui_consumes_click_before_world`).
  - Action names that are missing or of the wrong kind warn once at the first click check and the component is ignored.
- **ADR-0078 (pointer rays as input; click-to-move)** records N2, N4, the ray kind and its replay recording, the lens, `PhysicsQueries` and `ClickToMove`. ADR-0072 gets a one-line note that ray values are recorded.
- **Studio:** QA gains `input_inject` and `viewport_pointer_ray`, so a playtest can click like a player. Its prompt gets a click-to-move recipe (project the point, convert, inject the ray and the button, step). `input_inject`'s schema grows (the new event), so every role's numbers are reported.
- **Editor schemas** change (`InputEvent`, `ActionValue`, the new method) and are regenerated.

**Sub-phases:**
| Sub-phase | Content |
|---|---|
| **2.5b.2a Click-to-move** (`m2/2.5b.2a-click-to-move`) | The ray kind, event and binding; injection and replay of rays; the lens and `viewport.pointer_ray`; `PhysicsQueries`; `ClickToMove` and `nav.click`; ADR-0078; QA's tools |
| **2.5b.2b Windowed play** | `concordia run` plays the main scene: `sim.play` in real time, window input sampled into the play world (cursor moves also sent as rays from the drawing camera), the play world drawn from its highest-priority camera, `--edit` keeping today's editor view; refined again at its start |

**2.5b.2a tests (first list):**
- `concordia-input`: `pointer_ray_event_sets_a_ray_action`, `ray_bindings_fit_only_ray_actions`, `inject_rejects_bad_rays`.
- `concordia-replay`: `replays_record_and_play_back_pointer_rays`.
- `concordia-physics`: `processors_raycast_through_physics_queries` (the same hit as `physics.raycast`).
- `concordia-render`: `pointer_ray_from_camera_matches_hand_computed_rays` (perspective and orthographic, the image centre and a corner), and `viewport_pointer_ray_over_api` (camera choice and errors).
- `concordia-nav`, `tests/click.rs`:
  - `click_sets_the_target_to_the_snapped_hit` (exact);
  - `click_on_a_wall_snaps_to_the_nearest_walkable_point`;
  - `click_on_nothing_changes_nothing`;
  - `hold_retargets_while_pressed`;
  - `click_actions_of_the_wrong_kind_warn_once`.
- `concordia-app`: `player_click_to_move_reaches_marker` (a project with `[input]`, `[render]`, `[physics]` and `[nav]`: a marker is projected to a pixel, converted with `viewport.pointer_ray`, injected as a ray and a click, and the hero walks there; equal hashes at 1 and 8 threads).
- `concordia-studio`: `qa_clicks_like_a_player_with_input_inject_and_pointer_rays`.

### Phase 2.5b.2a As built (2026-11-22, ADR-0078)
**Reconciled with the code as built:** the refinement held, with one addition:
- **Every project starts with an input map.** Giving QA `input_inject` exposed that Studio refuses a role naming a tool its project lacks, and `input_inject` exists only with an `[input] map`. `concordia new` never wrote one. Following the `[nav]` precedent:
  - `concordia new` now writes `input/default.input.ron` (`InputMap::starter`: `move` on the left mouse button and `pointer_ray` on the pointer ray, which are `ClickToMove`'s defaults) and `[input] map`.
  - `concordia studio init` adds both when missing, inside an existing `[input]` table if there is one, and keeps a project's own map.
  - Knock-ons: `cli_replay` now repoints the map instead of appending a second `[input]`. The Studio projects in `role_scope`, `shared_engine` and the role-budget test gain the starter map. Studio's test catalogue gains `input_inject` and `viewport_pointer_ray`, so `concordia-record-replay`'s M3 note no longer uses the "Not available yet" marker.

Details the refinement left open:
- **Physics queries find nothing on tick 0.** They see bodies as of the last step, and syncing bodies at play start would change every existing play hash, so clicks count from tick 1.
- **The lens starts rays on the near plane**, through `concordia_input::pointer_ray`. `scene_camera` and `RenderWorld::camera` share `choose_camera` (the highest priority, ties to the lowest id). `viewport.pointer_ray` takes fractional pixels in 0..=width and needs no GPU.
- **`physics.raycast` keeps its own lock**, so a poisoned runtime is still an error rather than a silent miss. It calls the same `PhysicsRuntime::raycast` as `PhysicsQueries`.
- **Ordering:** `nav.click` runs after `input.sample` by registration order (input's module comes first), and before `nav.plan` by constraint.

**Engine:**
- `concordia-input`: `ActionKind::Ray`, `ActionValue::Ray`, `InputEvent::PointerRay`, `Binding::PointerRay`, ray checks and normalizing, and `InputMap::starter`.
- `concordia-render`: `camera_ray`, `scene_camera`, and `viewport.pointer_ray` (`ViewportPointerRay{,Params,Result}`).
- `concordia-physics`: `PhysicsQueries`.
- `concordia-nav`: `ClickToMove`, `nav.click` (`add_click_processor`, `CLICK`), and a dependency on `concordia-input`.
- `concordia-cli`: `concordia new` and `studio init` write the starter map.
- Studio: QA gains `input_inject` and `viewport_pointer_ray`, and a "play like a player" recipe.
- Docs:
  - ADR-0078, and a note in ADR-0072;
  - the AGENTS.md of `concordia-input`, `-render`, `-physics`, `-nav`, `-cli` and `-studio`;
  - the READMEs of `concordia-input` and `-nav`;
  - `docs/guides/agents-engine-api.md` and `plans/README.md`.

**Tests (15 new, 3 extended):**
- `concordia-input/tests/ray.rs` (3): `pointer_ray_event_sets_a_ray_action`, `ray_bindings_fit_only_ray_actions`, `inject_rejects_bad_rays`.
- `concordia-replay`: `replays_record_and_play_back_pointer_rays` (four recorded changes: the three new rays, plus the tick where `just_pressed` falls; replayed to the same hash).
- `concordia-physics`: `processors_raycast_through_physics_queries` (nothing on tick 0, then the ground 9.5 m down, the same hit as `physics.raycast`).
- `concordia-render/tests/pointer.rs` (2):
  - `pointer_ray_from_camera_matches_hand_computed_rays` (perspective: the centre and the corner (−0.685994, 0.514496, −0.514496); orthographic: the corner (−6.666667, 10, −5));
  - `viewport_pointer_ray_over_api` (the default and a named camera, a view, and errors at `/x`, `/y`, `/view`).
- `concordia-nav/tests/click.rs` (5):
  - `click_sets_the_target_to_the_snapped_hit` ((3, 0.2, −4), planned the same tick);
  - `click_on_a_wall_snaps_to_the_nearest_walkable_point` ((3.25, 0.2, 0) with exact outlines);
  - `click_on_nothing_changes_nothing`;
  - `hold_retargets_while_pressed`;
  - `click_actions_of_the_wrong_kind_warn_once`.
- `concordia-app`: `player_click_to_move_reaches_marker` (the marker's pixel comes back as the marker; arrives; equal hashes at 1 and 8 threads).
- `concordia-studio`: `qa_clicks_like_a_player_with_input_inject_and_pointer_rays`.
- `concordia-cli`: `studio_init_adds_an_input_map_when_missing_for_qa` (appended, inserted into an existing table, or a project's own map kept).
- Extended: `new_project_enables_modules_and_studio` (the starter map, matching `ClickToMove`'s defaults), `new_minimal_has_no_modules` (no `[input]`), and the role-budget project.

**Red run:** against stubs (the new types with no behavior, a `camera_ray` returning an error, an unregistered method, a `PhysicsQueries` that never hits, no `nav.click`, an empty starter map), every test above failed for the right reason. The exceptions:
- **Test mistakes I fixed deliberately:**
  - The replay count is four, not three (the tick where `just_pressed` falls is a change).
  - The "no camera" message says "Camera".
  - The wall test first used the default volume. Its 1.3-cell outline error cut the edge diagonally (2.5a's rule), so the test now uses `edge_max_error` 0.5 and a wall off cell boundaries.
- **Passed immediately:**
  - `click_on_nothing_changes_nothing` passed vacuously against the stub (the target stayed null). It became meaningful once clicks worked, and "a miss clears the target" catches it.
  - The replay and app chain tests were written after the code. They were shown red by "rays ignored by the sampler" and "pixel x and y swapped".
- **A bug the tests caught in my own code:** `studio init` first dropped a map inserted into an existing `[input]` table, because the file was written only when sections were appended. The extended test covers it.

**Mutation checks:** 15 mutations, each run against its tests with the restored file given a fresh mtime. All 15 were caught:
- **Input:** rays ignored by the sampler, directions not normalized, bad action rays accepted, the ray binding fitting any kind.
- **Physics:** no `PhysicsQueries` resource.
- **The lens:** the aspect ignored, the lowest-priority camera chosen, pixel x and y swapped.
- **Clicks:** firing while held without `hold`, hits not snapped, a miss clearing the target, `hold` ignored, warnings on every tick.
- **The starter map:** `pointer_ray` missing from it, and the map not added to an existing `[input]` table.

**Studio and budgets** (served definitions, 4 chars a token):
- QA: 22 → 24 tools, ≈18.5k → ≈21.5k (18,494 → 21,519).
- Unchanged, since no other role has `input_inject`: level designer 37 ≈23.1k (23,078), tech artist 23 ≈20.1k (20,106), animator 22 ≈16.8k (16,824), gameplay programmer 17 ≈12.9k (12,946), director 13 ≈12.5k (12,453), writer 11 ≈9.6k (9,576).
- All are under 50k.

**Not in 2.5b.2a:** windowed play (2.5b.2b); UI consuming clicks first and miss feedback (2.7).

**Goldens and snapshots:** no golden is new or changed. Three snapshots changed, all reviewed and accepted by the author: `input.inject`'s schema (`PointerRay`, `ActionValue::Ray`), QA's agent file, and the M3S agents-and-skills list. They were collected with `INSTA_FORCE_PASS`, so no gate could hide a second mismatch. **Editor schemas:** `MethodInputInjectParams` changes, and `MethodViewportPointerRay{Params,Result}` are new; committed.

**Checklist:** all of these pass:
- `just lint` and Tauri clippy;
- `CONCORDIA_REQUIRE_GPU=1` full suite with `--no-fail-fast` (1,471 tests: 1,471 pass, 8 skipped; 14 doc tests);
- `just gate-tests M1` (4) and `M3S` (13);
- `just deny` and the editor's, both `Cargo.lock`s, fresh advisory DB (the editor's lock gains `concordia-input` under `concordia-render` and `concordia-nav`; its deny prints the existing "no crate matched" warning);
- `just editor-schemas` (committed);
- `just editor-test` (118 unit, 22 end-to-end).

### Phase 2.5b.2b refinement (2026-11-22, against the code after 2.5b.2a)
**Status: done (see its As built); no new author decisions (N4 and the 2.5b.2 refinement set the shape). Branch `m2/2.5b.2b-windowed-play`. 2.5c (crowds) is next.**

**What exists (checked in code):**
- `concordia run` opens the scene and draws the *edit* world from the editor camera (`viewport.navigate` → `viewport.present`), only on open and resize. It ignores every input event. `--bench-latency` drives the editor camera.
- A playing simulation advances itself: `sim.play` sets it playing, and `Engine::poll` (the engine thread's housekeeping) steps it in real time.
- **The render module already redraws the play viewport after every tick** (`after_sim` → `queue_present`), but always from the editor's orbit camera. `viewport.present` takes only a `View`, and `RenderRequest` already supports a camera entity (or the highest-priority camera) when no view is given.
- `input.inject` queues device events for the next tick, and `viewport.pointer_ray` converts a pixel to a ray (2.5b.2a).
- winit sends no key-up when the window loses focus.

**My calls (say if you disagree):**
- **`concordia run` plays by default.** It opens the main scene, starts `sim.play` at the manifest's rate, and presents the play world from its game camera. `--edit` keeps today's editor view; `--bench-latency` implies it. Closing the window stops the simulation.
- **`viewport.present` can draw from a scene camera.** `view` becomes optional, and a new `camera` names one. With no `view`, it draws from `camera` or the highest-priority camera, the same choice as `capture.screenshot`; passing both is `InvalidParams`. The render state remembers the choice, so the after-tick redraw uses the same camera. One present at open and one per resize then keep a playing window drawn, with no frame timer.
- **Window input goes through the Engine API**, as `input.inject` events for the next tick (like any agent), so nothing new crosses the boundary:
  - Every cursor move is also sent as a `PointerRay`: the cursor in logical pixels, from `viewport.pointer_ray` over the window's logical size (physical size ÷ scale factor) on the play world's camera.
  - Without a camera, the ray is skipped, with one warning; keys and buttons still flow.
  - When focus is lost, every held key and button is released, so nothing sticks.
- **The logic is a library type, `concordia_app::Player`.** It turns `WindowEvent`s into Engine API calls through a call function, so tests drive it with a headless `Engine` and synthetic events. `concordia run` wraps it around its `EngineHandle`. Without `[render]` (or a GPU) it still plays; the window just shows nothing.
- **Docs:** ADR-0078's consequences gain the as-built note, `concordia-cli`'s and `concordia-app`'s AGENTS.md and the API guide (`viewport.present`) change, and editor schemas are regenerated (`ViewportPresentParams`).

**Tests (first list):**
- `concordia-render`: `viewport_present_takes_a_view_or_a_scene_camera` (both is `InvalidParams` at `/view`, an unknown camera is an error, and the camera choice is remembered for after-tick redraws: a pure `present_request` test).
- `concordia-app`, `tests/player.rs`:
  - `player_starts_play_and_presents_the_game_camera` (the calls made on open and resize);
  - `window_input_reaches_the_play_world`;
  - `cursor_moves_become_rays_from_the_drawing_camera` (the centre of a 1920 × 1080 window at scale 2 is the camera's forward ray, hand-computed);
  - `focus_loss_releases_held_input`;
  - `clicking_in_the_window_moves_the_hero` (the whole chain);
  - `edit_mode_keeps_the_editor_view`.
- `concordia-cli`: `run_plays_by_default_and_edit_keeps_the_editor` (argument parsing), and the existing `#[ignore]`d real-window smoke test, run locally.

### Phase 2.5b.2b As built (2026-11-22, ADR-0078 update)
**Reconciled with the code as built:** the refinement held. What the code taught:
- **Redraws follow real-time ticks only.** `after_sim` runs from `Engine::poll`, the path behind `sim.play`; `sim.step` never redraws. A playing window therefore needs no frame timer, and the GPU test drives `sim.play` with `poll`.
- **`Player` keeps no engine handle.** Each `event` takes the call function, so tests borrow a headless `Engine` and `concordia run` a blocking `EngineHandle` call.
- **Ordering in a window:** `concordia run` attaches the layer on `Opened`, then the player starts `sim.play` and presents. Without `[render]`, or without a surface, it plays undrawn. Input is injected for the next tick, so a key reaches the simulation at most one tick after it arrives.
- **An existing problem, not caused here:** the `#[ignore]`d `run_bench_latency_measures_the_engine_window_alone` fails on `main` too, checked by stashing this change. Its frames are queued but never presented on this machine. It is left as found, for a separate look.

**Engine:**
- `concordia-render`: `viewport.present` takes `view` *or* `camera` (`view` optional; both is `InvalidParams` at `/view`). The render state's `Source` (editor or scene camera) makes the after-tick redraw use the same viewpoint (`Look`, `look_for`).
- `concordia-app`: `Player` and `ApiCall`.
- `concordia-cli`: `concordia run` plays by default; `--edit`; `--bench-latency` implies the editor view.
- Docs: ADR-0078's update; the AGENTS.md of `concordia-app`, `-cli` and `-render`; the API guide's `viewport.present` row; `plans/README.md`. Editor schemas: `MethodViewportPresentParams`.

**Tests (9 new, 1 unit, 1 ignored):**
- `concordia-render/tests/view.rs`:
  - `viewport_present_takes_a_view_or_a_scene_camera` (no camera, both, an unknown camera, and three accepted forms reaching the surface check);
  - `viewport_present_from_a_scene_camera_redraws_after_each_tick` (GPU).
- `concordia-render` unit test: `redraws_keep_the_last_presented_source`.
- `concordia-app/tests/player.rs` (6, headless):
  - `player_starts_play_and_presents_the_game_camera` (the exact calls);
  - `window_input_reaches_the_play_world`;
  - `cursor_moves_become_rays_from_the_drawing_camera` (the centre of 1920 × 1080 at scale 2: (0, −0.759257, −0.650791) from (0, 13.924074, 11.934921));
  - `focus_loss_releases_held_input`;
  - `clicking_in_the_window_moves_the_hero` ((4, 0.2, −3));
  - `a_window_without_a_camera_still_plays`.
- `concordia-cli`: `run_plays_by_default_and_edit_keeps_the_editor` (`--help`), and the `#[ignore]`d `run_plays_the_main_scene_in_its_window` (a real window; run locally, it passed).

**Red run:** against stubs (a `Player` doing nothing, a required `view`, an undocumented `--edit`), every test above failed for the right reason, with two exceptions, both test mistakes fixed deliberately:
- the GPU redraw test first stepped with `sim.step`, which never redraws;
- the help test looked for "plays" where the help reads "play its main scene".

**Mutation checks:** 7 mutations, each run against its tests with the restored file given a fresh mtime. All 7 were caught:
- **The player:** no ray for cursor moves, the logical size ignoring the scale, focus loss releasing nothing, play never starting.
- **Rendering:** a view and a camera both accepted, redraws always using the editor view.
- **The CLI:** `concordia run` never playing (caught by the ignored real-window test, run locally).

A first form of the "both accepted" mutation didn't compile, so it was rewritten.

**Studio and budgets:** no role changes, and no MCP tool schema changes (`viewport.present` is editor-only), so every role's numbers are as in 2.5b.2a.

**Not in 2.5b.2b:** UI routing of clicks (2.7); a fixed-timestep input clock from OS event times (`tick_for_time`). Injection for the next tick is enough for play, and replays record the sampled states either way.

**Goldens and snapshots:** none new or changed (collected with `INSTA_FORCE_PASS`: no `.snap.new` appeared). **Editor schemas:** `MethodViewportPresentParams` (`view` optional, `camera`); committed.

**Checklist:** all of these pass:
- `just lint` and Tauri clippy;
- `CONCORDIA_REQUIRE_GPU=1` full suite with `--no-fail-fast` (1,481 tests: 1,481 pass, 9 skipped; 14 doc tests);
- `just gate-tests M1` (4) and `M3S` (13);
- `just deny` and the editor's, both `Cargo.lock`s, fresh advisory DB (the editor's lock gains `concordia-app`'s `serde_json` and `tracing`; its deny prints the existing "no crate matched" warning);
- `just editor-schemas` (committed);
- `just editor-test` (118 unit, 22 end-to-end).

### Phase 2.5c refinement (2026-11-22, against the code after 2.5b.2b)
**Status: author decision N5 taken (2026-11-22); my other calls below (say if you disagree). 2.5c is split: 2.5c.1 crowd core (`m2/2.5c.1-crowd-core`, done; see its As built), then 2.5c.2 avoidance and LOD (`m2/2.5c.2-avoidance-lod`, done; see its As built).**

Refined against `main` after #61: the Phase 2.5 refinement (its 2.5c row, N1, "Scheduling (one rule)", "The 10k test lives in M7" and the simulation-LOD bullet), the As builts of 2.5a, 2.5b.1, 2.5b.2a and 2.5b.2b, ADR-0077 (with its 2.5b.1 update), ADR-0078, ADR-0076, ADR-0069, ADR-0063 and M7's 7.7 note. **This section wins wherever they disagree.** The implementation follows DetourCrowd v1.6.0's zlib source (`DetourCrowd.cpp`, `DetourObstacleAvoidance.cpp`, `DetourPathCorridor.cpp`, `DetourLocalBoundary.cpp`, `DetourProximityGrid.cpp`, at the vendored commit `6dc1667f`). It is read, not vendored (N1).

**What exists (checked in code):**
- `NavAgent` (gameplay's request) and `NavAgentState` (hashed: status, `planned_for`, string-pulled `waypoints`, `next`, `partial`, `volume`, `waiting_since`). `nav.plan` plans every agent that needs a plan, within `[nav] plans_per_tick`, in `(waiting_since, id)` order. It fails an agent with no `CharacterController` (`InvalidAgent`). `nav.follow` writes `CharacterMotion` for every agent. Both run in `PrePhysics`.
- `nav.click` writes `NavAgent.target` and needs no controller. `NavArrived` and `NavPathFailed` are raised on the agent and delivered next tick, whatever its LOD.
- `NavMesh` gives `nearest_point`, `path` (corridor plus string pulling; the polygon corridor is thrown away) and `raycast`, from a pool of Detour query objects. The shim has `cr_find_nearest`, `cr_find_path`, `cr_straight_path` and `cr_raycast` (`t` and the normal only, with no visited polygons). There is no `moveAlongSurface`, `getPolyHeight`, `findLocalNeighbourhood` or `getPolyWallSegments`, and no raycast that returns its visited polygons.
- Terra views have `query` only, with no `With`/`Without` filter (`World::query_filtered` exists). Processors may read and write the same component in two passes: a snapshot pass, then a parallel pass.
- **Jolt characters don't collide with each other** (`CharacterVirtual` with no character-vs-character collision). Two `NavAgent` characters walk through each other today.
- Templates compose traits (`Movement`, `Lod`); spawners expand them at play start in id order.
- `concordia-terra-crowd` doesn't exist yet.

**Author decision (2026-11-22):**
- **N5. Two kinds of body, picked per entity.** A crowd agent without a `CharacterController` is a **surface agent**: it integrates on the navmesh surface without physics (DetourCrowd's way, so it scales to 10k), and the crowd writes its `Transform`. A crowd agent with a controller is a **physics agent**: the crowd's chosen velocity goes to `CharacterMotion`, and Jolt moves it. Surface agents treat every `CharacterController` (the player, physics agents, plain nav characters) as a solid obstacle: they steer round it and are pushed aside by it, but never push it. Physics doesn't see surface agents. Physics agents are kept apart from each other by the crowd, since Jolt characters don't collide with one another.

**My calls (say if you disagree):**
- **A crowd agent is a `NavAgent` plus a `CrowdAgent`.** One entity carries both. `NavAgent` stays the request (target, speed, sizes, distances; Godot semantics). `CrowdAgent` says "the crowd steers this agent" and holds the crowd's tuning, the way Godot's `NavigationAgent3D.avoidance_enabled` turns on avoidance for the same agent. A `CrowdAgent` without a `NavAgent` does nothing and warns once. `CrowdAgent` lives in **`concordia-nav`**, next to `NavAgent`: nav must know which agents it doesn't follow, and it can't depend on the crowd crate. The crowd's work (its state, processors, grid, trait and debug layer) lives in **`concordia-terra-crowd`**, which depends on nav.
  - **`CrowdAgent` fields** (DetourCrowd's `dtCrowdAgentParams`, with the defaults DetourCrowd's sample uses for a 0.3 m agent): `max_acceleration` (8 m/s²), `collision_query_range` (3.6 m, 12 radii), `path_optimization_range` (9 m, 30 radii), `anticipate_turns` (true), `optimize_visibility` (true), `separation` (false) and `separation_weight` (2), and `avoidance: Option<CrowdAvoidance>` (on by default; none turns it off). **`CrowdAvoidance`** holds DetourCrowd's sampling parameters as component fields, with `dtCrowd::init`'s defaults: `velocity_bias` 0.4, `weight_desired` 2, `weight_current` 0.75, `weight_side` 0.75, `weight_toi` 2.5, `horizon` 2.5 s, `divs` 7 (1..=32), `rings` 2 (1..=4), `depth` 5 (1..=8). Every number is checked like `NavAgent`'s. Neighbours are capped at DetourCrowd's 6, and corners at 4: these are constants, not fields.
- **Planning is shared: `nav.plan`, one queue, one budget.** Crowd agents queue with plain agents in `(waiting_since, id)` order under `plans_per_tick`, so nobody starves. For a crowd agent, `nav.plan` also keeps the **polygon corridor**, in a new `NavAgentState.corridor: Vec<u64>` (Detour polygon references from the agent's polygon to the end). The 2.5b.1 refinement named this as 2.5c's crowd state. The field is empty for plain agents, and references are stable because the navmesh is byte-identical. Crowd agents need no controller to be planned (N5). They skip the off-path replan, since the corridor follows them. Instead, the crowd asks for a replan by setting `Waiting` when it loses the corridor (the agent can't be moved along it). Without a target, the state resets but keeps the corridor's first polygon, as DetourCrowd's `corridor.reset` does, so idle agents can still be pushed and kept on the navmesh. **`nav.follow` skips crowd agents**, through a new `View::query_filtered` (`Without<CrowdAgent>`, Terra's existing filters). `nav_paths` draws plain agents only; the `crowd` layer draws crowd agents.
- **N1's "corridor, velocities, neighbours, target in hashed components", as built:**
  - the **target** is `NavAgent.target`, and its snapped end is the last of `NavAgentState.waypoints`;
  - the **corridor** is `NavAgentState.corridor`, starting at the agent's position (`Transform`);
  - **velocities and neighbours** are a new hashed **`CrowdState`** (in `concordia-terra-crowd`, written only by `crowd.*`): `velocity`, `desired_velocity`, `desired_speed`, `new_velocity`, `candidate` (the position before collision, within a tick), `corners` (at most 4), `neighbours` (at most 6, nearest first) and, in 2.5c.2, the local boundary (`boundary_center` and at most 8 `walls`).
- **The processor chain**, in `PrePhysics` after `nav.plan` (`nav.click` → `nav.plan` → `crowd.*`). Each step names what it reads and writes, then why it's deterministic. Every per-agent step uses `par_for_each`, `par_for_each_deferred` or the `_due` variants, in fixed 256-row pieces. An agent writes only its own components and reads others only from a snapshot.
  1. **`crowd.grid`** (full rate). Reads `Transform`, `NavAgent`, `CrowdAgent`, `CrowdState`, `CharacterController` and `CharacterMotion`. Writes the **`CrowdGrid`** resource: a snapshot of every crowd agent and every other `CharacterController` (id, feet, radius, height, velocity, desired velocity, and whether it is solid), bucketed into square cells as wide as the largest `collision_query_range`, so a query touches at most 3 × 3 cells. *Deterministic:* it is built in one pass, in id order, from hashed state. It is a resource rebuilt each tick, not hashed (derived data; ADR-0069's rule), and it holds every agent whatever its LOD. ADR-0069's chunk components aren't used: chunk rows follow spawn order, not space.
  2. **`crowd.corridor`** (`.lod()`, DetourCrowd's corner and visibility steps). Reads `NavMeshes`, `NavAgent`, `CrowdAgent` and `Transform`. Writes `NavAgentState` (corridor, status) and `CrowdState.corners`.
     - A physics agent's corridor follows its body first: `movePosition` to `Transform`, through `moveAlongSurface` and `dtMergeCorridorStartMoved`.
     - Corners come from `findCorners`: straight-path corners along the corridor, at most 4, dropping those within 0.01 m. With `optimize_visibility`, a surface raycast toward the second corner (stretched to `path_optimization_range`) that reaches it (t > 0.99, more than one polygon) shortcuts the corridor (`dtMergeCorridorStartShortcut`).
     - Topology optimization (the sliced local re-plan) is left out: it is an optimization queue that the rest doesn't depend on (later).
     - *Deterministic:* each agent's Detour calls read only the navmesh and its own state, from a pooled query object that each call resets.
  3. **`crowd.neighbours`** (full rate, DetourCrowd's `getNeighbours`). Reads `CrowdGrid`, `Transform`, `NavAgent` and `CrowdAgent`. Writes `CrowdState.neighbours`: every snapshot entry within `collision_query_range` sideways that overlaps vertically (|Δy| < (hₐ + h_b) / 2), sorted by **(distance², id)**, first 6. DetourCrowd caps its grid query at 32 items in hash order, which can miss the nearest; ours takes everything in range. *Deterministic:* the sort has no ties.
  4. **`crowd.steer`** (`.lod()`, DetourCrowd's steering and separation). Reads `CrowdState.corners`, `NavAgent`, `CrowdAgent` and `CrowdGrid`. Writes `desired_velocity` and `desired_speed`. The direction is smooth with `anticipate_turns` (`calcSmoothSteerDirection`) or straight. The speed is `max_speed`, scaled by min(distance to end, 2r) / 2r when the last corner is the end (DetourCrowd's slow-down). With `separation`, it adds separation from the neighbours' snapshot positions (weighted 1 − (d / range)², averaged, and clamped to the desired speed). Agents that are idle, arrived, failed or waiting with no corridor want zero. *Deterministic:* sums run in neighbour order.
  5. **`crowd.avoid`** (`.lod()`, 2.5c.2, DetourCrowd's sampled velocity obstacles: `sampleVelocityAdaptive`).
     - **Obstacles:** the neighbours become circles, with their snapshot position, radius, velocity and desired velocity. Wall segments come from the local boundary, refreshed when the agent has moved more than a quarter of its range (`findLocalNeighbourhood` + `getPolyWallSegments`); segments behind the agent are skipped.
     - **The search:** the sample pattern is aligned to the desired velocity. Each candidate's penalty adds the desired-velocity, current-velocity, side and time-of-impact terms, with the early-out. The search runs `depth` passes, halving the radius each time.
     - **Output:** `new_velocity`. Without `avoidance` (and in all of 2.5c.1), `new_velocity` = `desired_velocity`.
     - **No trigonometry:** DetourCrowd's `cosf`/`sinf` of 2π/divs and π/divs come from a fixed table for divs 1..=32, checked against `std` by a test, as 2.5a's tessellation tables are.
     - *Deterministic:* each agent reads only snapshots, and every candidate is tried in a fixed order.
  6. **`crowd.integrate`** (full rate, DetourCrowd's `integrate`). Writes `velocity` and `candidate`. It moves `velocity` toward `new_velocity` by at most `max_acceleration × dt`, then sets `candidate = feet + velocity × dt` (a velocity under 0.0001 m/s becomes zero). On the last stretch the step is clamped so a tick never passes the end. That is an addition to DetourCrowd, carried over from 2.5b.1's follower.
  7. **`crowd.collide`** (full rate, DetourCrowd's collision loop). It runs 4 **Jacobi** iterations. Each one snapshots every agent's `candidate`, then computes every displacement from that snapshot alone, in parallel. No agent sees a position that changed in the same iteration, so the order agents run in can't matter.
     - **Between agents:** DetourCrowd's rule. The penetration is p = rₐ + r_b − d, and each agent moves (p / 2) × 0.7 along the separating direction, averaged over its contacts. Coincident agents split perpendicular to their desired velocities, using the id order to choose the side, with p = 0.01.
     - **Against a solid** (a `CharacterController`), a surface agent takes the whole correction (p × 0.7).
     - **Physics agents** aren't displaced. Their correction from other physics agents becomes a velocity (displacement / dt) added to what they hand to `CharacterMotion`.
     - This collision step uses **this tick's** integrated positions, as DetourCrowd does. **Last tick's state** is what neighbours and avoidance read, through `CrowdGrid`.
  8. **`crowd.move`** (full rate, DetourCrowd's `movePosition`). It writes `Transform`, `CharacterMotion`, `NavAgentState` and `CrowdState`, and raises signals.
     - **Surface agents:** `moveAlongSurface` from the feet (on the corridor's first polygon) to the candidate (16 visited polygons at most), merged into the corridor, then `getPolyHeight` for y. The result is written to `Transform.translation`.
     - **Physics agents:** the velocity (plus the collision correction) is written to `CharacterMotion`, and physics moves them the same tick.
     - **Arrival:** when the last corner is the end and the feet are within `target_desired_distance` (sideways, half the height vertically: 2.5b.1's rule), the agent is `Arrived`, its velocities are zeroed and `NavArrived` is raised.
     - **Lost corridors:** a surface move that fails, or a corridor that has emptied, sets `Waiting` (a replan).
     - Arrived and idle agents still collide and are pushed (DetourCrowd does the same), and stay arrived.
     - *Deterministic:* signals go through deferred buffers joined in piece order.
- **Simulation LOD (ADR-0076; 2.5c.2):**
  - **What opts in with `.lod()`:** `crowd.corridor`, `crowd.steer` and `crowd.avoid`, the expensive per-agent queries and sampling. A far agent at level *i* chooses its velocity every *rᵢ* ticks and holds it in between.
  - **What stays at full rate:** `crowd.grid`, `crowd.neighbours`, `crowd.integrate`, `crowd.collide` and `crowd.move`. They are cheap, and they keep far agents from walking through each other, leaving the navmesh or overshooting their end.
  - **Neighbours** always read `CrowdGrid`, which holds every agent's last stored state whatever its LOD.
  - **Signals** are checked at full rate in `crowd.move`, so arrival never waits for a due tick.
  - Feature cuts by level (no avoidance when far) still wait for M7.
- **Signals:** crowd agents raise the same `NavArrived { target, position, partial }` (from `crowd.move`) and `NavPathFailed { target, reason, detail }` (from `nav.plan`, unchanged). **`ClickToMove` works on crowd agents unchanged**, because it writes `NavAgent.target` and needs no controller; a test proves it.
- **The `Crowd` trait** (ADR-0070, in `concordia-terra-crowd`): `Crowd { agent: NavAgent, crowd: CrowdAgent }`, both with defaults. It adds `Transform`, `NavAgent` (no target) and `CrowdAgent`, and requires `crowd.move`, so a template is refused without the crowd. Surface is the default body; a template adds a `CharacterController` component for physics agents. A template for 1,000 grunts, spawned by `Spawner { template: "grunt", count: 1000, seed: 7, distribution: … }`:
  ```ron
  Template(
      format: 1,
      description: "A grunt: walks with the crowd.",
      traits: {
          "Crowd": (agent: (max_speed: 3.5), crowd: (separation: true)),
          "Lod": (distances: (20.0, 60.0, 120.0), rates: (2, 4, 8), hysteresis: 2.0),
      },
      components: { "Name": "Grunt" },
  )
  ```
  Gameplay then sets each grunt's `NavAgent.target` (with `component.patch`, a script, or `ClickToMove`).
- **The crowd module:** `CrowdModule` ("crowd") registers `CrowdState` and the `Crowd` trait, gives crowd agents states at play start, adds the chain and names the `crowd` layer. **`concordia-app` enables it whenever `[nav]` is on**, so there is no new manifest section (crowds are part of navigation).
- **The Detour queries the chain needs**, each added to the shim narrowly, with a smoke test and the header-hash update:
  - 2.5c.1: `cr_move_along_surface` (`moveAlongSurface`, with visited polygons), `cr_poly_height` (`getPolyHeight`) and `cr_raycast_path` (`raycast` with its visited polygons, for the visibility shortcut).
  - 2.5c.2: `cr_local_neighbourhood` (`findLocalNeighbourhood`) and `cr_poly_wall_segments` (`getPolyWallSegments`).
  - **Rewritten in Rust instead, as pure list operations with unit tests from Detour's code:** the corridor helpers (`dtMergeCorridorStartMoved`, `dtMergeCorridorStartShortcut`, `findCorners`' pruning).
  - **Not needed:** sliced pathfinding (`nav.plan`'s `find_path`), `isValidPolyRef` (navmeshes don't change at runtime) and off-mesh connections (none are built).
  - The safe wrappers are `DtQuery` methods in `recast-sys`, exposed by `NavMesh` with raw `u64` references, so the crowd crate needs no `recast-sys`. `concordia-terra-crowd` is `#![forbid(unsafe_code)]`.
- **Debug layer `crowd`:** for each crowd agent, a 16-sided ring of its radius (2.5a's fixed table), its velocity and desired velocity (as 0.5 s of travel), its corners, and a link to each neighbour (drawn once per pair, from the lower id), lifted 2 cm. It is tested by its geometry, like `nav_paths`, with no golden: the renderer's path is the `navmesh` golden's. Avoidance samples aren't hashed state, so drawing them waits for a debug-agent hook like DetourCrowd's `dtCrowdAgentDebugInfo` (later).
- **Studio:**
  - **Gameplay programmer:** the `concordia-nav-agents` skill gains a "Crowds" section (`CrowdAgent`, `CrowdState`, surface and physics bodies, the same signals, the `crowd` layer), and its mirror test extends to `CrowdAgent`.
  - **Level designer:** `concordia-terra-templates` gains the `Crowd` trait template (checked by its examples test), and `concordia-navigation` names the `crowd` layer.
  - No tool schema changes, so the budgets shouldn't move; every role's numbers are reported in As built.
- **ADR-0077** gains a 2.5c update (N5, `CrowdAgent` in nav, the corridor in `NavAgentState`, the chain, `CrowdGrid`, LOD). No new ADR: N1 already decided the shape.

**Sub-phases:**
| Sub-phase | Content |
|---|---|
| **2.5c.1 Crowd core** (`m2/2.5c.1-crowd-core`, done) | the three shim queries; `CrowdAgent`; `NavAgentState.corridor`; `nav.plan` and `nav.follow` changes; `View::query_filtered`; `concordia-terra-crowd` (`CrowdState`, `CrowdGrid`, the corridor helpers, corridor, neighbours, steering and separation, integration, collision, move and arrival, both bodies, `CrowdModule`, the `Crowd` trait, the `crowd` layer); ADR-0077's update; Studio |
| **2.5c.2 Avoidance and LOD** (`m2/2.5c.2-avoidance-lod`, done) | the two boundary queries; the local boundary; sampled velocity-obstacle avoidance (fixed tables); the LOD opt-in; the `crowd_10k` bench in `BASELINE.md`; refined again at its start |

**2.5c.1 tests (first list).** The expected values are derived in the test headers, using 2.5a's Recast rules and 2.5b.1's rig tick rules (60 Hz, positions moved by velocity × dt). Surface agents need no rig, since the crowd moves them itself.
- `recast-sys` (`tests/smoke.rs`): `move_along_surface_slides_and_stops_at_edges`, `poly_height_reads_the_surface`, `raycast_path_lists_visited_polygons`, plus the new entry points in `shim_rejects_bad_input`, `ffi_handles_are_released` and `header_hash_and_struct_sizes_match`.
- `concordia-nav`:
  - `crowd_agents_are_planned_with_their_corridor_in_the_shared_budget`;
  - `nav_follow_leaves_crowd_agents_to_the_crowd`;
  - `malformed_crowd_agents_are_descriptive_errors`.
- `concordia-terra-crowd` (unit tests): `merge_start_moved_matches_detour`, `merge_start_shortcut_matches_detour`, `corners_drop_points_at_the_start`.
- `concordia-terra-crowd/tests/crowd.rs`:
  - `crowd_agent_walks_to_its_target_and_arrives` (the arrival tick and position, `NavArrived`);
  - `crowd_agent_slides_along_walls_and_stays_on_the_navmesh`;
  - `crowd_corridor_shortcuts_visible_corners`;
  - `crowd_agents_separate_and_do_not_overlap` (1,000 agents, hand-derived separation bounds);
  - `surface_agents_yield_to_characters_and_never_push_them`;
  - `physics_crowd_agents_keep_apart_and_arrive` (Jolt);
  - `crowd_signals_and_click_to_move_work_like_nav_agents` (`NavPathFailed`, `ClickToMove`);
  - `crowd_is_deterministic_across_runs_and_threads` (10,000 agents at 1 and 8 threads; a hash test, not a perf target);
  - `crowd_template_spawns_a_thousand_agents`;
  - `crowd_debug_layer_draws_agents_velocities_and_links`.
- `concordia-app`: `nav_section_adds_the_crowd_module`.
- `concordia-studio`: the nav-agents mirror test extended to `CrowdAgent`, and the templates skill's examples test covering the `Crowd` template.

**2.5c.2 tests (first list):** `cr_local_neighbourhood`/`cr_poly_wall_segments` smoke tests; `avoidance_pattern_tables_match_std`; `head_on_agents_pass_without_touching`; `avoidance_keeps_agents_off_walls`; `far_crowd_agents_steer_less_often_and_still_arrive`; `neighbours_read_last_tick_state_whatever_their_lod`; the 1,000-agent and 10,000-agent tests re-run with avoidance and LOD on; the `crowd_10k` bench (recorded in `crates/concordia-terra-crowd/benches/BASELINE.md`, with no pass/fail line). `crowd_of_10k_agents_avoids_overlap` stays in M7 7.7.

**References:** DetourCrowd's zlib source (above) and Mikko Mononen's blog posts on DetourCrowd and its sampled avoidance; van den Berg, Lin and Manocha, "Reciprocal Velocity Obstacles for Real-Time Multi-Agent Navigation" (ICRA 2008), and van den Berg, Guy, Lin and Manocha, "Reciprocal n-Body Collision Avoidance" (ISRR 2009); Godot's `NavigationAgent3D` avoidance docs (MIT), for semantics only. No Unreal or other non-open source.

**Carry-over items:**
- The `#[ignore]`d `run_bench_latency_measures_the_engine_window_alone` (frames queued but never presented on this machine): looked into at the end of 2.5c.1 if it is cheap.
- The `navmesh` layer's re-gather, the editor's deny warning, ADR-0069's chunk components (M7 7.7), and physics queries seeing last-step bodies all stand as before.

### Phase 2.5c.1 As built (2026-11-22, ADR-0077 update)
**Reconciled with the code as built:** the refinement held. What the code taught:
- **`nav.plan` places idle crowd agents** on the polygon they stand on (with its volume), outside the budget. So an agent without a target needn't keep its old first polygon: the refinement's "keep the corridor's first polygon" was written, then removed, because a mutation showed it changed nothing (the agent is placed again the same tick). This is 2.5a's redundant clamp again.
- **Terra gained two small general things:** `View::query_filtered` (`With` / `Without` over declared components, tags included), which `nav.follow` and `crowd.move` use, and five- and six-element queries.
- **`CrowdState.surface`** (where the agent stands on the navmesh, its corridor's start) was added to the refinement's fields. A physics agent's corridor follows its body from there, and a surface agent's `Transform` is set to it. `correction` holds a physics agent's collision push for the tick.
- **One `move_on_surface`** (`moveAlongSurface`, `dtMergeCorridorStartMoved`, `getPolyHeight`) serves both bodies: a physics agent's corridor following its body in `crowd.corridor`, and a surface agent's move in `crowd.move`.
- **Test names:** `crowd_signals_and_click_to_move_work_like_nav_agents` became two tests, `crowd_path_failures_are_signalled_once_like_nav_agents` (no physics) and `click_to_move_drives_a_crowd_agent` (an engine with input). `crowd_agent_never_steps_past_its_end` was added for the end clamp.
- **Build profile:** `concordia-terra-crowd` and `recast-sys` build at `opt-level = 2` in dev. The 10,000-agent test runs three 90-tick runs in about 14 s.

**Engine:**
- `recast-sys`: `cr_move_along_surface`, `cr_poly_height` (`CR_EMPTY` outside the polygon) and `cr_raycast_path`; `DtQuery::{move_along_surface, poly_height, raycast_path}` with `SurfaceMove` and `RayPath`; the header hash updated.
- `concordia-terra`: `View::query_filtered`, `QueryFilter::names`, and five- and six-element `QueryData`.
- `concordia-nav`:
  - `CrowdAgent` and `CrowdAvoidance` (`validate`);
  - `NavAgentState.corridor`, and `NavPath.corridor`;
  - `nav.plan` plans crowd agents (no controller needed, no off-path replan, idle placement), and `nav.follow` / `nav_paths` skip them;
  - `NavMesh::{nearest_poly, straight_path, move_along_surface, poly_height, raycast_path}` with `SurfaceMove` and `SurfaceRay`;
  - public `sideways`, `NavAgent::near` and the circle table (`CIRCLE_STEPS`, `circle_step`).
- `concordia-terra-crowd` (new):
  - `CrowdState`, `CrowdGrid` and `GridEntry`;
  - the corridor helpers (`corridor.rs`), steering (`steer.rs`), integration (`motion.rs`) and collision (`collide.rs`);
  - the seven steps (`steps/`), `add_crowd_processors` and `insert_crowd_states`;
  - `CrowdModule`, the `Crowd` trait and the `crowd` layer.
- `concordia-app`: `[nav]` adds `CrowdModule` after `NavModule`.
- Studio:
  - `concordia-nav-agents` gains "Crowds";
  - `concordia-terra-templates` gains "Crowds that walk the navmesh";
  - `concordia-navigation` names the `crowd` layer.
- Docs:
  - ADR-0077's 2.5c.1 update;
  - the AGENTS.md of `recast-sys`, `concordia-terra`, `-nav`, `-terra-crowd` (new), `-app` and `-studio`;
  - the READMEs of `recast-sys` and `concordia-terra-crowd` (new);
  - `docs/guides/agents-engine-api.md` (Crowds), `plans/README.md` (crates and the ADR registry), and this plan.

**Tests (25 new, 2 extended, 4 unit tests):**
- `recast-sys/tests/smoke.rs` (3 new, 2 extended): `move_along_surface_slides_and_stops_at_edges` (a 20 × 10 m plane in two tiles: across the seam to (8, 0.2, 0) through both polygons; stopped at x = 14.25; slid to (2, 0.2, 4.25)), `poly_height_reads_the_surface`, `raycast_path_lists_visited_polygons` (t = 14.25 / 20); `shim_rejects_bad_input` and `ffi_handles_are_released` cover the new entry points.
- `concordia-terra`: `view_queries_filter_with_and_without`.
- `concordia-nav` (3 new, 1 extended): `crowd_agents_are_planned_with_their_corridor_in_the_shared_budget` (five agents, two plans a tick by id across both kinds; the corridor's string pulling gives back the waypoints; idle placement), `nav_follow_leaves_crowd_agents_to_the_crowd`, `malformed_crowd_agents_are_descriptive_errors`; `nav_paths_debug_layer_draws_remaining_paths` leaves a crowd agent out.
- `concordia-terra-crowd/tests/crowd.rs` (8, on a rig without physics; the header derives 2/15 m/s of acceleration a tick and the 0.0081-per-tick collision contraction):
  - `crowd_agent_walks_to_its_target_and_arrives` (tick 205 at x = 4.762222);
  - `crowd_agent_never_steps_past_its_end` (2 m/s² would need 2.25 m to stop);
  - `crowd_agent_slides_along_walls_and_stays_on_the_navmesh`;
  - `crowd_corridor_shortcuts_visible_corners` (8 m tiles: the plan bends at (−4, −4), 0.2 m off the line; with the shortcut the walk stays within 0.005 m of it);
  - `crowd_agents_separate_and_do_not_overlap` (1,000 agents in 500 lanes; no overlap above 0.0081 × 0.1 / 0.9919 = 0.000817 m);
  - `surface_agents_yield_to_characters_and_never_push_them` (bounds 0.000408 and 0.000272 m; the character never moves);
  - `crowd_path_failures_are_signalled_once_like_nav_agents`;
  - `crowd_is_deterministic_across_runs_and_threads` (10,000 agents, 1 and 8 threads and run to run; exactly 60 × 64 = 3,840 moved over 1 m, from the plan budget).
- `concordia-terra-crowd/tests/engine.rs` (4): `physics_crowd_agents_keep_apart_and_arrive` (Jolt), `click_to_move_drives_a_crowd_agent`, `crowd_template_spawns_a_thousand_agents`, `crowd_debug_layer_draws_agents_velocities_and_links` (3 × 16 ring lines, 2 × 3 velocity and corner lines, 1 link).
- Unit tests: `merge_start_moved_matches_detour`, `merge_start_shortcut_matches_detour`, `corners_drop_points_at_the_start`, `neighbours_are_the_nearest_six_by_distance_then_id`.
- `concordia-app`: `nav_section_adds_the_crowd_module`.
- `concordia-studio` (1 new, 1 extended): `terra_templates_skill_crowd_example_is_a_valid_template`; `nav_agents_skill_examples_are_real_params_and_its_mirror_matches_nav_agent` checks the `CrowdAgent` and `CrowdAvoidance` mirrors too.

**Red run:** against stubs (shim methods returning `Query`, `query_filtered` ignoring its filter, `CrowdAgent::validate` accepting everything, crowd processors doing nothing, a trait adding nothing, no layer, no module in the app, skills without crowd sections), every test above failed for the right reason. The exceptions:
- **Test mistakes I fixed deliberately:**
  - the walk test compared the corner's y exactly (it is 0.20000005);
  - the first slide scenario had B pass on A's left and stop 0.5 m short, so A was never pinned (B now pushes from A's right with a 0.05 m arrival distance);
  - the 10,000-agent count first assumed most agents move, when the 64-a-tick plan budget allows exactly 3,840 (now derived and asserted);
  - the app test first lacked `[assets]` and `[render]` for the fixture's floor (2.5a's lesson) and spawned without an actor.
- **Passed immediately (written after the code they test), then shown red by mutation:** `crowd_corridor_shortcuts_visible_corners` ("no visibility shortcut"), `crowd_agent_never_steps_past_its_end` ("no end clamp", after it was strengthened; below), `physics_crowd_agents_keep_apart_and_arrive` ("physics agents never corrected"), `click_to_move_drives_a_crowd_agent` ("surface agents not moved"), `neighbours_are_the_nearest_six_by_distance_then_id` ("neighbours by id").
- **Passed vacuously at first:** the 10,000-agent hash test (the clock alone changes the hash); it now also counts moved agents.

**Mutation checks:** 26 mutations, each run against its tests with the restored file given a fresh mtime; the crowd's 15 were run again after its steps moved into `steps/`. 23 were caught at once. Of the 3 that survived:
- **"Idle crowd agents lose their polygon"** survived because the code was redundant (`nav.plan` places them again), so it was removed.
- **"No end clamp"** survived the first end-clamp test (at 8 m/s² the slow-down happened to stop short). It is caught at 2 m/s².
- **"Neighbours by id"** survived the 1,000-agent test (never more than 6 in range). It is caught by the new unit test.

The caught ones:
- **Shim:** visited polygons dropped, the height ignored, the ray's polygons empty.
- **Terra:** `Without` ignored.
- **Nav:** the corridor not kept, crowd agents needing a controller, crowd agents replanning off their path, idle agents never placed, `nav_paths` drawing crowd agents, validation skipped.
- **Crowd:** moves not merged into the corridor, no shortcut, solids pushed half, one collision iteration, physics agents never corrected, no acceleration limit, the candidate taken without the navmesh, characters left out of the grid, arrival not signalled, surface agents not moved, the trait adding no `CrowdAgent`, links drawn both ways.
- **App:** no crowd with `[nav]`.

**Studio and budgets** (served definitions, 4 chars a token; no tool or tool schema changed): level designer 37 tools ≈23.1k (23,078), QA 24 ≈21.5k (21,519), tech artist 23 ≈20.1k (20,106), animator 22 ≈16.8k (16,824), gameplay programmer 17 ≈12.9k (12,946), director 13 ≈12.5k (12,453), writer 11 ≈9.6k (9,576), all unchanged and under 50k.

**Carry-over:**
- `run_bench_latency_measures_the_engine_window_alone` was looked into, not fixed. Metal reports `presentedTime` 0 for a drawable that never reaches the screen, and the frame log then records no presentation time. That fits a window this machine doesn't show (occluded, another Space, or the display asleep); confirming it needs a desktop session. It is left noted.
- The `navmesh` layer's re-gather, the editor's deny warning, ADR-0069's chunk components (M7 7.7), and physics queries seeing last-step bodies all stand.

**Not in 2.5c.1:** sampled avoidance, the local wall boundary and the LOD opt-in, with the `crowd_10k` bench (2.5c.2); topology optimization (later); traffic (2.5d).

**Goldens and snapshots:** none new or changed (collected with `INSTA_FORCE_PASS=1 INSTA_UPDATE=new`: no `.snap.new` appeared). **Editor schemas:** unchanged (`just editor-schemas` gives no diff; the schema project has no `[nav]`).

**Checklist:** all of these pass:
- `just lint` and Tauri clippy;
- `just wasm`, `just native-fixtures`, then the `CONCORDIA_REQUIRE_GPU=1` full suite with `--no-fail-fast` (1,506 tests: 1,506 pass, 9 skipped; 14 doc tests);
- `just gate-tests M1` (4) and `M3S` (13);
- `just deny` and the editor's, both `Cargo.lock`s, fresh advisory DB (the editor's lock gains `concordia-terra-crowd` under `concordia-app`; its deny prints the existing "no crate matched" warning);
- `just editor-schemas` (no change);
- `just editor-test` (118 unit, 22 end-to-end).


### Phase 2.5c.2 refinement (2026-11-22, against the code after 2.5c.1)
**Status: done (see its As built); no new author decisions (N1 and N5 set the shape). One PR, `m2/2.5c.2-avoidance-lod`. 2.5d (traffic) is next.**

Refined against `main` after #62: the Phase 2.5c refinement (its `crowd.avoid` step, simulation-LOD bullet and 2.5c.2 test list), 2.5c.1's As built, ADR-0076 and ADR-0077 with its 2.5c.1 update. **This section wins wherever they disagree.** The implementation follows `DetourObstacleAvoidance.cpp` and `DetourLocalBoundary.cpp` (v1.6.0, zlib), read, not vendored.

**What exists (checked in code):**
- `CrowdAgent.avoidance: Option<CrowdAvoidance>` (on by default) is validated, but nothing reads it: `crowd.steer` sets `new_velocity` to the desired velocity.
- `crowd.corridor` and `crowd.steer` are already declared `.lod()`. Without a `LodViewer` and `LodProfile`s every agent is due every tick, and nothing tests the far case.
- `CrowdGrid` holds last tick's position, radius, height, velocity and desired velocity of every crowd agent and character, whatever its LOD.
- The shim has no `findLocalNeighbourhood` or `getPolyWallSegments`.
- No crate has a crowd bench; `concordia-anim` and `concordia-terra` record criterion benches in `benches/BASELINE.md`.

**My calls (say if you disagree):**
- **Two shim queries, narrow** (each with a smoke test, the bad-input and release tests, and the header hash):
  - `cr_local_neighbourhood(query, filter, start_ref, center, radius, refs, cap, count)`: `findLocalNeighbourhood`, without parents.
  - `cr_poly_wall_segments(query, filter, ref, segments, cap, count)`: `getPolyWallSegments` with no segment refs, so only walls (edges with no passable neighbour) come back, as xyz pairs.
  - `DtQuery::{local_neighbourhood, wall_segments}`, then `NavMesh::{local_neighbourhood, wall_segments}` with raw `u64` references, as in 2.5c.1.
- **The local boundary** (`dtLocalBoundary`) lives in `CrowdState`, which is hashed:
  - `boundary_center: Option<Vec3>` (none: never gathered), and `walls: Vec<Wall>` with `Wall { from, to }`, at most 8.
  - It is gathered in `crowd.avoid` for walking agents with avoidance, when there is no center yet or the agent has moved more than `collision_query_range / 4` from it (sideways).
  - Gathering: the polygons around the corridor's first polygon within `collision_query_range` (at most 16); each one's walls (at most 18); those farther than the range are skipped. The nearest 8 are kept by squared distance, inserted the way `addSegment` does (an equal distance goes before; past 8, the farthest drops).
  - DetourCrowd's `isValid` re-check is left out: navmeshes don't change at runtime (2.5c.1's reasoning for `isValidPolyRef`).
  - An agent that stops walking keeps its walls. They are drawn, not used.
- **`crowd.avoid`** (`.lod()`, after `crowd.steer`, before `crowd.integrate`). It reads `NavAgent`, `CrowdAgent`, `NavAgentState`, `NavMeshes` and `CrowdGrid`, and writes `CrowdState` (`new_velocity`, the boundary).
  - **Without `avoidance`,** or for an agent that wants no speed, `new_velocity` = `desired_velocity`. That is DetourCrowd's result with zero speed too, since only the zero sample fits a zero-speed disc. `crowd.steer` stops writing `new_velocity`.
  - **Obstacles:**
    - each neighbour, in neighbour order (nearest first, as 2.5c.1 sorts them), is a circle with its snapshot position, radius, velocity and desired velocity;
    - characters are circles too, with their `CharacterMotion` velocity and no desired velocity;
    - each wall that the agent isn't behind (`dtTriArea2D(pos, p, q) ≥ 0`) is a segment.
    - DetourCrowd reads neighbours' desired velocities from the same tick. Ours come from the snapshot, which is last tick's, so that agents never see each other's half-finished step.
  - **The search** (`sampleVelocityAdaptive` and `processSample`, rewritten in Rust as pure functions in `avoid.rs`):
    - The pattern has a sample at zero, then `rings` rings of `divs` samples aligned to the desired direction, each ring rotated by π/divs.
    - It runs `depth` passes. Each starts from `velocity_bias × desired`, uses a radius of `max_speed × (1 − velocity_bias)` that halves each pass, and skips candidates faster than the desired speed + 0.001.
    - The penalties for the desired velocity, the current velocity, the side and the time of impact use DetourCrowd's exact formulas, early-outs and `sweepCircleCircle` / `isectRaySeg`.
    - Ties keep the first candidate (strict `<`), so the order is fixed.
  - **One deliberate fix:** with an even `divs`, DetourCrowd writes a ring's last sample one slot too far, and samples the slot it skipped, which is stale memory. Ours puts the sample in that slot (DetourCrowd's evident intent). Odd `divs`, the default 7 included, are unaffected.
  - **No trigonometry at run time:** cos and sin of 2π/n and π/n for n in 1..=32 are a fixed table. `avoidance_pattern_tables_match_std` checks it against `std`, as 2.5a's circle table is.
  - *Deterministic:* every agent reads only `CrowdGrid` and its own state, and the samples are tried in a fixed order.
- **Physics agents avoid the same way:** their `new_velocity` goes through integration to `CharacterMotion`.
- **Simulation LOD, as the 2.5c refinement set it:**
  - `crowd.corridor`, `crowd.steer` and `crowd.avoid` are `.lod()`, so a far agent re-picks its corners, desired velocity, walls and avoidance velocity only when due, and holds them in between.
  - Grid, neighbours, integration, collision and movement stay full-rate.
  - None of the three needs `Due.dt`: each picks a velocity, it doesn't advance time.
  - **The held corners and desired velocity:** between due ticks the agent keeps steering at the old velocity, while `crowd.move` keeps it on the navmesh at full rate (moved along the surface, its corridor merged) and `crowd.integrate`'s end clamp stops it at its end. It can run past a corner by up to (*r* − 1) × speed / 60 m (0.35 m at rate 8 and 3 m/s), and slides along the wall there until it is due again.
  - **Arrival stays full-rate** in `crowd.move`.
  - Feature cuts by level (no avoidance when far) still wait for M7.
- **The `crowd` layer** also draws each agent's walls (in its own colour). Avoidance samples still wait for a debug hook (later).
- **The bench `crowd_10k`** (`benches/crowd.rs`, criterion): one tick of 10,000 surface agents walking a 100 × 100 lattice (2.5c.1's 10,000-agent field, planned before timing), measured four ways: avoidance off or on, and all near or with LOD (a viewer at one corner, the `Crowd` template example's profile). Each runs at 1 and 8 threads. The results go in `crates/concordia-terra-crowd/benches/BASELINE.md`, with no pass/fail line. `crowd_of_10k_agents_avoids_overlap` stays in M7 7.7.
- **Studio:**
  - `concordia-nav-agents` "Crowds" gains avoidance (what `CrowdAvoidance` does and when to turn it off), the walls and LOD (what far agents hold);
  - `concordia-navigation` names the walls in the `crowd` layer.
  - No tool schema changes; every role's numbers are reported in As built.
- **ADR-0077** gains a 2.5c.2 update.

**2.5c.2 tests.** The expected values are derived in the test headers.
- `recast-sys` (`tests/smoke.rs`, on 2.5c.1's two-tile plane: polygon `a` covers x −4.25..5 and `b` covers 5..14.25, z ±4.25, linked at the seam x = 5):
  - `local_neighbourhood_grows_with_its_radius`: from (0, 0.2, 0), radius 4.9 gives [a] (the seam is 5 m away), and radius 6 gives [a, b].
  - `wall_segments_are_the_unlinked_edges`: `a`'s walls total 9.25 + 8.5 + 9.25 = 27 m, and none lies on the seam.
  - The new entry points in `shim_rejects_bad_input` and `ffi_handles_are_released`.
- `concordia-terra-crowd` (unit tests):
  - `avoidance_pattern_tables_match_std`;
  - `free_space_keeps_the_desired_velocity` (cruising at the desired velocity d with |d| = max speed, the first ring's first sample is 0.4 d + 0.6 |d| d̂ = d, and nothing beats it);
  - `sampled_avoidance_matches_hand_computed_penalties` (divs 3, rings 1, depth 1: four samples, with every penalty worked out in the header);
  - `even_divs_fill_every_ring` (the fix above);
  - `local_boundary_keeps_the_nearest_eight`.
- `concordia-terra-crowd/tests/crowd.rs`:
  - `head_on_agents_pass_without_touching`: two agents head-on on one line. With avoidance they never come within 2r and both arrive. With avoidance off, the same pair touches (collision alone separates them, 2.5c.1).
  - `avoidance_keeps_agents_off_walls`: an agent walking along a wall meets one head-on. The side rule would send it into the wall, so the wall segments turn it the other way: its candidate never leaves the navmesh (no surface clip).
  - `far_crowd_agents_steer_less_often_and_still_arrive`: a viewer 200 m away puts an agent at level 3 (rate 8). Its desired velocity changes only on its due ticks, and it arrives within the derived corner-overrun bound of the full-detail run.
  - `neighbours_read_last_tick_state_whatever_their_lod`: a near agent meets a far one. Its neighbours and avoidance see the far agent's current position every tick, though the far agent steers only when due.
  - `crowd_agents_avoid_each_other_at_a_thousand_with_lod`: 2.5c.1's 1,000 agents with avoidance and LOD on. Collision's overlap bound still holds (avoidance keeps every speed within the 3 m/s disc), lanes never touch (checked in the test), and everyone arrives.
  - `crowd_is_deterministic_across_runs_and_threads` also runs with avoidance and LOD on: equal hashes at 1 and 8 threads and run to run, LOD visibly in effect (agents at every level), and a hash that differs from the full-detail run. The plain run keeps its exact 3,840 with `avoidance: None` written into its fixture. In 2.5c.1 avoidance changed nothing, so the default stood for "off".
  - `crowd_debug_layer_draws_agents_velocities_and_links` gains an agent walking beside an edge, whose walls are drawn.
- The `crowd_10k` bench, recorded in `BASELINE.md`.

**Carry-over items:** the ignored `run_bench_latency_measures_the_engine_window_alone`, the `navmesh` layer's re-gather, the editor's deny warning, ADR-0069's chunk components (M7 7.7), and physics queries seeing last-step bodies all stand as before.

### Phase 2.5c.2 As built (2026-11-22, ADR-0077 update)
**Reconciled with the code as built:** the refinement held. What the code taught:
- **The shared query pool was the bottleneck.** The first `crowd_10k` run took 27 ms a tick at 8 threads against 35 ms at one. Timers in each step showed `crowd.corridor` and `crowd.move` getting *slower* with threads, because every Detour call went through one mutex-guarded pool.
  - `NavMesh` now shards its pool 16 ways by thread. Results can't change, since every Detour call resets its object.
  - `crowd.collide`'s per-iteration hash map of candidates became a vector indexed by `CrowdGrid::slot`.
  - `CrowdGrid` shares its data behind an `Arc`, so each step's clone is cheap.
  - Together: 12.3 ms at 8 threads.
- **`facing`** (the walls an agent isn't behind) is its own function with a unit test: in every scene the integration tests use, all walls face the agent, so the filter can't be seen there.
- **Three tests turn avoidance off** (`avoidance: None`): the 2.5c.1 slide test (B now steers round A instead of pushing it), the shortcut test (the floor's walls bend the walk) and the plain 10,000-agent run, which keeps its exact 3,840. In 2.5c.1 avoidance did nothing, so the default stood for "off" there. These are deliberate fixes, not weakened tests: each tests something other than avoidance, and `avoidance.rs` covers avoidance against controls.
- **The 10,000-agent test became two:** plain (14.8 s) and with avoidance and LOD (20.8 s), so neither nears the 2-minute guard under suite load.

**Engine:**
- `recast-sys`: `cr_local_neighbourhood` and `cr_poly_wall_segments`; `DtQuery::{local_neighbourhood, wall_segments}`; the header hash updated.
- `concordia-nav`: `NavMesh::{local_neighbourhood, wall_segments}`; the query pool sharded by thread.
- `concordia-terra-crowd`:
  - `avoid.rs` (`Sampler`, `pattern`, `sample_velocity`, `sweepCircleCircle`, `isectRaySeg`, `dtDistancePtSegSqr2D`, `dtTriArea2D`);
  - `tables.rs` (the fixed cos/sin table);
  - `boundary.rs` (`keep_nearest`, `gather`);
  - `CrowdState.{boundary_center, walls}` with `Wall`;
  - the `crowd.avoid` step (`AVOID`, `.lod()`), with `crowd.steer` no longer writing `new_velocity`;
  - walls in the `crowd` layer;
  - `CrowdGrid::slot`;
  - `benches/crowd.rs` and `benches/BASELINE.md`.
- Studio:
  - `concordia-nav-agents` "Crowds" gains avoidance, far agents and walls;
  - `concordia-navigation` names the walls.
- Docs:
  - ADR-0077's 2.5c.2 update;
  - the AGENTS.md of `recast-sys`, `concordia-nav`, `concordia-terra-crowd` and `concordia-studio`;
  - the READMEs of `recast-sys` and `concordia-terra-crowd`;
  - `docs/guides/agents-engine-api.md`, `plans/README.md` (crates and the ADR registry), and this plan.

**Tests (17 new, 3 extended, 3 with avoidance turned off; 1 fixed outside the crowd):**
- `recast-sys/tests/smoke.rs` (2 new, 2 extended):
  - `local_neighbourhood_grows_with_its_radius` ([a] at 4.9 m, [a, b] at 6 m: the seam is 5 m away);
  - `wall_segments_are_the_unlinked_edges` (27 m of walls, none on the seam);
  - `shim_rejects_bad_input` and `ffi_handles_are_released` cover the new entry points.
- `concordia-terra-crowd` unit tests (7):
  - `avoidance_pattern_tables_match_std`;
  - `free_space_keeps_the_desired_velocity`;
  - `sampled_avoidance_matches_hand_computed_penalties`, every penalty worked out in the header: a circle 2 m ahead (c1, 5); 1.5 m ahead (stop); overlapping (8.702381); a second pass (it veers to (0.75, −0.433)); a wall ahead (2.777778, doubled time);
  - `even_divs_fill_every_ring`;
  - `segment_helpers_match_detour`;
  - `local_boundary_keeps_the_nearest_eight` (`addSegment` followed by hand over 11 inserts);
  - `walls_behind_the_agent_are_skipped`.
- `concordia-terra-crowd/tests/avoidance.rs` (6):
  - `head_on_agents_pass_without_touching` (never within 2r; the control with avoidance off touches);
  - `avoidance_keeps_agents_off_walls` (no surface clips for the agent the side rule sends toward the wall);
  - `walking_agents_gather_their_walls` (re-gathered on tick 29, at x = −4.087778; only the near edges of a 5.5 m square);
  - `far_crowd_agents_steer_less_often_and_still_arrive` (desired and chosen velocities change only on due ticks; arrival within the derived 49 ticks of full detail: 197 against 195);
  - `neighbours_read_last_tick_state_whatever_their_lod` (each lists the other on ticks 98..=163 exactly);
  - `crowd_agents_avoid_each_other_at_a_thousand_with_lod` (collision's bound for 2 × 3.001 m/s, no agent off its line by 0.55 m, levels 0–2 used, all arrive).
- `concordia-terra-crowd/tests/crowd.rs`: `crowd_with_avoidance_and_lod_is_deterministic_across_runs_and_threads` (new; every level used, hashes differ from the plain run, at most 3,840 moved). `crowd_is_deterministic_across_runs_and_threads`, the slide test and the shortcut test now turn avoidance off (above).
- `concordia-terra-crowd/tests/engine.rs`: `crowd_debug_layer_draws_walls` (new); `crowd_debug_layer_draws_agents_velocities_and_links` checks that its walkers have no walls.

**Red run:** against stubs (shim functions returning `CR_ERR_INPUT`, the table's first row zeroed, a sampler and pattern returning zero, a boundary keeping nothing, no `crowd.avoid`, no walls drawn), every test above failed for the right reason. The exceptions:
- **Passed immediately, because 2.5c.1 had already declared `crowd.corridor` and `crowd.steer` `.lod()`:**
  - `far_crowd_agents_steer_less_often_and_still_arrive`, `neighbours_read_last_tick_state_whatever_their_lod`, the 1,000-agent LOD test and the 10,000-agent full run.
  - The first two were then shown red by mutation (below). The last two are regression checks (bounds and hashes), and they pass with avoidance implemented too.
- **Test mistakes I fixed deliberately:**
  - the far test counted arrival's zeroing (full-rate, on tick 197) as a held-velocity change;
  - its header first counted 4 corners where the path has 3 turning points;
  - the wall test's first derivation re-gathered a tick early (the check reads the grid, last tick's position);
  - `segment_helpers_match_detour` first had `dtTriArea2D`'s sign backwards.
  - The last two were caught while deriving, before any run.
- **Written after the code:**
  - the far test's tracking of the chosen velocity;
  - the six strengthenings after the surviving mutations (below).

**Mutation checks:** 20 mutations, each run against its tests with the restored file given a fresh mtime. 14 were caught at once. The 6 that survived, and what now catches each:
- "overlaps not avoided more": the overlapping-circle case;
- "depth ignored": the second-pass case;
- "walls behind kept": `walls_behind_the_agent_are_skipped`;
- "no range cut": the 5.5 m square in `walking_agents_gather_their_walls`;
- "`crowd.avoid` every tick": a standing neighbour on the far test's path;
- "neighbours only when due": the far agent's own neighbour list, checked every tick.

All 20 are caught now. The caught ones:
- **Shim:** one neighbourhood polygon; walls dropped.
- **Sampler:** no early out, no side penalty, circles never hit, walls not doubled, rings not turned.
- **Step:** walls ignored, neighbours ignored, the boundary gathered every tick.
- **Boundary:** an equal distance goes after.
- **LOD:** steer every tick; the grid holding only due agents.
- **Debug:** walls not drawn.

**Studio and budgets** (served definitions, 4 chars a token; no tool or tool schema changed): level designer 37 tools ≈23.1k (23,078), QA 24 ≈21.5k (21,519), tech artist 23 ≈20.1k (20,106), animator 22 ≈16.8k (16,824), gameplay programmer 17 ≈12.9k (12,946), director 13 ≈12.5k (12,453), writer 11 ≈9.6k (9,576), all unchanged and under 50k.

**A flaky test fixed (2.5b.2b's `concordia-app/tests/player.rs`):** `window_input_reaches_the_play_world` failed once in the first full run and passed 203 times alone. The cause: `Player` starts play when the window opens, and a playing engine also advances by wall-clock time on every API call (`Engine::poll`). Under suite load, a gap of more than one tick between the key release and the check let an extra tick clear `just_released`. The tests now pause after opening (`Stage::open`), so only `sim.step` advances. A 50 ms sleep after each fed event makes the old version fail every time and the fixed one pass.

**Carry-over:** `run_bench_latency_measures_the_engine_window_alone` (left noted in 2.5c.1), the `navmesh` layer's re-gather, the editor's deny warning, ADR-0069's chunk components (M7 7.7), and physics queries seeing last-step bodies all stand. New for M7 7.7 (`BASELINE.md`): the crowd's sequential parts (`crowd.grid`'s build, the collision snapshots, `nav.plan`'s scan) are about 4 ms a tick at 10k.

**Not in 2.5c.2:** topology optimization (later); avoidance samples in the debug layer (they wait for a debug-agent hook); feature cuts by LOD level (M7); traffic (2.5d).

**Goldens and snapshots:** none new or changed (collected with `INSTA_FORCE_PASS=1 INSTA_UPDATE=new`: no `.snap.new` appeared). **Editor schemas:** unchanged (`just editor-schemas` gives no diff; the schema project has no `[nav]`).

**Checklist:** all of these pass:
- `just lint` and Tauri clippy;
- `just wasm`, `just native-fixtures`, then the `CONCORDIA_REQUIRE_GPU=1` full suite with `--no-fail-fast` (1,523 tests: 1,523 pass, 9 skipped; 14 doc tests);
- `just gate-tests M1` (4) and `M3S` (13);
- `just deny` and the editor's, both `Cargo.lock`s, fresh advisory DB (neither lock gains a crate: `criterion` is a dev-dependency already in the workspace lock; the editor's deny prints the existing "no crate matched" warning);
- `just editor-schemas` (no change);
- `just editor-test` (118 unit, 22 end-to-end).

### Phase 2.5d refinement (2026-11-22, against the code after 2.5c.2)
**Status: done (see its As built); author decisions N6 and N7 taken (2026-11-22). One PR, `m2/2.5d-traffic`.**

Refined against `main` after #63: the Phase 2.5 refinement (its 2.5d row and test list), the 2.5c refinement and As builts, ADR-0069, ADR-0070, ADR-0076, ADR-0077 and M7's 7.7 note. **This section wins wherever they disagree.**

**What exists (checked in code):**
- No lanes, vehicles or traffic anywhere.
- `concordia-terra-crowd` is listed in `plans/README.md` as "crowds and traffic". Its `CrowdGrid` snapshots every crowd agent and `CharacterController` each tick (position, radius, velocity), in id order.
- Physics has `BodyKind::Kinematic`: the body follows its `Transform` and pushes dynamic bodies, and Jolt characters collide with it.
- `concordia_scene::builtin::world_transform` gives world matrices, parents included.
- Templates compose traits; the `Crowd` trait is the model. The `[nav]` section enables `CrowdModule`.

**Author decisions (2026-11-22):**
- **N6. Lanes are scene entities.** Each lane is an entity with a `Lane` component, and each junction is an entity with a `Junction` component. Roads are therefore saved in `.scene.ron`, undoable, editable over MCP and in the editor, and spawnable from templates, with no new file format or tool.
- **N7. Cars brake for people.** A vehicle treats any crowd agent or character standing in its lane ahead as a stopped car and brakes for it. Crowd members don't yet steer round vehicles (M7). A vehicle template may add a kinematic body and collider, so physics characters, the player included, bump into cars.

**My calls (say if you disagree):**
- **Where it lives:** `concordia-terra-crowd`, in a `traffic` module beside the crowd. It reuses `CrowdGrid` for N7's people, which is why it isn't a new crate. Traffic needs no navmesh. `CrowdModule` adds traffic's processors, so `[nav]` turns traffic on too (navigation, crowds and traffic are one section), and there is still no new manifest section. A new **ADR-0079** records the lane graph and the junction rule; it is a new design, not a navmesh decision.
- **Components** (all reflected and validated like `NavAgent`; gameplay or the level designer writes the first three):
  - **`Lane`**: `points: Vec<Vec3>` (at least 2, in the entity's space: its world transform's translation and rotation apply, but not its scale, as in physics), `speed_limit` (m/s, default 13.9, i.e. 50 km/h), `width` (default 3.5 m), `next: Vec<EntityId>` (the lanes it leads to) and `priority: u8` (higher goes first at a junction; default 0).
  - **`Junction`**: `lanes: Vec<EntityId>`, the connector lanes inside it. Two connectors **conflict** when their polylines cross (sideways), or when they lead into the same lane (a merge). Connectors that only share a start don't conflict: their vehicles are queued on the same approach anyway.
  - **`Vehicle`**: the Intelligent Driver Model's parameters (Treiber, Hennecke and Helbing 2000, with the paper's urban values): `max_speed` (15 m/s), `max_acceleration` *a* (1.0 m/s²), `comfortable_braking` *b* (1.5 m/s²), `time_headway` *T* (1.5 s), `min_gap` *s₀* (2.0 m) and `length` (4.5 m). Also an optional `destination: Option<EntityId>` (a lane). The exponent δ is fixed at 4.
  - **`VehicleState`** (hashed, written only by `traffic.*`): `lane`, `distance` (m along it), `speed`, `route` (lanes still to drive), `status` (`Idle`, `Driving`, `Waiting` at a junction, `Arrived`, `Failed`), `waiting_since`, and `granted` (the junction that let it in).
  - **`JunctionState`** (hashed, written only by `traffic.junctions`): the vehicles granted, by id.
- **The lane graph** is a `TrafficGraph` resource, built at play start from every `Lane` and `Junction`. Built per lane: world-space points, cumulative lengths, and conflicts per junction. It is derived data and not hashed (ADR-0069's rule). Lanes are static during play in M2, as navmeshes are.
  - **Problems are descriptive and don't stop play:** a dangling `next` or `lanes` reference, fewer than 2 points, a non-finite point, a zero-length lane, and a connector in two junctions. Each is logged once by entity and drawn red in the `lanes` layer. A vehicle on a bad lane stops (`Failed`).
- **Routing:** with a `destination`, Dijkstra over the lane graph by travel time (length / speed limit), ties broken by lane id, is computed when the destination changes. Unreachable gives `Failed` and a `VehicleRouteFailed { destination }` signal, once per destination, like `NavPathFailed`. Without a destination a vehicle **wanders**. At each lane end it takes `next[h mod n]`, where *h* is SplitMix64 of (vehicle id, lane id): a fixed choice, with no RNG state. A lane with no `next` is a dead end, and the vehicle stops at its end.
- **The processor chain** (`PrePhysics`, after `crowd.grid`). Each step writes only its own entity and reads others from a snapshot, in fixed 256-row pieces, so the hash is the same at any thread count:
  1. **`traffic.place`**: puts vehicles without a lane on the nearest lane point to their feet (sideways, ties by lane id), and gives them states.
  2. **`traffic.snapshot`**: a `TrafficSnapshot` resource, holding every vehicle's lane, distance, speed and length, sorted per lane by distance, then id.
  3. **`traffic.junctions`** (one junction each). It reads the snapshot and its `JunctionState`, and keeps grants for vehicles still on its connectors. Requests come from vehicles whose next lane is one of its connectors and that are within their braking distance v²/(2b) + *s₀* of the stop line, or already stopped there. They are ordered by (priority descending, `waiting_since`, id), and each is granted greedily if its connector conflicts with no connector that is occupied or already granted.
  4. **`traffic.drive`** (per vehicle; full rate).
     - The leader is the nearest of:
       - the vehicle ahead on its lane and the lanes ahead on its route, within 150 m;
       - the stop line, as a stopped leader of length 0, when its next lane is an ungranted connector;
       - the end of a dead end or of its destination lane;
       - **N7's people**: `CrowdGrid` entries whose feet are within width / 2 + their radius of the lane ahead (sideways, and within half the vehicle's height vertically), as stopped leaders at their projected distance minus their radius.
     - **IDM:** *a*_IDM = *a* [1 − (v / v₀)⁴ − (s* / s)²], with s* = *s₀* + max(0, vT + vΔv / (2√(ab))) and v₀ = min(`max_speed`, the lane's `speed_limit`). With no leader the last term is zero.
     - **Treiber's ballistic update:** if v + a·dt < 0, it stops after −v² / (2a); else it moves v·dt + ½a·dt² and v += a·dt.
     - It moves along its polyline and on into its next lane (routed or wandered), then writes `Transform`: the translation on the lane and a heading from `Quat::from_rotation_arc`, so no trigonometry.
     - **Signals:** `VehicleArrived { destination }` when it stops at its destination lane's end, through deferred buffers joined in piece order.
- **Simulation LOD:** nothing opts in. `traffic.drive` is a few dozen operations per vehicle, and a held speed would carry far vehicles into the car ahead. That is the same reasoning as `nav.follow`'s (2.5b.1). Feature cuts by level stay in M7.
- **The `Vehicle` trait** (ADR-0070): `Vehicle { vehicle: Vehicle }`. It adds `Transform` and `Vehicle` and requires `traffic.drive`. A template adds a mesh, and optionally a kinematic `RigidBody` and `Collider`.
- **Debug layer `lanes`:**
  - each lane's polyline, with an arrow every 5 m showing its direction;
  - connectors coloured by state: free, granted or occupied;
  - each vehicle's footprint, a box of `length` by 1.8 m along its heading;
  - links from waiting vehicles to their stop line;
  - problems in red;
  - all lifted 2 cm. Tested by its geometry, like `crowd`.
- **Studio:**
  - **Level designer:** a new `concordia-traffic` skill covering lanes and junctions as entities, `next`, `priority`, conflicts, reading problems from the log and the `lanes` layer, and the `Vehicle` template. Its example calls and components are parsed as the real types.
  - **Gameplay programmer:** `concordia-nav-agents` gains a "Vehicles" section (`destination`, `VehicleState`, the two signals, a script mirror of `Vehicle` checked field by field).
  - No tool schema changes, so the budgets shouldn't move; every role's numbers are reported in As built.
- **Clean room:** Treiber, Hennecke and Helbing, "Congested traffic states in empirical observations and microscopic simulations" (Phys. Rev. E 62, 2000), and Treiber and Kesting, *Traffic Flow Dynamics* (2013), for the ballistic update. Dijkstra (1959). SplitMix64 (Steele, Lea and Flood, 2014), already in Terra. No Unreal or other non-open source; no traffic engine's code is read.

**2.5d tests (first list).** The expected values are derived in the test headers: 60 Hz, the first step is tick 0, and the IDM's ballistic update by hand.
- `concordia-nav`'s and Studio's existing tests, unchanged.
- `concordia-terra-crowd` unit tests:
  - `idm_acceleration_matches_hand_computed_values` (free road, a leader at equilibrium distance, a stopped leader);
  - `ballistic_update_stops_without_reversing`;
  - `routes_take_the_quickest_lanes` (Dijkstra by time, ties by id);
  - `junction_conflicts_are_crossings_and_merges`;
  - `junction_grants_follow_priority_then_arrival_then_id`;
  - `wander_choices_are_fixed_by_ids`.
- `concordia-terra-crowd/tests/traffic.rs` (a rig without physics):
  - `vehicle_accelerates_to_the_limit_along_its_lane` (the first ticks exact; the speed approaches min(limit, max); the heading follows a bend);
  - `vehicles_queue_without_overlap` (a platoon behind a stopped car: every bumper gap stays positive, and each settles within 0.1 m of *s₀*);
  - `traffic_follows_lanes_and_yields_at_junctions` (the milestone test: a four-way crossing, main road priority 1 and side road 0; conflicting connectors are never occupied at once; side vehicles enter only after the main ones pass; the order of entry is derived);
  - `vehicles_brake_for_people_in_their_lane` (a crowd agent and a character standing in the lane: the car stops ≥ *s₀* short of each; someone beside the lane, past width / 2 + r, isn't braked for);
  - `vehicles_reach_their_destination_or_fail_once` (`VehicleArrived`; an unreachable lane gives one `VehicleRouteFailed`);
  - `lane_problems_are_descriptive_and_stop_only_their_vehicles`;
  - `traffic_is_deterministic_across_runs_and_threads` (2,000 vehicles wandering a grid of roads, at 1 and 8 threads and run to run, and how many moved).
- `concordia-terra-crowd/tests/engine.rs`:
  - `vehicle_template_drives_a_kinematic_body` (the `Vehicle` trait; Jolt's kinematic body follows the lane; a character it meets is pushed, not passed through);
  - `lanes_debug_layer_draws_lanes_junctions_and_vehicles`.
- `concordia-studio`: the traffic skill's examples test, and the nav-agents mirror test extended to `Vehicle`.

**Carry-over items:** the ignored `run_bench_latency_measures_the_engine_window_alone`, the `navmesh` layer's re-gather, the editor's deny warning, ADR-0069's chunk components (M7 7.7), physics queries seeing last-step bodies, and the crowd's sequential steps at 10k (M7 7.7) all stand as before.

### Phase 2.5d As built (2026-11-22, ADR-0079)
**Reconciled with the code as built:** the refinement held (N6, N7). What the code taught:
- **The trait is `Traffic`** (`"Traffic": (vehicle: (…))`), because the component is already `Vehicle`. `VehicleState` has no `granted` field: grants live only in `JunctionState`.
- **Junction requests start at twice the IDM's desired gap**, 2 (s₀ + vT + v² / (2√(ab))): about 115 m at 10 m/s, 4 m at rest. The refinement's v² / (2b) + s₀ was shorter than the distance at which the IDM already brakes for a stopped line (about 58 m at 10 m/s), so vehicles would have slowed before they had even asked.
- **Look-ahead applies to every obstacle.** A dead end 450 m away first slowed a free-road vehicle to 9.963 m/s; the acceleration test caught it.
- **Arrival also counts the queue.** A vehicle on its destination lane has arrived when it stops at the end, or within s₀ + 0.5 m behind a vehicle that has arrived. Otherwise only the first of a platoon ever arrives. Stopping for a person or a slow car mid-lane isn't arrival.
- **People count within 2 m vertically** of the lane, since vehicles have no height field.
- **Problems are also components.** The level designer's scope has no log tool, so each problem entity gets `TrafficProblems { messages }` in the play world at play start, which `query_run` can read.
- **The heading is a yaw then a pitch.** A mutation showed no test had a slope. Vehicles now turn about y to the ground direction, then pitch about their own x to the slope, so they follow ramps without ever rolling. The special case for facing −z was redundant (glam already turns about +y) and was removed.
- **`TrafficGraph` and `TrafficSnapshot` share their data behind `Arc`s**, as `CrowdGrid` does, so each step's clone is cheap. Terra's `splitmix64` is now public, for wandering.
- **The rig** (`tests/common`) makes its navmesh optional and adds traffic's processors. `+z` is a vehicle's forward, as in glTF models.

**Engine:**
- `concordia-terra`: `splitmix64` exported.
- `concordia-terra-crowd` (`src/traffic/`):
  - `Lane`, `Junction`, `Vehicle`, `VehicleState` / `VehicleStatus`, `JunctionState` / `Grant`, `TrafficProblems`, `VehicleArrived` and `VehicleRouteFailed`;
  - `TrafficGraph` (`LaneInfo::{at, project}`, conflicts, approaches, problems);
  - the IDM and its ballistic update (`idm.rs`), Dijkstra and wandering (`route.rs`), and grants (`junction.rs`);
  - `TrafficSnapshot`; driving (`drive.rs`); `traffic.place`, `traffic.snapshot`, `traffic.junctions` and `traffic.drive`, with `insert_traffic` and `add_traffic_processors`;
  - the `Traffic` trait and the `lanes` layer (`LANES_LAYER`);
  - `CrowdModule` registers, starts and draws both crowds and traffic.
- Studio:
  - the new `concordia-traffic` skill, given to the level designer;
  - `concordia-nav-agents` gains "Vehicles".
- Docs:
  - ADR-0079 (new);
  - the AGENTS.md of `concordia-terra-crowd` and `concordia-studio`;
  - the README and `Cargo.toml` description of `concordia-terra-crowd`;
  - `docs/guides/agents-engine-api.md`, `plans/README.md` (crates and the ADR registry), and this plan.

**Tests (31 new, 1 extended):**
- `concordia-terra-crowd` unit tests (13):
  - `idm_acceleration_matches_hand_computed_values` (free road; a leader at 17 m and at the 18.977366 m equilibrium; a stopped leader, −2.912765; pulling away, with s* clamped: 0 and 0.947654);
  - `ballistic_update_stops_without_reversing`;
  - `lanes_are_placed_measured_and_projected` (a turned, moved L);
  - `junction_conflicts_are_crossings_and_merges`;
  - `lane_problems_name_the_entity_and_field`;
  - `routes_take_the_quickest_lanes` (10 s against 20 s; a tie goes to the lower id);
  - `wander_choices_are_fixed_by_ids`;
  - `junction_grants_follow_priority_then_arrival_then_id`;
  - `grants_hold_until_the_rear_leaves_the_connector`;
  - and the four from before the phase.
- `concordia-terra-crowd/tests/traffic.rs` (8):
  - `vehicle_accelerates_to_the_limit_along_its_lane` (x = 5 + 0.5 / 3600 and 5 + 2 / 3600; t(v) = 5 (atanh r + atan r) gives 10 m/s by 60 s);
  - `vehicle_heading_follows_a_bend_and_a_slope_without_rolling`;
  - `vehicles_queue_without_overlap` (five cars stop with positive gaps, each settling within 0.1 m of 2 m);
  - `traffic_follows_lanes_and_yields_at_junctions` (the milestone test):
    - at tick 0 the main road is granted and the side road waits;
    - footprints never overlap, and the two crossing connectors are never granted together;
    - the side road enters only after the first main car clears;
    - everyone arrives;
  - `vehicles_brake_for_people_in_their_lane` (a character and a crowd agent: stops ≥ 2 m short; someone 2.5 m off the line isn't braked for);
  - `vehicles_reach_their_destination_or_fail_once`;
  - `lane_problems_are_descriptive_and_stop_only_their_vehicles`;
  - `traffic_is_deterministic_across_runs_and_threads` (2,000 wandering vehicles in a 10 × 10 town, at 1 and 8 threads and run to run).
- `concordia-terra-crowd/tests/engine.rs` (2):
  - `vehicle_template_drives_a_kinematic_body` (a templated car's kinematic body shoves a crate in its lane);
  - `lanes_debug_layer_draws_lanes_junctions_and_vehicles` (1 + 18 + 4 + 2 lines).
- `concordia-studio` (2 new, 1 extended): `traffic_skill_examples_are_real_params_components_and_a_valid_template`, `nav_agents_skill_mirrors_vehicle`, and `the_default_roster_ships_the_planned_skills` (with `concordia-traffic`).

**Red run:** against stubs (the IDM, routes, grants and wander returning nothing, an empty graph, traffic processors doing nothing, no module wiring, no skill), every test above failed for the right reason. The exceptions:
- **`TrafficProblems` (added mid-phase) and the Studio mirror test** were red by not compiling, before their types and skill section existed.
- **Test mistakes I fixed deliberately:**
  - the acceleration header first used a time-constant argument; it now uses the exact free-road integral;
  - the destination test first parked the unreachable car on the same lane as the routed one, which correctly stopped behind it, so it now has its own lane;
  - the junction test's first "box" check tripped on a stopped car whose front sits under a centimetre past its line (the IDM's discrete approach to rest), so it now checks real footprint overlap;
  - the engine test first named a `Point` distribution that doesn't exist;
  - the skill's template example was first indented inside a list, where the examples parser doesn't look.
- **Passed immediately, written with the code:** the slope half of the heading test, then shown red by the two heading mutations.

**Mutation checks:** 22 mutations, each run against its tests with the restored file given a fresh mtime. 17 were caught at once. Of the 5 that survived:
- "s* may go negative" was missed because the pulling-away case had v = 0; a 5 m/s case now catches it.
- "waiting time ignored" was missed because no request was decided by waiting time; a two-request case now catches it.
- "grants dropped on entry" was missed because nothing tested the rear rule; `grants_hold_until_the_rear_leaves_the_connector` now catches it.
- "heading rolls at −z" exposed a redundant special case, which was removed.
- "heading not flattened" exposed that no lane was sloped; the ramp leg now catches it and the new "no pitch" mutation.

The caught ones:
- **IDM:** no free-road term; reversing.
- **Graph:** merges not conflicting; shared starts conflicting.
- **Grants:** conflicts ignored.
- **Routes and wandering:** the last tie winning; always taking the first lane.
- **Driving:** no look-ahead limit, stop lines ignored, queues never arriving, the heading not flattened, no pitch.
- **People:** never in the lane; everyone in the lane.
- **Steps:**
  - vehicles never placed, and requests from any distance;
  - the route failure unsignalled;
  - the heading not written;
  - problems not put on entities.

**Studio and budgets** (served definitions, 4 chars a token; no tool or tool schema changed): level designer 37 tools ≈23.1k (23,078), QA 24 ≈21.5k (21,519), tech artist 23 ≈20.1k (20,106), animator 22 ≈16.8k (16,824), gameplay programmer 17 ≈12.9k (12,946), director 13 ≈12.5k (12,453), writer 11 ≈9.6k (9,576), all unchanged and under 50k.

**Goldens and snapshots:** four changed, all from the level designer gaining `concordia-traffic` and "road traffic" in its description:
- Studio's `agent_level-designer` and `sync_generates_expected_claude_md_block`;
- the M3S gate's `m3s_agents_and_skills` and `m3s_claude_md_block`.

They were collected with `INSTA_FORCE_PASS=1 INSTA_UPDATE=new`, reviewed on an artifact page, and blessed by the author with `assertion_line` stripped. No render golden changed. **Editor schemas:** unchanged.

**Carry-over:** the ignored `run_bench_latency_measures_the_engine_window_alone`, the `navmesh` layer's re-gather, the editor's deny warning, ADR-0069's chunk components (M7 7.7), physics queries seeing last-step bodies, and the crowd's sequential steps at 10k (M7 7.7) all stand.

**Not in 2.5d:** lane changes, traffic lights, gap acceptance, people stepping aside for cars (M7), lanes edited during play, and traffic at M7's scale.

**Checklist:** all of these pass:
- `just lint` and Tauri clippy;
- `just wasm`, `just native-fixtures`, then the `CONCORDIA_REQUIRE_GPU=1` full suite with `--no-fail-fast` (1,544 tests: 1,544 pass, 9 skipped; 14 doc tests);
- `just gate-tests M1` (4) and `M3S` (13);
- `just deny` and the editor's, both `Cargo.lock`s, fresh advisory DB (neither lock gains a crate; the editor's prints the existing "no crate matched" warning);
- `just editor-schemas` (no change);
- `just editor-test` (118 unit, 22 end-to-end).

### Phase 2.7a refinement (2026-11-22, against the code after 2.5d)
**Status: done (see the 2.7a.1 and 2.7a.2 As builts); author decisions A1 and A2 taken (2026-11-22). Two sub-phases, two PRs: 2.7a.1 `m2/2.7a-mixer`, 2.7a.2 `m2/2.7a-engine`. The in-game UI half is phase 2.7b, refined at its own start.**

Refined against `main` after #64: the original "Phase 2.7: concordia-audio and concordia-ui" (below), the M2 refinement's phase table (its 2.7 row: "Godot import and sound bank out"), ADR-0063, ADR-0069, ADR-0073, ADR-0074 and ADR-0076, and M3's 3.7 (the synthesizer). **This section wins wherever they disagree.** In particular, from the original phase:
- **Out, to M5 (D1):** cues "from a Godot-side sound bank resource", `bus_layout_tres_maps_to_audio_bus_config`, the `.import` loop-flag mapping, and the Godot `Control`/HUD import (2.7b's half, moved anyway).
- **Replaced:** `sound_bank_cue_maps_pitch_and_volume_variation_with_seeded_rng` becomes `cue_variation_is_seeded_and_in_range` over an engine-native `.cue.ron`.
- **Kept and made exact:** the decoders, mixer, buses, voices, spatialization, both backends, the no-allocation test, streaming, bit-identical offline renders and the world-hash test, with hand-derived values.
- **Split:** 2.7 becomes 2.7a (audio) and 2.7b (UI, ADR-0060), because the two halves share no code.

**What exists (checked in code):**
- **No audio anywhere.** The only mention is `Phase::Presentation`'s doc comment ("representation, audio cues"). The test list and `crates/concordia-audio/` layout come from the original phase; `plans/README.md` lists the crate ("mixer, spatial audio, streaming").
- **The determinism boundary** (ADR-0063 point 4, ADR-0069, ADR-0076):
  - the world hash covers entity ids, non-presentation components, the clock, the RNG and **pending signals**; resources and chunk components aren't hashed;
  - presentation components are declared by `#[reflect(presentation)]` (`Representation` is the precedent), `register_presentation` or `writes_presentation`, and `hash_entity` skips them;
  - a `Presentation` processor may write only presentation components, may not record commands or write resources, and may receive signals; no other phase may touch a presentation component;
  - **no crate yet runs a `Presentation` processor** (only Terra's tests do). Audio is the first.
- **Processors own state:** `run` takes `FnMut(&mut View, &SystemCtx) + Send`, so a processor may hold a `Send` handle outside the world (the renderer's thread is the model, `concordia-render/src/thread.rs`).
- **Signals:** any reflected type; raised by `CommandBuffer::signal` (processors), `World::signal` (exclusive systems) or a script's `Command::Signal`; delivered at the start of the next tick in entity order. Scripts can raise Rust-defined signals already.
- **Clip events** (2.4.4): `AnimEvent { name, clip, node, time, weight }`, raised in `FrameEnd` by `anim.sample`, delivered next tick; hashed while pending.
- **Assets:** `Importer` (`name`, `version`, `extensions`, `default_settings`, `import`) registered through `AssetsModule::with_importer`; sidecars `<file>.import.ron`; artifacts by `AssetKind` (no audio kind yet) in the DDC under `import_key` (importer, version, platform, settings, source bytes). Text formats are read with `concordia_scene::read_document` and validated (`.animgraph.ron` is the model).
- **Modules:** `EngineModule` (`methods`, `start_play_in`, `after_sim`, `debug_layers` / `debug_geometry`); manifest sections switch modules on in `Project::modules`. There is no stop hook: `sim.stop` drops `PlaySetup.state`, so `Drop` is the stop hook. `after_sim` runs on the engine thread after each advance of at least one tick.
- **Windowed play:** `concordia run` spawns the engine thread and a window loop (`Viewer`, `Player`); the engine thread ticks itself.
- **Studio:** the sound designer (`assets/sfx/**`, ten tools) and composer (`assets/music/**`) are disabled in `team.toml` ("audio arrives in M2"). `concordia-synthesize-sfx` carries a "not available yet" note naming `gen_run` (M3 3.7). `role_tool_budget.rs` measures served tools against 50k for the enabled roles, using a `MODULES` manifest that must gain `[audio]`.
- **Dependencies** (probed in a scratch crate against both `deny.toml`s, 2026-11-22): `cpal` 0.18.2 (Apache-2.0; on macOS `coreaudio-rs` 0.14 over `objc2`, so no bindgen or libclang; on Linux `alsa-sys`), `lewton` 0.10.2 (MIT/Apache; `ogg` 0.8 BSD-3-Clause, `tinyvec`, `byteorder`), `hound` 3.5.1 (Apache-2.0), `rtrb` 0.4.1 (MIT/Apache). All pass `licenses`, `bans`, `sources` and `advisories` for the engine and the editor. `assert_no_alloc` is BSD-1-Clause, which isn't allowed, so it is out. `symphonia` (MPL-2.0) stays out.

**Author decisions (2026-11-22):**
- **A1. Both audio roles are enabled now.** The sound designer imports and wires existing sounds: cues, buses, emitters and clip-event sounds. The composer imports finished music, makes streamed, looping music cues and owns the Music bus. Writing new sounds and scores waits for M3 3.7's synthesizer (`gen_run`).
- **A2. The allocation counter is a test-only crate**: `allocation-counter` 0.8 (MIT/Apache-2.0) as a dev-dependency, so no new `unsafe` in our code. It is checked against both `deny.toml`s before use.

**My calls (say if you disagree):**
- **Crates and ADR.** One new crate, **`concordia-audio`**, with `#![forbid(unsafe_code)]`. Its library half (decode, the mixer, buses, voices, spatial math, streaming, cues, backends) depends on `concordia-core` (DDC), `concordia-reflect` and `concordia-assets` (the importers). Its engine half is the `audio` module (`[audio]`): components, the presentation processor and the methods. `concordia-app` adds it. That is one crate, like `concordia-anim`. **ADR-0066 is written now** under its reserved title, widened to the whole design: the stack, the determinism boundary, cues and the threading rule. M2's block (0060–0079) has no free number left, and nothing here needs a second ADR.
- **The determinism boundary: audio lives outside the world.** Audio state is not stored in presentation components. It is held entirely by the audio processor and the audio thread, as the renderer holds GPU state. That is stronger than `#[reflect(presentation)]`: there is nothing to skip. The rule:
  - **Hashed (they're gameplay's requests, made by the simulation):**
    - the `AudioEmitter` and `AudioListener` components;
    - `AnimEventCues`;
    - pending `PlayCue` / `StopCue` signals (hashed like every pending signal).
  - **Not in the world, so never hashed:**
    - voices and their playheads;
    - the mixer, bus runtime gains and mutes (`audio.buses`);
    - the emitter-to-voice map;
    - cue variation draws;
    - streams;
    - preview voices (`audio.play`);
    - the offline capture.
  - **The audio processor:** `audio.collect` runs in `Phase::Presentation`. It reads `Transform`, `AudioEmitter`, `AudioListener` and `AnimEventCues`, receives `PlayCue`, `StopCue` and `AnimEvent`, and **writes nothing**. Its only output is commands pushed into a lock-free queue it owns. Terra's planner already rejects a presentation processor that writes simulation state, and that rule stays the guard.
  - **The proof.** The same run, with the audio processor scheduled and with it left out, gives identical world hashes at every tick, at 1 and 8 threads, across runs and through a replay (`audio_is_presentation_only_and_never_changes_the_world_hash`).
- **How gameplay plays a sound** (Godot's `AudioStreamPlayer` for semantics only: `playing`, buses by name):
  - **Looping or long sounds: an `AudioEmitter` component**, written by gameplay, scripts, the editor or agents:
    - its fields are `cue: AssetRef`, `playing: bool` (default true) and `volume_db: f32` (default 0);
    - while `playing` is true its cue sounds: a looping cue loops, and a one-shot cue plays once each time `playing` turns true;
    - turning it false, or despawning the entity, stops the voice with a 5 ms ramp;
    - position comes from the entity's `Transform` (parents included, as everywhere else).
  - **One-shots: the `PlayCue { cue: AssetRef, volume_db: f32 }` signal**, aimed at an entity: the sound plays at that entity's position, or unpositioned if it has no `Transform`. `StopCue { cue: Option<AssetRef> }` stops what that entity started (that cue only, or everything).
    - A **scripted processor** (2.6) raises it like any signal, with `Command::Signal`, in any simulation phase.
    - A **native processor** uses `CommandBuffer::signal`.
    - A signal raised on tick *t* sounds on tick *t* + 1, about 17 ms later at 60 Hz. That is below what players notice for effects, and it is the price of keeping audio out of the tick that raised it.
  - **Clip events (2.4.4): an `AnimEventCues { cues: BTreeMap<String, AssetRef> }` component.** On a character it maps event names to cues: `"footstep"` plays `step.cue.ron` at the character. So an animator's events sound with no code. Events with no entry are ignored.
  - **Listener:** the lowest-id entity with `AudioListener`. Its `Transform` gives position, right (+x) and forward (−z), so put it on the camera or the player. With none, spatial sounds play centred at full gain, with one warning.
  - **Replays stay exact.** Everything audio reads is hashed simulation state or delivered signals, so a replay reproduces the same requests on the same ticks. Every random draw comes from the cue's seed, not the world RNG. So a replay renders the same audio offline, and audio can't make one diverge.
- **The pipeline:**
  - **Decoding.** The `sound` importer handles `wav` and `ogg`:
    - `hound` reads WAV: 8/16/24/32-bit integer and 32-bit float, any rate, mono or stereo;
    - `lewton` reads Vorbis;
    - both give interleaved `f32`, with integers scaled by 2^(bits−1);
    - more than 2 channels is an import error naming the file.
  - **Import settings** (sidecar `settings`, reflected, `deny_unknown_fields`): `stream: Option<bool>` (default: Ogg longer than 10 s streams), `gain_db` (default 0), `loop_start` and `loop_end` (in frames; optional).
  - **The DDC.** A non-streamed sound is stored decoded as an `AssetKind::Sound` artifact (`SoundData { rate, channels, frames }`, postcard). A streamed one stores its source bytes, decoded while it plays. Re-imports follow the existing key: importer, version, platform, settings and source bytes.
  - **The mixer runs at 48 kHz, stereo `f32`.** It is a plain struct with `render(&mut self, out: &mut [f32])`, the same code in both backends:
    - **the command queue:** `rtrb` single-producer, single-consumer rings carry commands in; a second ring carries retired sounds and streams back out;
    - **no frees on the audio thread:** every `Arc` or `Box` the mixer would drop goes back through that second ring and is dropped on the engine thread.
  - **Buses.** Each has a name, `gain_db`, `mute` and `send` (its parent).
    - Master always exists; the default layout is Master, SFX → Master, Music → Master.
    - A project may name a layout in `[audio] buses = "<path>.buses.ron"`.
    - Sends must form a tree: a cycle or an unknown parent is a load error naming both buses.
    - Buses are summed children-first in a fixed order into preallocated buffers.
    - Gain is 10^(dB/20), and −80 dB or below is silence.
  - **Voices.** A fixed pool (`[audio] voices`, default 32, 1–256), each playing a sound at a playback rate of `source_rate / 48000 × pitch`. Resampling is linear interpolation.
    - **Stealing:** when the pool is full, a new voice takes the lowest-priority voice, the oldest first. If every voice outranks it, the new one is dropped and counted.
    - **Stopping:** a stop is a 5 ms (240-frame) linear ramp.
  - **Streaming (music).** A streamed voice reads from a preallocated ring of 2 s, filled by a decoder:
    - live, on an `audio-stream` thread that wakes every 50 ms;
    - offline, synchronously before each render, so offline renders never depend on timing.
    - A loop seeks back to `loop_start` by re-opening the reader.
    - An underrun plays silence and is counted (`audio.inspect`).
  - **Spatialization** (when the cue has a `spatial` block):
    - **distance:** inverse distance, clamped: g = min / clamp(d, min, max), with `min_distance` (default 1 m) and `max_distance` (default 50 m);
    - **pan:** equal power, from the source's sideways position in listener space: p = x / √(x² + z²) ∈ [−1, 1], θ = (p + 1)π/4, L = cos θ, R = sin θ;
    - **channels:** a stereo source is summed to mono first; a source at the listener is centred; unpositioned voices aren't panned (mono goes to both channels unchanged, stereo plays as is);
    - positions are refreshed per tick for playing voices.
  - **Backends:**
    - **`cpal`:** the default output device. It asks for 48 kHz `f32` and otherwise takes the device's rate and format, then mixes at that rate and converts in the callback.
    - **`offline`:**
      - it renders exactly the frames each tick covers: frames [⌊t·48000/rate⌋, ⌊(t+1)·48000/rate⌋), 800 per tick at 60 Hz;
      - commands apply at their tick's first frame, so renders are sample-exact;
      - it keeps a running BLAKE3 hash, peak and RMS of everything mixed;
      - it stores a capture only when `audio.render` or a test asks for one.
    - **Which backend:** headless engines (`concordia mcp`, tests, CI, replays) always use `offline`; `concordia run` asks for `cpal`. `[audio] backend = "none"` turns mixing off.
  - **The audio thread.** cpal's callback runs only `Mixer::render` and the conversion. It allocates nothing, frees nothing, takes no lock and makes no system call; it only drains the command ring into preallocated slots.
  - **A missing or failing audio device** is one `warn!` naming the error. The engine then falls back to `offline`: never an error, and play continues.
- **Cues: `.cue.ron`, engine-native, canonical text.** A new format in `plans/README.md`'s list; imported by a `cue` importer (`AssetKind::Cue`) like `.animgraph.ron`. Fields:
  - **`sounds`:** a list of `(sound: AssetRef, weight: u32)` entries, at least one, weights ≥ 1, chosen by weight;
  - **variation:** `volume_db: (min, max)` and `pitch_semitones: (min, max)`, each in [−24, 24] with min ≤ max; pitch is applied as 2^(s/12);
  - **routing and limits:** `bus` (a name), `priority: u8`, `looping: bool` and `max_instances` (≥ 1; the oldest instance stops when a new one exceeds it);
  - **space:** `spatial: Option<(min_distance, max_distance)>`;
  - **`seed: u64`.**
  - **The seeded draw:** each play draws from SplitMix64 seeded with `seed ⊕ entity id ⊕ (tick × 2⁸) ⊕ n`, where *n* counts plays from that entity on that tick. The draws are the sound, then the volume, then the pitch, and no clock or thread is involved. So headless renders are bit-identical run to run, at any thread count and through a replay.
  - Validation errors name the field.
- **The Engine API and MCP tools** (module `audio`, `[audio]`; five tools). None writes the world, so none needs an actor:
  - **`audio.play`** `{cue, volume_db?, at?}` → `{voice}`: previews a cue outside the world, so a sound designer can audition it. A voice id is returned.
  - **`audio.stop`** `{voice?}`: stops one preview, or all of them.
  - **`audio.buses`** `{set?: [{name, gain_db?, mute?}]}` → each bus's name, send, gain, mute and peak dBFS since the last call. Changes apply at runtime only; the layout file is the saved mix.
  - **`audio.render`** `{cue?, seconds, path?}` → `{frames, peak_dbfs, rms_dbfs, hash, path}`:
    - with a cue, it renders that cue alone through the bus layout, offline;
    - without one, it reports the play world's offline capture so far;
    - `path` writes a 16-bit WAV under the project (paths checked like `asset.import`).
    - This is how agents and CI "listen": by numbers and by hash.
  - **`audio.inspect`** → the backend, active voices (cue, entity, bus, gain, pan, position), the listener, steals and underruns.
  - The core method list and every role's scope are unchanged except as below. Every role's served-tool numbers are reported in As built, and each must stay under 50k.
- **Studio roles (D8, A1):**
  - **Sound designer:**
    - **enabled**, with paths `assets/sfx/**` and `assets/audio/**` (bus layouts);
    - **tools:** its task and team tools plus `asset_import`, `asset_list`, `entity_get`, `component_get`, `component_set`, `component_patch`, `query_run`, `sim_step` and the five `audio_*` tools;
    - **skills:** a new `concordia-sound-design` (import settings, cues, buses, emitters, `PlayCue`, `AnimEventCues`, listening with `audio_render`). Its examples and RON blocks are parsed as the real types.
    - `concordia-synthesize-sfx` keeps its note, which now names only `gen_run` (M3 3.7).
  - **Composer:**
    - **enabled**, with path `assets/music/**`;
    - **tools:** its task and team tools plus `asset_import`, `asset_list`, `component_get`, `component_patch`, `audio_play`, `audio_stop`, `audio_buses` and `audio_render`;
    - **skills:** a new `concordia-music` (streamed import, loop points, looping music cues on the Music bus, an emitter that starts the music). Its examples are parsed the same way.
  - Both get role tests in the animator's pattern, roster and sync snapshots, and the M3S gate snapshots.
  - **Engineers:** the gameplay programmer's script skill gains a `PlayCue` example.
- **Editor debug view (D7): an `audio` layer**, all lifted 2 cm and tested by its geometry like `crowd`:
  - each emitter's `min_distance` and `max_distance` as horizontal circles (24 segments), green while it plays and grey when idle;
  - the listener's axes;
  - a line from the listener to each playing spatial voice.
- **Windowed play.** `concordia run` builds the `cpal` backend, and `Player` needs no change. Sound starts with play and stops on close, because the backend is dropped with `PlaySetup.state`. The engine thread feeds the queue from `audio.collect`; nothing in the window loop touches audio.
- **Clean room:**
  - **Specifications:** the RIFF/WAVE format (Microsoft/IBM, 1991) and the Vorbis I specification (Xiph.Org).
  - **Crate documentation:** `cpal`, `hound`, `lewton` and `rtrb`.
  - **Standard DSP references:** decibels as 20·log₁₀ of an amplitude ratio; equal-power panning (the cos/sin law, as in Roads, *The Computer Music Tutorial*, 1996); inverse-distance-clamped attenuation (the OpenAL 1.1 specification's model); linear interpolation; and the sample-rate arithmetic above.
  - **SplitMix64:** Steele, Lea and Flood, 2014; already in Terra.
  - **Godot (MIT):** `AudioStreamPlayer` and audio-bus documentation, for semantics only.
  - **Never:** Unreal, Wwise, FMOD or any other non-open source.

**2.7a tests (first list).** The expected values are derived in each test header: 48 kHz, 60 Hz ticks (800 frames each), DC and ramp sources wherever possible so every sample is hand-computable.
- **2.7a.1 `m2/2.7a-mixer`** (library only, in `concordia-audio`; no engine):
  - `gain_and_decibels_convert_both_ways`:
    - 0 dB is a gain of 1;
    - −6.0206 dB is 0.5, and +6.0206 dB is 2;
    - −80 dB and below is silence;
    - 0.5 is −6.0206 dB.
  - `equal_power_pan_follows_the_cos_sin_law`: p = −1 gives (1, 0); p = 0 gives (0.707107, 0.707107); p = 0.5 gives (0.382683, 0.923880); p = 1 gives (0, 1); L² + R² = 1 throughout.
  - `distance_attenuation_is_inverse_clamped` (min 1, max 50): a gain of 1 at 0.5 m, 0.5 at 2 m, 0.25 at 4 m, and 0.02 from 50 m on.
  - `wav_decodes_to_expected_frames`: written in the test with `hound`.
    - 16-bit samples [0, 16384, −16384, 32767] become [0, 0.5, −0.5, 0.999969];
    - also 8-, 24- and 32-bit integer, 32-bit float and stereo;
    - three channels is an error.
  - `ogg_decodes_to_expected_frames`: `tests/fixtures/audio/tone-440.ogg`, 1 s of a 440 Hz sine at −6.0206 dBFS, mono, 48 kHz, made by `tools/fixtures/make_audio.sh` with `oggenc`. It should give:
    - 48,000 frames at 48 kHz;
    - RMS within 0.01 of 0.5/√2 = 0.353553;
    - 880 ± 2 zero crossings.
    - Vorbis is lossy, so these are tolerance checks.
  - `sound_importer_caches_decoded_frames_in_the_ddc`: a second load is a DDC hit; a changed setting re-imports; Ogg over 10 s streams by default.
  - `cue_files_roundtrip_canonical` (insta) and `cue_validation_names_the_field` (empty `sounds`, weight 0, min > max, pitch out of range, `max_instances` 0).
  - `cue_variation_is_seeded_and_in_range`: SplitMix64's published first output for seed 0 (0xE220A8397B1DCDAF) fixes the first draw; the same inputs give the same draws; 10,000 draws stay in range; weights 3:1 split within 2%.
  - `commands_apply_at_their_tick_frame`: tick 3 starts at frame 2,400 (60 Hz), 2,880 (50 Hz) and 1,000 (144 Hz: ⌊3 · 48000 / 144⌋).
  - `mixer_sums_voices_and_applies_bus_gain_db`: DC 0.25 + DC 0.125 on SFX at −6.0206 dB gives 0.1875 on both channels (unpositioned voices aren't panned: mono goes to both channels unchanged, stereo plays as is).
  - `bus_routing_sends_sfx_and_music_to_master`: SFX −6.0206, Music −12.0412 and Master +6.0206 dB, DC 0.5 on each, gives 0.5 + 0.25 = 0.75; muting Music gives 0.5; a cycle is an error naming both buses.
  - `pitch_resamples_by_linear_interpolation`: a ramp of k / 100 at 24 kHz played at 48 kHz gives 0, 0.005, 0.01, …; +12 semitones at 48 kHz gives 0, 0.02, 0.04, ….
  - `one_shots_end_and_loops_wrap`: a 3-frame one-shot then silence; a loop between frames 1 and 3 wraps exactly.
  - `stop_ramps_to_silence_in_5_ms`: DC 1 stopped at frame 0 gives 1 − k / 240 over 240 frames, then 0.
  - `voice_limit_steals_lowest_priority_oldest`: with a limit of 2, A (priority 1) then B (1) then C (2) steals A; D (0) is dropped and counted.
  - `max_instances_stops_the_oldest`.
  - `spatial_voice_attenuates_with_distance_and_pans`: listener at the origin facing −z, DC 1 mono, min 1. At (2, 0, 0) it gives L 0, R 0.5; at (0, 0, −4), L = R = 0.25 × 0.707107 = 0.176777; at (−3, 0, 0), L 1/3, R 0.
  - `streamed_music_does_not_underrun_offline`: the fixture streamed and looped for 30 s, with no underruns. Every frame equals the same file decoded in memory and looped.
  - `offline_backend_renders_bit_identical_buffers`: 10 s of a script (looping and one-shot cues, seeded variation, moving spatial voices, a stream, steals) rendered twice gives equal BLAKE3 hashes and equal buffers.
  - `audio_callback_never_allocates`: `allocation-counter` around `Mixer::render` with 32 voices, a stream, steals, stops and 200 queued commands; zero allocations and zero frees.
- **2.7a.2 `m2/2.7a-engine`** (the `audio` module, `cpal`, tools, Studio):
  - `emitter_plays_while_playing_and_stops_with_a_ramp` (offline capture; the first non-zero frame is tick *t*'s first frame; despawn stops it too).
  - `play_cue_signal_plays_once_at_the_next_tick` (a native processor), and `scripted_processor_plays_a_cue` (a WASM fixture guest raising `PlayCue`).
  - `anim_event_cues_play_on_clip_events` (the fixture character's events).
  - `listener_is_the_lowest_id_and_missing_listener_centres_sound`.
  - `audio_is_presentation_only_and_never_changes_the_world_hash`: the old `audio_events_are_presentation_only_and_do_not_change_world_hash`. With and without `audio.collect`, every tick's hash is equal at 1 and 8 threads and run to run, while cues play.
  - `engine_audio_renders_identically_across_runs_threads_and_replay` (the offline hash at 1 and 8 threads, and after `replay.run`).
  - `missing_device_warns_and_falls_back_to_offline` (a device opener that fails gives one warning, and play continues).
  - `buses_file_loads_and_bad_layouts_are_descriptive`.
  - `audio_methods_have_schemas_and_mcp_tools`, plus an API test for each method (`audio.render` writes a WAV that `hound` reads back with the reported frame count).
  - `audio_debug_layer_draws_emitters_and_listener` (2 × 24 + 3 + one line per playing voice).
  - `cpal_device_plays_a_cue` (`#[ignore]`, manual: an audible check on the reference machine; also `concordia run` on a fixture scene).
  - **Studio:**
    - `sound_designer_and_composer_are_enabled_with_audio_tools_under_budget`;
    - the role-scope server tests for their paths;
    - each new skill's examples test (real params, components and a valid `.cue.ron`);
    - the planned-skills list;
    - snapshots.

**Carry-over (noted, not in 2.7a):**
- the ignored `run_bench_latency_measures_the_engine_window_alone`;
- the `navmesh` layer's re-gather;
- the editor's deny warning;
- ADR-0069's chunk components (M7 7.7);
- physics queries seeing last-step bodies;
- the crowd's sequential steps at 10k (M7 7.7).

**Not in 2.7a:** DSP effects (reverb, filters, compression), HRTF, Doppler, occlusion, crossfades between music cues, audio from the UI (2.7b), the sound-catalog export, Godot sound-bank and bus-layout import (M5), and synthesis (M3 3.7).

### Phase 2.7a.1 As built (2026-11-22, ADR-0066)
**Reconciled with the code as built:** the refinement held (A1 and A2; A1 lands in 2.7a.2). What the code taught:
- **The importers live in `concordia-audio`.** `concordia-assets` only gains `AssetKind::Sound` and `AssetKind::Cue`. Whoever builds the `AssetDb` registers `SoundImporter` and `CueImporter`; that is the `audio` module, from 2.7a.2. The workspace entry for the crate waits for its first user (2.7a.2), so cargo doesn't warn about an unused workspace dependency.
- **Lengths are exact through the Vorbis end trim.** lewton decodes the fixture to 48,832 frames. The final page's granule position (48,000) trims the last block, in `OggStream::read_packet`, so loops are seamless and a streamed sound equals its decoded twin.
- **A positioned stereo source is averaged** ((L + R) / 2), not summed, so centring it doesn't double its level.
- **Gains are clamped at 0.** f32's cos(π/2) is −2.2 × 10⁻⁸, so a hard-right pan produced a negative gain, which the request check rejected. The spatial test caught it.
- **`SoundSettings::streams(is_ogg, frames, rate)`** decides streaming as a pure function, so the 10 s rule is tested without an 11 s fixture. Sidecar settings are a JSON-style map (`"gain_db": 0.0`), like every importer's.
- **The `.buses.ron` reader and writer** (`read_buses` / `write_buses`, validated through `BusGraph`) landed here, so 2.7a.2 only loads the file named by `[audio] buses`.
- **The mixer's design:**
  - `Mixer::new` returns the mixer and a `MixerHandle`.
  - The handle validates requests, opens streams (prefilling their 2 s ring), boxes each voice, and frees retired voices in `collect`.
  - The mixer drains at most 1,024 commands per render.
  - `MixerStats` holds atomics: steals, drops, underruns, active voices and per-bus peaks.
  - A full command queue is `AudioError::QueueFull`, never a block.
- **Offline streams are refilled before every 1,024-frame block**, not once per render. A 30 s render in one call would otherwise underrun; the mutation check showed it.

**Engine:**
- New crate `concordia-audio` (`#![forbid(unsafe_code)]`):
  - `decode` (`decode_wav`, `decode_ogg`, `OggStream`), `sound` (`SoundData`, `Channels`, `Sound`, `SoundBody`, `LoadedSound`, `LoadedBody`);
  - `import` (`SoundImporter`, `SoundSettings`, `STREAM_AFTER_SECONDS`), `cue` (`Cue`, `CueSound`, `Span`, `DrawKey`, `Draw`, `read_cue`, `write_cue`, `CueImporter`);
  - `bus` (`BusLayout`, `BusDef`, `BusGraph`, `BusIndex`, `MASTER`, `read_buses`, `write_buses`), `gain`, `spatial` (`Spatial`, `Listener`, `distance_gain`, `equal_power`, `spatial_gains`);
  - `mixer` (`Mixer`, `MixerConfig`, `MixerHandle`, `PlayRequest`, `VoiceId`, `MixerStats`), `voice`, `stream`, `offline` (`OfflineBackend`, `RenderStats`, `tick_frames`), and `error` (`AudioError`, `CueError`).
- `concordia-assets`: `AssetKind::Sound` and `AssetKind::Cue`.
- Workspace dependencies: `hound` 3.5.1, `lewton` 0.10.2, `rtrb` 0.4.1, and the dev-only `allocation-counter` 0.8.1 (A2).
- Fixtures: `tests/fixtures/audio/tone-440.ogg` and `tools/fixtures/make_audio.sh` (byte-identical run to run).
- Docs:
  - ADR-0066 (new);
  - the AGENTS.md and README of `concordia-audio`, and the AGENTS.md of `concordia-assets`;
  - `plans/README.md` (the crate, `.cue.ron` and `.buses.ron`, the ADR registry and the index), `tests/fixtures/README.md`, and this plan.
  - No method, tool or Studio file changed in 2.7a.1, so `docs/guides/agents-engine-api.md` and the skills wait for 2.7a.2.

**Tests (34 new, all in `concordia-audio`):**
- `tests/math.rs` (5):
  - `gain_and_decibels_convert_both_ways`;
  - `equal_power_pan_follows_the_cos_sin_law`;
  - `distance_attenuation_is_inverse_clamped`;
  - `spatial_gains_follow_the_listener` (including a turned listener, height and no listener);
  - `commands_apply_at_their_tick_frame` (2,400, 2,880, 666 and 1,000).
- `tests/decode.rs` (2): `wav_decodes_to_expected_frames` (8-, 16-, 24- and 32-bit integer, float, stereo, three channels refused) and `ogg_decodes_to_expected_frames` (48,000 frames; RMS 0.353553 ± 0.01; 880 ± 2 crossings).
- `tests/import.rs` (5):
  - `sound_importer_caches_decoded_frames_in_the_ddc`;
  - `short_ogg_decodes_and_stream_setting_keeps_its_bytes`;
  - `long_ogg_streams_by_default`;
  - `bad_sources_and_settings_are_import_errors_naming_the_file`;
  - `cue_importer_reads_and_validates_cue_files`.
- `tests/cue.rs` (3):
  - `cue_files_roundtrip_canonical` (insta);
  - `cue_validation_names_the_field`;
  - `cue_variation_is_seeded_and_in_range`: SplitMix64's published 0xE220A8397B1DCDAF and its successors give sound 1, −3.410832 dB and a ratio of 0.896357; 3:1 weights give 7,300–7,700 of 10,000.
- `tests/mixer.rs` (12):
  - `mixer_sums_voices_and_applies_bus_gain_db` (0.1875) and `bus_routing_sends_sfx_and_music_to_master` (0.75, 0.5 muted);
  - `bus_layouts_are_validated_by_name` (a cycle, an unknown send, a duplicate, Master with a send, gain; a chain gives 0.25);
  - `bus_layout_files_roundtrip_canonical` (insta);
  - `pitch_resamples_by_linear_interpolation` (0, 0.005, 0.01, …; 0, 0.02, 0.04, …) and `one_shots_end_and_loops_wrap`;
  - `stop_ramps_to_silence_in_5_ms`, `voice_limit_steals_lowest_priority_oldest` (11 → 110, then a drop) and `max_instances_stops_the_oldest`;
  - `spatial_voice_attenuates_with_distance_and_pans` ((0, 0.5), 0.176777, (1/3, 0), stereo averaged 0.353553);
  - `bad_requests_are_refused_before_the_queue` and `a_full_queue_is_an_error_not_a_block`.
- `tests/stream.rs` (3): `streamed_music_does_not_underrun_offline` (30 s looped, bit-equal to the decoded loop, no underruns), `a_one_shot_stream_ends_like_its_decoded_twin` and `stream_loop_points_wrap_like_memory`.
- `tests/offline.rs` (3): `ticks_render_their_frames_and_commands_land_on_the_tick` (the first sound at sample 4,800), `stats_report_peak_rms_and_hash` and `offline_backend_renders_bit_identical_buffers` (10 s, 600 ticks, with steals).
- `tests/no_alloc.rs` (1): `audio_callback_never_allocates` (32 voices, a stream, 200 commands; zero allocations, net zero).

**Red run:** against stubs (unit gains, no decoding, empty imports, a mixer rendering silence, a handle accepting everything), all 34 tests failed for the right reason. The exceptions:
- **Test mistakes, fixed deliberately:**
  - three import tests first edited the sidecar as `gain_db: 0.0`, but settings are a JSON-style map (`"gain_db": 0.0`), so nothing changed;
  - the 10 s script first played one-shots every 7 ticks, which never filled the pool of 4, so it never stole; it now plays one every 2 ticks.
- **A real bug the tests found:** the negative pan gain (above).
- **Written after the code:** `bus_layout_files_roundtrip_canonical` (the reader and writer reuse `concordia_scene`'s documents). It is checked by the mutations below that break the layout.
- **Passed immediately:** none of the 34. The two cases added for the mutation survivors (below) passed with the code, and each was shown red by its mutation.

**Mutation checks:** 21 mutations, each run against its test file with the restored file given a fresh mtime. 19 were caught at once. The 2 survivors were fixed with new cases:
- "the pan uses 3D distance" was missed because no source was raised; (2, 2, 0) now catches it (0.353553, fully right);
- "tick frames rounded" was missed because 144 Hz's tick 3 is whole; tick 2 ([666, 1000)) now catches it.

The caught ones:
- **The mix callback:** it allocates; a retired voice is freed on the audio thread.
- **Stealing:** the newest is stolen; a voice is stolen even when it outranks the request; `max_instances` lets one too many through.
- **Math:** pan cos/sin swapped; no minimum-distance clamp; the ramp off by one; no interpolation; no stereo downmix.
- **Buses:** parents summed before children; mute ignored.
- **Voices:** a one-shot loops.
- **Decoding:** Ogg not trimmed; WAV scaled by 2^bits.
- **Streams:** streams refilled only once per render; the stream loop ignoring `loop_start`.
- **Cues:** the weighted pick off by one; the draw ignoring the tick.

**Studio and budgets:** no tool, tool schema or role changed in 2.7a.1, so every role's served tools are unchanged from 2.5d's numbers (all under 50k). They are measured again in 2.7a.2, when the audio tools land.

**Goldens and snapshots:** two new, `cue__cue_files_roundtrip_canonical` and `mixer__bus_layout_files_roundtrip_canonical`. They were collected with `INSTA_FORCE_PASS=1 INSTA_UPDATE=new` and are shown on an artifact page for the author's review before blessing. No existing snapshot or render golden changed.

**Carry-over:**
- the ignored `run_bench_latency_measures_the_engine_window_alone`;
- the `navmesh` layer's re-gather;
- the editor's deny warning;
- ADR-0069's chunk components (M7 7.7);
- physics queries seeing last-step bodies;
- the crowd's sequential steps at 10k (M7 7.7).

**Checklist:** all of these pass:
- `just lint` and Tauri clippy;
- `just wasm`, `just native-fixtures`, then the `CONCORDIA_REQUIRE_GPU=1` full suite with `--no-fail-fast` (1,578 tests: 1,578 pass, 9 skipped; 14 doc tests);
- `just gate-tests M1` (4) and `M3S` (13);
- `just deny` and the editor's, both `Cargo.lock`s, fresh advisory DB. The engine's lock gains `hound`, `lewton`, `ogg`, `rtrb` and the dev-only `allocation-counter`; the editor's is unchanged and prints the existing "no crate matched" warning;
- `just editor-schemas` (no change);
- `just editor-test` (118 unit, 22 end-to-end).

After blessing, one full re-run saw `m3s_live_checker_rejects_an_edit_by_the_wrong_actor_and_a_missing_review` (the M3S live checker, which drives an MCP server in a temp project) fail once under load. It passed alone five times and in the next full run (1,578 of 1,578); it touches nothing in audio.

### Phase 2.7a.2 As built (2026-11-22, ADR-0066 decision 8)
**Reconciled with the code as built:** the refinement held (A1 enabled both roles). What the code taught:
- **`AudioListener` has a `gain`** (default 1), which scales positioned sounds. The refinement had it as a marker.
- **Positions compose parents from the processor's own columns.** A processor can't take the `World`, so `concordia_scene::builtin::world_matrix_with` composes from `Transform` and `Parent` lookups, with `world_transform`'s rules. `world_transform` now delegates to it, so there is one implementation.
- **Every imported cue loads at play start**, with its sounds, because a `PlayCue` may name any cue. `audio.play` and `audio.render` load others on demand.
- **Runtime bus settings last across plays.** A test found that play start's fresh offline mixer forgot an `audio.buses` change; play start now re-applies them.
- **`audio.render` without a cue** reports the play world since play start, and its WAV keeps the newest 30 s (`keep_last`). With a `path` it writes, so the MCP server checks that path against the role's paths, like `asset_import` (ADR-0080).
- **The device:**
  - cpal streams aren't `Send` everywhere, so the stream lives on a `concordia-audio-device` thread until its guard drops;
  - `concordia-audio-stream` refills streams and frees retired voices every 50 ms;
  - the callback renders in 4,096-frame blocks into preallocated scratch and converts to `f32`, `i16`, `u16` or `i32`.
- **`Project::engine_config_for(Playback::Windowed)`** is what `concordia run` uses. `audio_settings(playback)` reads and validates `[audio]` (voices 1–256, the bus file, the backend), and `auto` is the device only when windowed.
- **The library gained** `MixerHandle::{is_live, live}` (voices live until retired and collected) and `OfflineBackend::keep_last`. `write_wav16` writes s × 32768, rounded and clamped.
- **The emitter rule:**
  - a one-shot emitter restarts only when `playing` turns true;
  - `StopCue` stops what signals and clip events started, never an emitter;
  - two plays from one entity on one tick get different draws, because *n* counts them.
- **The debug layer** draws emitters whose cue is loaded (after a play, or a method that loaded it), so the edit world shows them once a play has run.
- **Studio:**
  - nine enabled roles outnumber Claude Code's eight agent colors, so `every_default_role_has_a_color_and_enabled_roles_differ` now requires distinct exact colors, with every named color used before any repeats. This is a deliberate fix: the old rule can't hold. The sound designer is orange and the composer `#14b8a6`;
  - both roles save scene files, so `scenes/**/*.scene.ron` joins `[paths] shared`;
  - `concordia new` writes `[audio]` and `studio init` adds it when missing, as `[anim]` was for the animator.
- **No new `unsafe`.** cpal's platform code is a dependency, and our callback and threads are safe Rust.

**Engine:**
- `concordia-audio`:
  - `components` (`AudioEmitter`, `AudioListener`, `AnimEventCues`, `PlayCue`, `StopCue`, `register_types`, with `Cue` and `BusLayout` for `schema.get`), `library` (`AudioLibrary`), and `core` (`AudioCore`, `AudioShared`, `AudioSettings`, `Backend`, `AudioDevice`, `OpenDevice`, `VoiceInfo`);
  - `collect` (`Collector`, `audio.collect`), `methods` (`audio.play`, `audio.stop`, `audio.buses`, `audio.render`, `audio.inspect`), `debug` (the `audio` layer), `device` (`CpalDevice`), `module` (`AudioModule`), `wav`, and `importers()`.
- `concordia-scene`: `world_matrix_with`.
- `concordia-app`: `[audio]` (`AudioSection`, `AudioBackendChoice`), `Playback`, `engine_config_for`, `modules_for` and `audio_settings`; every project registers the audio importers.
- `concordia-cli`: `concordia run` is windowed; `concordia new` writes `[audio]` and `studio init` adds it.
- `concordia-mcp`: `audio.render`'s path check.
- Workspace: `cpal` 0.18.2 (no default features), and the new `audio-guest` WASM fixture (`just wasm`, `just lint`).
- Studio:
  - the sound designer (24 tools) and the composer (20) are enabled;
  - new skills `concordia-sound-design` and `concordia-music`;
  - `concordia-synthesize-sfx`'s note names only `gen_run`;
  - `concordia-scripting` gains "Sound from a script".
- Docs:
  - ADR-0066 (decision 8 and its consequences), ADR-0073 (the budgets) and ADR-0080 (the path check);
  - the AGENTS.md of `concordia-audio`, `-app`, `-cli`, `-mcp`, `-scene` and `-studio`, and the README of `concordia-audio`;
  - `docs/guides/agents-engine-api.md` (an Audio section and five methods), `plans/README.md` (the crate, the methods, the ADR registry and the index), the `audio-guest` README, and this plan.

**Tests (35 new, 1 manual, 8 extended):**
- `concordia-audio` (18, plus 1 manual):
  - **Library:** `the_handle_tracks_live_voices`, `a_bounded_capture_keeps_the_last_frames` and `wav_writer_scales_by_32768_and_clamps`.
  - **`tests/engine.rs`** (8):
    - `emitter_plays_while_playing_and_stops_with_a_ramp` (0.5 from frame 0; the ramp at 2,400 and, after gameplay's despawn, at 4,000);
    - `play_cue_signal_plays_once_at_the_next_tick` (frames 2,400..2,500);
    - `listener_is_the_lowest_id_and_missing_listener_centres_sound` (0.353553, then right 0.25, then 0.125 at gain 0.5);
    - `audio_is_presentation_only_and_never_changes_the_world_hash` (30 ticks, with and without `audio.collect`, at 1 and 8 threads);
    - `engine_audio_renders_identically_across_runs_threads_and_replay`;
    - `missing_device_warns_and_falls_back_to_offline` (one warning);
    - `two_plays_on_one_tick_draw_differently`;
    - `stop_cue_stops_what_the_entity_started`.
  - **`tests/api.rs`** (7): registration by name; render of a cue (96,000 frames, −6.0206 dBFS, the hash, the WAV) and of the play world, with bad params at their pointers; previews and stops; buses (0.25 at −6.0206 dB, peaks of −12.0412 then −80, mute); inspect; and the debug layer (48 + 3 + 1 lines, radii 1 and 50, grey when idle).
  - **`tests/device.rs`:** `cpal_device_plays_a_cue` (`#[ignore]`, manual). It ran on the reference machine: a −30 dBFS tone played to its end in real time.
- `concordia-scene` (1): `world_matrix_with_composes_from_lookups`.
- `concordia-app/tests/audio.rs` (4):
  - `anim_event_cues_play_on_clip_events`: Idle's `breathe` at 0.458333 s is crossed in tick 27 and sounds from frame 22,400, then 70,400;
  - `scripted_processor_plays_a_cue`: the `audio-guest` WASM module chimes on ticks 0, 30 and 60, sounding at 800, 24,800 and 48,800;
  - `buses_file_loads_and_bad_layouts_are_descriptive`;
  - `the_backend_follows_the_section_and_the_playback`.
- `concordia-cli` (1 new, 2 extended): `studio_init_adds_audio_when_missing_for_the_sound_designer`, and `[audio]` in both new-project tests.
- `concordia-mcp` (4 new, 2 extended):
  - `audio_methods_have_generated_schemas_and_mcp_tools` (all read-only) and `agent_renders_an_imported_cue_over_mcp`;
  - `server_holds_the_audio_roles_to_their_paths` and `sound_designer_and_composer_are_enabled_with_audio_tools_under_budget`;
  - `ENABLED` and the manifests of `every_enabled_default_role_stays_under_its_tool_budget`, `role_scope` and `shared_engine` gain the roles or `[audio]`.
- `concordia-studio` (7 new, 4 extended):
  - **roles:** `sound_designer_wires_and_listens_with_its_tools_and_skill` and `composer_brings_finished_music_with_its_tools_and_skill`;
  - **skills:** `sound_design_skill_examples_are_real_params_components_and_a_valid_cue` and `music_skill_examples_are_real_params_components_and_a_valid_cue` (calls, components, cue and layout parsed as the real types), `synthesize_sfx_waits_only_for_the_synthesizer` and `scripting_skill_mirrors_play_cue_and_stop_cue`;
  - **hook:** `audio_roles_write_only_their_audio_paths_and_scenes`;
  - **extended:** the planned skills, the synced agent files, the director's spawnable roles, and the color test (above).

**Red run:** against a skeleton (a processor that did nothing, methods returning `Internal`, an empty debug layer, `[audio]` always offline with defaults, no Studio changes), every new test failed for the right reason. The exceptions:
- **Passed immediately:**
  - `audio_methods_are_registered_by_name` (the skeleton already registered them);
  - `scripted_processor_plays_a_cue` (written after `PlayCue` worked; shown red by its mutation).
- **Written after the code, each shown red by its mutation:** `stop_cue_stops_what_the_entity_started`, `two_plays_on_one_tick_draw_differently` and `wav_writer_scales_by_32768_and_clamps` (added for mutation survivors).
- **Test mistakes, fixed deliberately:**
  - three tests first edited the running game with `entity.spawn` / `entity.despawn` (which act on the edit world) and `component.remove` (no play-world form). They now use play-world `component.set` and a gameplay despawn (`test.Doom`);
  - two rigs first minted different asset ids, so emitters' `cue` values (hashed) differed. The rig now pins every sidecar;
  - the clip-event test double-prefixed an asset reference;
  - the expected scopes weren't sorted as the roster sorts them;
  - the inspect test's cue count rose when the rig gained a cue;
  - the color test, which can't hold with nine roles (above).
- **A real bug the tests found:** runtime bus settings lost at play start.

**Mutation checks:** 25 mutations, each restored file given a fresh mtime. The first run's two "`PlayCue` ignored" mutations didn't apply (rustfmt had reflowed the line), and were rerun and caught. 19 were caught at once. The 4 survivors were fixed with new cases:
- previews all keyed to tick 0 (no test previewed a varied cue twice);
- the WAV scaled by 32,767 (0.5 rounds alike either way; 0.75 doesn't);
- the per-entity draw count never counting;
- `StopCue` ignored (it had no test).

The caught ones:
- **Emitters:** restarting every tick, never stopping, and despawned emitters playing on.
- **Signals and events:** `PlayCue` ignored (engine and script) and `AnimEvent` ignored.
- **The listener:** the highest id; gain ignored; voices never re-panned; no listener meaning silence.
- **Buses and the device:** runtime buses lost at play; a device failure becoming an error.
- **Methods:** `audio.stop` reporting nothing; `audio.render` ignoring `seconds`.
- **The debug layer:** idle circles green, nothing lifted, radii swapped.
- **The server:** `audio.render`'s path unchecked.
- **Scene and app:** parents ignored; windowed playing offline; voices unchecked.

**Studio and budgets** (served definitions, 4 chars a token):
- sound designer 24 tools ≈15.1k (15,138) and composer 20 ≈13.1k (13,054), both new;
- unchanged: level designer 37 ≈23.1k (23,078), QA 24 ≈21.5k (21,519), tech artist 23 ≈20.1k (20,106), animator 22 ≈16.8k (16,824), gameplay programmer 17 ≈12.9k (12,946), director 13 ≈12.5k (12,453), writer 11 ≈9.6k (9,576);
- all under 50k.

**Goldens and snapshots:** six, all from enabling the sound designer and composer:
- new: Studio's `agent_composer` and `agent_sound-designer`;
- changed: `agent_director` (it may spawn both) and `sync_generates_expected_claude_md_block`, and the M3S gate's `m3s_agents_and_skills` and `m3s_claude_md_block`.

They were collected with `INSTA_FORCE_PASS=1 INSTA_UPDATE=new` and shown on an artifact page for the author's review before blessing. No render golden changed.

**CI follow-up:** the first push failed `lint` and `editor ui` on Linux: cpal's `alsa-sys` needs ALSA's headers, which the Mac runs never build. Every Linux job that builds the workspace (`lint`, `editor ui`, and the regression gate on Ubuntu) now installs `libasound2-dev`.

**Manual checks:** the cpal test above. `concordia run` on a scene with sound was not run in this phase (it opens a window); the sample game (2.10) will exercise it.

**Carry-over:**
- the ignored `run_bench_latency_measures_the_engine_window_alone`;
- the `navmesh` layer's re-gather;
- the editor's deny warning;
- ADR-0069's chunk components (M7 7.7);
- physics queries seeing last-step bodies;
- the crowd's sequential steps at 10k (M7 7.7).

**Not in 2.7a.2:** DSP effects, HRTF, Doppler, occlusion, music crossfades, UI sounds (2.7b), Godot sound-bank and bus-layout import (M5), and synthesis (M3 3.7).

**Checklist:** all of these pass:
- `just lint` (with the new `audio-guest`) and Tauri clippy;
- `just wasm`, `just native-fixtures`, then the `CONCORDIA_REQUIRE_GPU=1` full suite with `--no-fail-fast` (1,613 tests: 1,613 pass, 10 skipped; 14 doc tests);
- `just gate-tests M1` (4) and `M3S` (13; its two snapshots pending review, as above);
- `just deny` and the editor's, both `Cargo.lock`s, fresh advisory DB. Both locks gain `cpal` and its platform crates (`coreaudio-rs` and the `objc2-*` audio crates; `alsa` on Linux), and the editor's also gains `concordia-audio`, `hound`, `lewton`, `ogg` and `rtrb` through `concordia-app`. The editor's prints the existing "no crate matched" warning;
- `just editor-schemas` (no change: the editor's example project has no `[audio]`);
- `just editor-test` (118 unit, 22 end-to-end).

### Phase 2.7b refinement (2026-11-22, against the code after 2.7a.2)
**Status: refined; author decisions U1–U12, P1 and P2 taken (2026-11-22); 2.7b.1, 2.7b.2 and 2.7b.3 done (see their As builts); phase 2.7b complete. Three sub-phases, three PRs: 2.7b.1 `m2/2.7b-display`, 2.7b.2 `m2/2.7b-interact`, 2.7b.3 `m2/2.7b-text-world`.**

Refined against `main` after #69: the original "Phase 2.7: concordia-audio and concordia-ui" (below), the M2 refinement's phase table (its 2.7 row), the 2.7a refinement, ADR-0063, ADR-0069, ADR-0072, ADR-0073, ADR-0076 and ADR-0078. **This section wins wherever they disagree.** From the original phase:
- **Out, to M5 (D1):** `control_tree_imports_to_ui_prefab`, the `Control` mapping in `mappings/nodes.ron`, the HUD-scene import to a UI prefab, and routing `canvas_item` gdshaders as UI materials.
- **Kept and made exact:**
  - the anchor/offset layout (U7, now);
  - the box, margin, center, grid and panel containers (now taffy flex and grid, plus `Stack`);
  - stretch from a base size;
  - progress fill modes (generalized; nothing game-specific, so no "orbs");
  - nine-patch;
  - label shaping, wrapping and alignment;
  - the pointer taking a click before the world does;
  - commands with an actor;
  - the HUD and damage-number goldens, over our own fixtures.
- **Widened (U1, U3):** from a HUD to a generic widget set like Godot's core. Nothing is game-specific: buttons, toggles (checkbox, switch, radio), sliders, dropdowns, tabs, tooltips, separators, list views, spin boxes, text boxes and two-way scroll containers. Every widget is styled through `UiStyle` and themes.
- **Changed (P1):** text is **parley** plus **swash**, not cosmic-text (see Dependencies).

**What exists (checked in code):**
- **No UI anywhere.** There's no `concordia-ui`, no `Ui*` type, no fixture. ADR-0060 is reserved and not yet written. `plans/README.md:111` lists the crate.
- **Rendering** (`concordia-render`):
  - **No vertex buffers.** Every shader pulls its data through bindless indices in push constants (`DrawPush`, `renderer.rs:791`).
  - **Overlays:** screen-space triangles, 6 floats per vertex (`overlay.rs:290`), drawn inside the tonemap pass after the fullscreen triangle (`renderer.rs:1563`) with `BlendMode::PremultipliedAlpha`. No textures, no depth, no scissor.
  - **The RHI** has no scissor or viewport command (`concordia-rhi/src/command.rs`). `CopyBufferToTexture` copies whole mips, so there are no region updates. `Format::R8Unorm` exists but no asset format maps to it (`gpu.rs:245`).
  - **Formats:** the offscreen output is `Rgba8UnormSrgb`; the window surface is `Bgra8UnormSrgb`.
  - **Samplers:** a fixed static set (`LinearClamp`, `LinearRepeat`, `NearestClamp`, `NearestRepeat`).
  - **Debug layers** come from modules (`EngineModule::debug_layers` / `debug_geometry`). They're gathered on the engine thread (`module.rs:255`) and carried in `RenderRequest.debug`. This is the pattern the UI draw list follows.
- **Captures** (`capture.screenshot`): 768×432 by default, image pixels, origin top-left, y down. DPI only reaches captures through `Overlays.scale`, and game views get no DPI scale.
- **Windowed play** (`concordia-platform` window.rs, `concordia-app` player.rs):
  - winit 0.30; cursor positions are logical pixels; `Player` keeps physical size and scale.
  - Key repeats, `KeyEvent.text` and `Ime` are dropped. There's no cursor-icon API.
- **Input** (`concordia-input`):
  - `InputEvent` has no text or char event, and there's no gamepad support.
  - `InputSampler` samples into the `ActionStates` resource in `PrePhysics`. `nav.click` reads `ActionStates` directly.
  - Replay records `ActionStates` diffs only (`concordia-replay/src/record.rs:45`).
- **Determinism:**
  - The world hash covers entity ids, non-presentation components, the clock, the RNG and pending signals. Resources aren't hashed.
  - `#[reflect(presentation)]` (ADR-0076) marks presentation components.
  - `Presentation` processors may write only those components and get no command buffer. Exclusive systems aren't checked.
- **Tweens** (2.4.4): `Tweens { tweens: Vec<PropertyTween { component, field (JSON pointer), to, … }> }`. They target any registered component's numeric fields, run as an exclusive system in `FrameEnd`, and are hashed.
- **Assets:**
  - `ImageImporter` (png, jpg, hdr) produces `TextureData`.
  - `AssetKind` has no `Font` or `Theme` kind.
  - `AssetRef` is either `asset:<uuid>[#sub]` or `builtin:<kind>/<name>`.
- **Projection:** `world_matrix_with` (`concordia-scene/src/builtin.rs:139`), and `View::project` (`concordia-render/src/view.rs:41`, world → pixels, y down).
- **Studio:**
  - `ui-ux` is disabled (`team.toml:20-22`), has 15 tools and no UI skill. Its `color: cyan` equals the tech artist's, so enabling it as is fails `every_default_role_has_a_color_and_enabled_roles_differ`.
  - `role_tool_budget.rs` measures nine enabled roles; the largest is the level designer at 23,078 tokens.
  - `just editor-schemas` reads `examples/m1-physics-stack`, which has no `[ui]`.
- **Dependencies** (probed in scratch crates against both `deny.toml`s with a fresh advisory DB, 2026-11-22):
  - `taffy` 0.15.1 (MIT): passes.
  - `cosmic-text` 0.19.0, the latest, even with `default-features = false` and `std` + `swash`: **fails advisories**. Through `fontdb` 0.23 it pulls `ttf-parser` 0.25.1, flagged unmaintained by RUSTSEC-2026-0192, with no safe upgrade.
  - `parley` 0.13.0 (Apache-2.0/MIT, Linebender) with `default-features = false, features = ["std"]`: passes `licenses`, `bans`, `sources` and `advisories` for the engine and the editor. It brings `fontique` 0.13 (no system fonts without its `system` feature), `harfrust` 0.15 (shaping), `skrifa` / `read-fonts`, and ICU4X 2.x segmenters (Unicode-3.0).
  - `swash` 0.2.10 (Apache-2.0/MIT): passes.
  - No `-sys` crate, and nothing needs a Linux system library.
  - `arboard` 3.7.0: on Linux it uses `x11rb` (pure Rust), so no system library. On Windows it brings `clipboard-win` 5.4.3 and `error-code` 3.4.0 (BSL-1.0), which **fail `licenses`** until BSL-1.0 is allowed (P2).

**Author decisions (2026-11-22):**
- **U1. All of it in M2, as three sub-phases and three PRs:**
  - 2.7b.1: display widgets, layout, style, text and the render pass;
  - 2.7b.2: interaction, focus, scrolling and sounds;
  - 2.7b.3: text boxes, spin boxes, list views and world-space UI.
- **U2. Styling:** an inline `UiStyle` on each widget, plus an optional shared `.theme.ron` with variants per state (normal, hover, pressed, disabled, focused), like Godot's Theme. Shadows and blur come later.
- **U3. Extra widgets:** slider, dropdown, tab container, separator, tooltip, list view and spin box.
- **U4.** "Texture" means tiled or stretched fills and atlas sub-regions. Live camera views (render to texture) come later.
- **U5. Rich text:** styled runs (font, size, weight, italic, colour per run). No markup language.
- **U6. Text boxes:** single- and multi-line, caret, selection, clipboard, maximum length, password masking. No IME.
- **U7. Anchors now (2.7b.1),** with Godot's documented anchor and offset semantics.
- **U8. Fonts:** OFL-1.1 is accepted **for font files only**, recorded in ADR-0060 and `licenses/fonts.md`.
  - We ship Noto Sans v2.015 (Regular, Bold, Italic, BoldItalic) and Noto Sans Mono v2.014 (Regular), unhinted TTFs from the notofonts `latin-greek-cyrillic` releases, about 2.1 MB in all.
  - `cargo deny` checks crates, not files, so the allow lists don't change for fonts.
- **U9. One `UiToggle`** covers the checkbox, switch and toggle button (`look`). Toggles that share a `group` are radios.
- **U10. Keyboard focus** (Tab and Shift-Tab, arrows, Enter and Space) through named input actions. Gamepad devices come later and bind to the same actions.
- **U11. World-space UI is always drawn on top in M2.** It's sorted far to near and hidden when its anchor is behind the camera or off screen. Depth occlusion comes later.
- **U12. The OS clipboard** in the windowed player through `arboard`. Headless runs use an in-engine clipboard. Pasted text is recorded as typed input.
- **P1. parley plus swash for text,** not cosmic-text: it passes deny with no new ignore. ADR-0060 records the change.
- **P2. BSL-1.0 joins both allow lists** (applied in 2.7b.3 with arboard).

**My calls (say if you disagree):**
- **Crates and ADR.**
  - **One new crate, `concordia-ui`**, with `#![forbid(unsafe_code)]` and no GPU code. It holds:
    - the components and signals;
    - `.theme.ron`;
    - layout: taffy plus an anchor pass;
    - text: parley shaping and swash rasterizing into an atlas packed on the CPU;
    - the draw-list builder;
    - interaction and focus (2.7b.2);
    - the `font` and `theme` importers;
    - the built-in fonts;
    - the `[ui]` engine module and the `ui.*` methods.
  - **`concordia-render` gains the drawing side and doesn't depend on `concordia-ui`.** The draw-list types (`UiDrawList`, `UiQuad`, `UiPaint`, `UiAtlas`) live in `concordia-api` beside `DebugGeometry`. A new module hook, `EngineModule::ui_draw(engine, world, &UiFrame) -> Option<UiDrawList>`, is gathered on the engine thread into `RenderRequest.ui`, exactly as debug layers are. The UI and the renderer meet only at that seam (SOLID; no cycle).
  - **ADR-0060 is written now** under its reserved title, widened to the whole design:
    - retained ECS UI, taffy plus anchors, parley and swash;
    - the determinism boundary;
    - the draw-list seam;
    - clipping in the shader;
    - the font licence rule;
    - rejected: egui, a webview, a port of Godot's `Control`/`Container` system, and cosmic-text (the advisory).
  - Two other ADRs get updates: ADR-0072 in 2.7b.2/2.7b.3 (the input frame gains viewport and text, with a format version bump), and ADR-0073's consequences (the ui-ux numbers).
- **The determinism boundary.**
  - **Hashed simulation state:**
    - every UI component;
    - `UiState { hovered, pressed, focused }` (2.7b.2), hashed because "fire on release over the same widget" needs memory;
    - every widget value: toggle `on`, slider `value`, the dropdown's `selected`, tab and list selection, scroll `offset`, a text box's `text`, `caret` and `anchor`;
    - pending UI signals.
  - **Never hashed (derived or outside the world):**
    - layout rects;
    - shaped lines and glyph positions;
    - the glyph atlas;
    - draw lists;
    - the font database;
    - the OS clipboard;
    - the cursor icon.
  - **Layout is a pure function:** `layout(world, assets, viewport) -> UiLayout`, a function of hashed components, the fonts' and images' bytes, and the viewport. The simulation calls it with the *input* viewport for hit tests (2.7b.2). The draw path and `ui.inspect` call the same function with the capture's size. That's one implementation, and simulation and drawing agree whenever the sizes match.
  - **The viewport is input (2.7b.2):** `InputEvent::Viewport { width, height, scale }`, sent by the player on open, resize and scale change. It's sampled into the per-tick input frame and recorded by replay. A headless run's viewport is each root's base size unless a test injects one.
  - **Typed text is input (2.7b.3):** `InputEvent::Text(TextInput::Insert(String) | Edit(EditKey))`, with key repeats kept for editing. It's recorded by replay; a paste is an `Insert`.
  - **How gameplay hears the UI:**
    - signals on the widget's entity, delivered next tick like every signal: `UiPressed`, `UiToggled { on }`, `UiValueChanged { value }`, `UiSelected { index }`, `UiTextChanged`, `UiTextSubmitted`, `UiFocusChanged { focused }`;
    - the component values themselves (a processor can just read `UiSlider.value`).
  - **The click goes to the UI first (2.7b.2):** `ui.interact` is an exclusive system in `PrePhysics`, ordered after `input.sample` and before `nav.click`. When the pointer is over an interactive widget, it marks the pointer actions it used in a new `concordia-input` resource, `ConsumedActions` (cleared by `input.sample`). World readers use `ActionStates::unconsumed(name)`, and `nav.click` switches to it. So the HUD never triggers click-to-move, and `concordia-nav` doesn't depend on `concordia-ui`.
  - **Mutations:** every UI edit is `entity.*` / `component.*` with an actor. Play-world edits use `world: "play"` (ADR-0075). Interaction runs inside the simulation from recorded input, so the editor has no UI logic of its own.
  - **The proof:**
    - 2.7b.1: a scene full of UI hashes the same with the module on and off, at 1 and 8 threads.
    - 2.7b.2 and 2.7b.3: a replay with clicks, drags, typing and a resize is exact at 1 and 8 threads.
- **Coordinates.**
  - UI space is **logical pixels, origin at the root's top left, y down**.
  - A root's canvas maps UI space to the viewport (stretch, below). Physical pixels are logical × the device scale.
  - A capture is logical at scale 1 unless `capture.screenshot` gets `ui_scale`.
  - `UiFrame { width, height, scale }` is what the draw hook receives: the target's pixel size and its pixels per logical pixel.
- **Layout.**
  - **Taffy** lays out every node's children by the node's `layout`: `Flex` (CSS Flexbox), `Grid` (CSS Grid, explicit tracks only), or `Stack` (children overlap at the content box, like Godot's `PanelContainer` and `MarginContainer`).
  - **Then the anchor pass** places `Anchored` children inside their parent's rect, using Godot's `Control` rules:
    - `left = parent.x + anchor_left × parent.w + offset_left`;
    - `right = parent.x + anchor_right × parent.w + offset_right`;
    - and the same for top and bottom;
    - when the rect is smaller than the node's minimum size, `grow_h` / `grow_v` (`Begin | End | Both`) choose which side moves.
  - **The 16 Godot layout presets** are constructors (`Anchors::preset(Preset::TopRight, size, margin)`), not stored state.
  - **Stretch from a base size,** with Godot's `canvas_items` semantics. For base size B and viewport V, and *s* = min(Vw/Bw, Vh/Bh):
    - `Disabled`: scale 1, and the UI space is V.
    - `Ignore`: a non-uniform scale (Vw/Bw, Vh/Bh), and the UI space is B.
    - `Keep`: scale *s*; the UI space is B, centred with bars.
    - `KeepWidth`: scale *s*. When the viewport is taller than the base's aspect, the UI space is (Bw, Vh/*s*); otherwise it is B with side bars.
    - `KeepHeight`: the mirror of `KeepWidth`.
    - `Expand`: scale *s*, and the UI space is (Vw/*s*, Vh/*s*), with no bars.
    - Integer scaling is later work.
  - **Intrinsic sizes:**
    - text measures through parley (a taffy measure function; it wraps at the available width);
    - images measure at their texture's size (`size: Auto`), the region if one is set.
- **Text.**
  - **Fonts:** `UiFonts` is a fontique collection loaded from bytes only: the built-ins plus imported fonts. It never reads system fonts, so every machine shapes identically.
  - **Families** are named by font asset (`builtin:font/noto-sans`, `builtin:font/noto-sans-mono` or an imported `asset:`). Weight and italic pick a face within the family.
  - **`UiText`:**
    - `runs: Vec<TextRun>`; each run has `text`, plus optional `font`, `size`, `weight`, `italic`, `color` and `underline`, inherited from `UiStyle`;
    - `align: Start | Center | End | Justify` and `valign: Top | Center | Bottom`;
    - `wrap: None | Word | Anywhere`;
    - `overflow: Visible | Clip | Ellipsis`;
    - `line_height` as a multiple of the font's line height;
    - `max_lines`.
  - **Shaping and wrapping** are parley's: harfrust shaping, UAX #14 line breaking, bidi.
  - **The atlas:** swash rasterizes each glyph, keyed by font, glyph id, size in physical pixels and a quarter-pixel subpixel x offset. Glyphs go into an `R8Unorm` atlas with shelf packing, 1,024 × 1,024, growing by pages.
    - The atlas lives in the `UiShared` state outside the world, so it's presentation.
    - A page that changed is uploaded whole (no region copy in the RHI).
  - **Not in M2:** markup, inline images, emoji colour fonts, SDF text (later work, with large world text), and bidi or complex-script tests beyond Latin.
- **Style and themes.**
  - **`UiStyle`** (every field optional, so a theme can fill it):
    - `background`: `Solid(color)`, `Linear { angle_deg, stops }` or `Radial { center, radius, stops }`, with 2–8 `stops` (`offset` in 0..1, ascending, plus a colour);
    - `border { width, color }`;
    - `radius: [tl, tr, br, bl]`;
    - `opacity` in 0..1, multiplying through children;
    - `padding`;
    - the text defaults: `font`, `font_size`, `weight`, `italic`, `color`;
    - `cursor: Arrow | Pointer | Text` (a presentation hint).
    - Colours are sRGB in the file and linear in the draw list, like `Color` elsewhere.
  - **`UiThemed { theme: AssetRef, class: String }`:** a `.theme.ron` maps class names to `{ base: UiStyle, hover, pressed, disabled, focused }`. Each field resolves in this order:
    1. the inline `UiStyle` field;
    2. the class's style for the current state;
    3. the class's base;
    4. the built-in default.
  - **State changes are instant.** `Tweens` (2.4.4) animate any numeric style field (`opacity`, `radius`, colours as arrays), which a test proves. Theme transition times come later.
- **Rendering.**
  - **One `ui` graph pass after `tonemap`:** it loads the output, uses `PremultipliedAlpha`, and has no depth. Order:
    1. world-space roots (2.7b.3), far to near;
    2. screen roots by `z`, then tree order;
    3. the editor and debug overlays, still last.
  - The pass is recorded only when the draw list isn't empty, so frames without UI are unchanged.
  - **`ui.slang`** pulls `UiQuad` instances through bindless, with six vertices per instance and no vertex buffers. Per quad, it evaluates:
    - the rect;
    - the clip rect;
    - a rounded-box SDF for radii and the border;
    - the paint: a solid colour, a linear or radial gradient (stops in a bindless buffer), a texture with UV rect and tint, or the glyph atlas's alpha;
    - an optional `fill` mask for progress bars (`Linear` cut or `Radial { center, start, sweep, direction }`);
    - opacity.
  - **Clipping is per quad in the fragment shader**, through each quad's clip rect. The RHI has no scissor command, and adding one would change the Metal backend; this needs no RHI change and no new `unsafe`.
  - **Batching:** quads are sorted in draw order and merged into one instanced draw per run of the same paint kind and texture.
  - **Images** use the existing `GpuAssets` textures. The glyph atlas uses the new `R8Unorm` mapping.
  - **Tiling and nine-patches** are built on the CPU into exact quads with cut UVs. Exact quads are testable without a GPU, and they work for atlas regions, where a repeat sampler can't.
  - **D7:** a `ui_bounds` debug layer outlines every node's rect (margin, padding and content in three colours), through `debug_layers` like the navmesh.
  - **Goldens** cover each feature (below). They render on macOS. On Linux, the draw-list snapshots and every CPU test run, and goldens skip (ADR-0042).
- **Sound (2.7b.2):** `UiSounds { hover, press, release, toggle, change: Option<AssetRef> }`. `ui.interact` raises `PlayCue` on the widget's entity, so it's unpositioned (a UI entity has no `Transform`). It uses 2.7a as is.
- **Engine API and MCP:**
  - **`ui.inspect { entity?, world, width?, height?, scale? }`** (read only). For a root, or every root:
    - each node's `rect`, `content` rect and `clip`;
    - its resolved style;
    - its state (2.7b.2);
    - its text lines (`text`, baseline, width, height).
    - Agents check layout with numbers instead of images.
  - **`ui.hit_test { x, y, world, width?, height? }`** (read only, 2.7b.2): the topmost interactive widget under a point, and the path to it. QA uses it with `input.inject`.
  - **`capture.screenshot`** gains `ui: bool` (default true) and `ui_scale`. `ui_bounds` is added as a debug layer name.
  - Every role's tool budget is measured again in each sub-phase and recorded in its As built.
- **Studio roles:**
  - **ui-ux is enabled in 2.7b.1** (D8):
    - it gets a new, unique hex colour;
    - paths: `ui/**`, `assets/ui/**`, `assets/fonts/**`, `**/*.theme.ron`, `scenes/**/*.scene.ron` (shared);
    - its tools are its 15, plus `component_get`, `component_patch`, `entity_get`, `schema_get`, `asset_import`, `asset_list`, `ui_inspect` and `capture_screenshot`; 2.7b.2 adds `ui_hit_test` and `input_inject`;
    - a new skill, `concordia-ui`, covering widgets, themes, anchors versus flex, checking with `ui.inspect`, and goldens with `capture.screenshot`;
    - **without an image generator** it builds the look from style primitives (gradients, borders, radii, opacity), nine-patches of art it's given, and the built-in fonts, and it asks the director for art it can't make.
  - **Gameplay programmer (2.7b.2):** wires HUD values, writing `UiProgress.value` and `UiText` runs from processors, and reacts to UI signals. Its skill gains a section.
  - **QA (2.7b.2):** clicks through menus with `ui_hit_test` and `input_inject`, and checks `UiState` and signals. Its skill gains a section.
  - **Editor:** inspectors are generated from schemas, so the new components get them for free once `examples/m1-physics-stack` enables `[ui]` (for `just editor-schemas`). No new panel (D7).
- **Windowed play:**
  - **2.7b.2:** viewport events on open, resize and scale change, and a cursor-icon API in `concordia-platform` driven by the hovered widget's `cursor`.
  - **2.7b.3:** text events, and the clipboard bridge in `Player`. A paste reads the OS clipboard and injects `Insert`; a `ClipboardWrite` signal writes it.
  - **Drawing:** the window draws the UI at the window's scale factor, so text rasterizes at physical size.
  - **Without a GPU:** layout, text metrics, interaction, hashes, replays, draw-list snapshots and `ui.inspect` all run headless, and on Linux, without one.
- **The components** (each gets a test of its fields, defaults and validation, a layout or draw test, and a schema snapshot). From the old plan: `UiNode`, `UiStyle`, `UiImage`, `UiText`, `UiProgress`, `UiNinePatch` and `UiButton` are kept under their names. Additions: `UiRoot`, `UiThemed`, `UiSeparator`, `UiState`, `UiToggle`, `UiSlider`, `UiDropdown`, `UiTabs`, `UiTooltip`, `UiScroll`, `UiFocus`, `UiSounds`, `UiTextBox`, `UiSpinBox` and `UiList`. Nothing is dropped or renamed. `UiToggle` merges checkbox, switch and toggle (U9). Layout containers aren't separate components; they're `UiNode.layout` modes.

  **2.7b.1 (display):**
  - **`UiRoot`** `{ canvas: [f32; 2] = [1280, 720], stretch: Disabled | Scale = Disabled, aspect: Ignore | Keep | KeepWidth | KeepHeight | Expand = Keep, space: Screen | World { offset, pixel_size } = Screen, z: i32 = 0 }`.
    - Validated: a finite, positive canvas no larger than 16,384, and a positive `pixel_size`.
    - Its subtree is the entity's `Parent` children (scene hierarchy).
  - **`UiNode`** `{ width, height: Auto | Px(f32) | Percent(f32) = Auto, min_*, max_*, margin, padding: Edges = 0, gap: [f32; 2] = 0, layout: Flex { direction = Row, wrap = false, justify = Start, align_items = Stretch, align_content = Start } | Grid { columns, rows: Vec<Track> } | Stack = Flex, position: InFlow | Anchored { anchors: [l, t, r, b] = 0, offsets = 0, grow_h, grow_v = End } = InFlow, flex_grow = 0, flex_shrink = 1, visible = true, clip = false, z = 0 }`.
    - Validated: finite values; `Px` ≥ 0; `Percent` in 0..100; min ≤ max; anchors in 0..1 with left ≤ right and top ≤ bottom; at most 64 grid tracks.
  - **`UiStyle`:** as above. Validated: 2–8 ascending stops in 0..1; opacity in 0..1; border width ≥ 0; radii ≥ 0; size in 1..1,024.
  - **`UiThemed`** `{ theme: AssetRef, class: String }`. Validated: a non-empty class. A missing class warns once and falls back to defaults.
  - **`UiText`:** as above. Validated: total text ≤ 64 KiB; `line_height` in 0.5..4; `max_lines` ≥ 1; run sizes in 1..1,024.
  - **`UiImage`** `{ image: AssetRef, region: Option<[u32; 4]>, mode: Scale | Tile | Keep | KeepCentered | KeepAspect | KeepAspectCentered | KeepAspectCovered = Scale, tint = white, flip_h, flip_v = false }`.
    - Godot's `TextureRect` stretch modes, for semantics.
    - Validated: the region lies inside the texture (checked when drawing; a bad region draws nothing and warns once), and at most 4,096 tiles.
  - **`UiNinePatch`** `{ image, region, margins: [l, t, r, b] (texture px), axis_h, axis_v: Stretch | Tile | TileFit = Stretch, draw_center = true, tint }`.
    - Godot's `NinePatchRect` for semantics.
    - When the rect is smaller than the margins, they shrink by the CSS `border-image` rule *f* = min(1, rect / (margin_a + margin_b)).
    - Validated: margins fit the region.
  - **`UiProgress`** `{ value = 0, min = 0, max = 100, fill: LeftToRight | RightToLeft | TopToBottom | BottomToTop | Clockwise | CounterClockwise | BilinearH | BilinearV | ClockwiseAndCounter = LeftToRight, radial: { start_deg = 0, sweep_deg = 360, center = [0.5, 0.5] }, under, progress, over: Option<UiImage-like ImageRef>, under_style, progress_style: Option<UiStyle> }`.
    - Godot's `TextureProgressBar` and `ProgressBar` for semantics.
    - The fill fraction is clamp((value − min)/(max − min), 0, 1).
    - With textures it draws under, then progress through the fill mask, then over. Without textures it draws two styled boxes.
    - Validated: min < max, all finite, sweep in (0, 360].
  - **`UiSeparator`** `{ orientation: Horizontal | Vertical = Horizontal, thickness = 1, color }`. It draws a line centred in its rect.

  **2.7b.2 (interaction):**
  - **`UiState`** `{ hovered, pressed, focused, disabled: bool }`. The engine writes it, except `disabled`. Hashed.
  - **`UiButton`** `{ disabled = false, action_on: Release | Press = Release }`.
    - `UiPressed` is raised on release over the same widget it was pressed on (the WAI-ARIA button pattern), or on press when `action_on: Press`.
  - **`UiToggle`** `{ on = false, look: Checkbox | Switch | Button = Checkbox, group: Option<String>, label_gap = 6 }`.
    - In a group, turning one on turns the others off. Clicking the one that's on leaves it on (radio), unless the group's `allow_none`.
    - Raises `UiToggled`.
  - **`UiSlider`** `{ value = 0, min = 0, max = 1, step = 0 (continuous), orientation = Horizontal, page = 0.1 }`.
    - Drag or click sets it; arrow keys move one step (or 1 % when continuous); Page keys move `page`.
    - The value is snapped as min + round((v − min)/step)·step, then clamped.
    - Raises `UiValueChanged`.
  - **`UiDropdown`** `{ options: Vec<String>, selected: Option<u32>, open = false, max_visible = 8 }`.
    - The popup draws as a top-layer list under the widget; clicking outside closes it.
    - Raises `UiSelected`.
  - **`UiTabs`** `{ selected = 0, titles: Vec<String> }`. The children are pages; only the selected page is laid out and drawn. Raises `UiSelected`.
  - **`UiTooltip`** `{ text: String, delay_ticks = 30 }`. It shows after the widget has been hovered for `delay_ticks` (a hashed counter in `UiState`), at the pointer, and kept on screen.
  - **`UiScroll`** `{ offset: [f32; 2], horizontal, vertical: Auto | Always | Never = Auto, step = 40, bar_width = 8 }`.
    - The node clips its content, and the offset is clamped to the content extent minus the viewport.
    - Scrolled by the wheel (logical pixels or lines × `step`) and by dragging its bars.
  - **`UiFocus`** `{ order: Option<i32>, up, down, left, right, next, prev: Option<EntityId> }`.
    - Tab goes in `order`, then tree order. Arrows go to the explicit neighbour, else to the nearest widget whose centre lies in that direction (the smallest distance along the direction plus twice the cross distance).
    - The actions are `ui_up`, `ui_down`, `ui_left`, `ui_right`, `ui_accept`, `ui_cancel`, `ui_next` and `ui_prev`, added to `InputMap::starter`.
  - **`UiSounds`:** as above.

  **2.7b.3 (text and world):**
  - **`UiTextBox`** `{ text, placeholder, multiline = false, max_len: Option<u32>, password = false, editable = true, caret = 0, anchor = 0 (byte offsets on char boundaries), wrap = Word }`.
    - Edit keys: Left, Right, Up, Down, Home and End, with Shift to select; Backspace, Delete, select all, copy, cut, paste; Enter submits (`UiTextSubmitted`), or adds a newline when multi-line.
    - Raises `UiTextChanged`.
    - Password masking replaces the glyphs when drawing only. The hashed text stays real, because gameplay reads it.
  - **`UiSpinBox`** `{ value, min, max, step = 1, decimals = 0 }`: a text box plus two buttons, built as child entities by the module when it is added (so they're in the scene, editable and hashed).
  - **`UiList`** `{ selected: Vec<u32>, multi = false }`. The children are items; click selects, Ctrl adds and Shift selects a range. Raises `UiSelected`.
  - **World-space roots** (`UiRoot.space: World { offset, pixel_size }`):
    - the root's screen position is its entity's world position (`world_matrix_with`, so parents count) plus `offset`, projected through the view (`View::project`);
    - it's laid out in UI pixels around that point;
    - it's hidden when behind the eye or entirely off screen (U11);
    - damage numbers are a world root with a `UiText`, animated by `Tweens` on `offset` and `opacity`;
    - floating bars are a world root with a `UiProgress`.
- **Sub-phases:**
  - **2.7b.1 `m2/2.7b-display`:**
    - the crate;
    - the 2.7b.1 components;
    - themes and the `theme` importer;
    - fonts and the `font` importer;
    - layout and stretch;
    - text;
    - the draw list and its seam;
    - the `ui` pass and shader;
    - `ui.inspect`;
    - the `ui_bounds` layer;
    - ui-ux enabled;
    - ADR-0060.
  - **2.7b.2 `m2/2.7b-interact`:**
    - the viewport input and the replay update;
    - `ConsumedActions`;
    - interaction, focus and the 2.7b.2 components;
    - scrolling and clipping by content;
    - sounds;
    - `ui.hit_test`;
    - cursor icons;
    - the QA and gameplay skill sections.
  - **2.7b.3 `m2/2.7b-text-world`:**
    - text input and its replay track;
    - `UiTextBox`, `UiSpinBox` and `UiList`;
    - arboard and BSL-1.0;
    - world-space roots.
- **Tests first** (expected values derived by hand in each test's header; goldens per feature; insta snapshots of schemas and draw lists).

  **2.7b.1:**
  - **Layout:**
    - `flex_row_places_children_with_gap_and_padding`;
    - `flex_column_justify_and_align`;
    - `grid_places_cells_in_explicit_tracks`;
    - `stack_overlaps_children_in_content_box`.
  - **Anchors and stretch:**
    - `anchor_presets_and_offsets_match_godot_rules` (all 16 presets on a 400 × 300 parent);
    - `anchored_child_grows_by_grow_direction`;
    - `canvas_stretch_for_each_aspect_mode` (a 2:1 base into 16:9 and 4:3 viewports, every mode).
  - **Text:**
    - `text_metrics_come_from_the_font_tables` (advance sum = Σ `hmtx` advance × size / `unitsPerEm`; line height from `hhea` ascender − descender + lineGap, read from the font by a test helper);
    - `text_wraps_at_word_boundaries_and_aligns`;
    - `styled_runs_change_font_and_color_per_run`;
    - `text_overflow_ellipsis_truncates_last_line`;
    - `text_node_measures_intrinsic_size_in_flex`;
    - `glyph_atlas_packs_and_reuses_glyphs`.
  - **Paint:**
    - `progress_fill_fraction_and_quad_for_each_mode`;
    - `nine_patch_slices_into_nine_quads_with_expected_uvs`;
    - `nine_patch_margins_shrink_by_border_image_rule`;
    - `image_modes_produce_expected_quads` (all seven);
    - `image_tile_cuts_last_row_and_column`;
    - `separator_draws_centred_line`;
    - `gradient_stops_and_colors_reach_the_draw_list_linear`.
  - **Themes:**
    - `theme_resolves_inline_then_state_then_base_then_default`;
    - `theme_file_reads_validates_and_imports`.
  - **Validation and scenes:**
    - `ui_components_validate_and_reject_bad_input` (one row per rule above, each naming its field);
    - `ui_scene_round_trips_through_scene_ron`;
    - `ui_edits_go_through_commands_with_actor`.
  - **Animation:** `tweens_animate_ui_style_opacity`.
  - **Determinism:** `ui_never_changes_the_world_hash_at_1_and_8_threads`.
  - **Fonts:** `font_importer_imports_ttf_into_the_ddc` and `builtin_fonts_resolve_by_name`.
  - **Draw lists and goldens:**
    - `draw_list_snapshot_for_the_hud_fixture` (insta; runs on Linux);
    - `ui_draw_hook_reaches_render_request`;
    - goldens `ui_hud`, `ui_hud_sequence_{0,1,2}` (a bar value written over three ticks), `ui_gradients_borders_radii`, `ui_nine_patch`, `ui_text_runs`, `ui_progress_modes`, `ui_image_modes` and `ui_clip`.
  - **Tools and roles:**
    - `ui_inspect_reports_rects_and_text_lines` and its schema snapshot;
    - `capture_screenshot_without_ui_omits_it`;
    - `ui_bounds_layer_outlines_nodes`;
    - the ui-ux role tests (scope, colour, skill and budget), with every role's numbers.

  **2.7b.2:**
  - `pointer_over_ui_consumes_click_before_world` (click-to-move from 2.5b.2 doesn't move);
  - `click_outside_ui_still_moves`;
  - `button_fires_on_release_inside_only`;
  - `button_press_then_drag_off_does_not_fire`;
  - `disabled_button_ignores_input`;
  - `hover_and_press_state_follow_the_pointer`;
  - `theme_state_style_follows_ui_state`;
  - `toggle_flips_and_signals`;
  - `radio_group_keeps_exactly_one_on`;
  - `slider_drag_and_keys_snap_to_step`;
  - `dropdown_opens_selects_and_closes`;
  - `tabs_lay_out_only_the_selected_page`;
  - `tooltip_shows_after_delay_ticks`;
  - `scroll_offset_clamps_to_content` and `scroll_clips_children`;
  - `wheel_and_bar_drag_scroll`;
  - `focus_moves_by_tab_order_and_tree_order`;
  - `focus_moves_by_arrows_to_nearest_or_explicit_neighbour`;
  - `accept_presses_the_focused_button`;
  - `ui_sounds_raise_play_cue_on_the_widget`;
  - `viewport_input_reflows_layout_and_is_replayed`;
  - `ui_hit_test_returns_topmost_widget_path`;
  - `replay_with_clicks_drags_and_resize_is_exact_at_1_and_8_threads`;
  - goldens `ui_button_states`, `ui_toggle_looks`, `ui_slider`, `ui_dropdown_open`, `ui_tabs` and `ui_scroll`.

  **2.7b.3:**
  - `text_box_inserts_and_edits_at_the_caret`;
  - `text_box_selection_by_shift_and_select_all`;
  - `text_box_copy_cut_paste_through_the_clipboard`;
  - `text_box_enforces_max_len_on_insert_and_paste`;
  - `text_box_enter_submits_or_breaks_line`;
  - `password_masks_drawing_only`;
  - `caret_stays_on_char_boundaries`;
  - `spin_box_builds_children_and_steps`;
  - `list_selects_single_ctrl_and_shift_ranges`;
  - `text_input_is_recorded_and_replayed`;
  - `replay_with_typing_is_exact_at_1_and_8_threads`;
  - `world_root_follows_its_parented_entity`;
  - `world_root_hidden_behind_the_camera_or_off_screen`;
  - `world_roots_sort_far_to_near`;
  - goldens `ui_text_box_caret_selection`, `ui_list`, `world_text_damage_number`, `world_floating_bar`;
  - `arboard_clipboard_round_trips` (`#[ignore]`, manual on a desktop).
- **Done criteria (whole phase):**
  - all tests green;
  - a HUD fixture driven by a component value updates its bar and label in a golden sequence;
  - a settings-menu fixture (tabs, toggles, slider, dropdown, text box) is driven headless through `input.inject`, and its replay is exact at 1 and 8 threads;
  - world text follows a moving entity in a golden;
  - ui-ux is enabled with its skill;
  - every role is under 50k.

**Not in 2.7b:** IME; gamepad devices; depth-occluded world UI; live camera views in UI; markup; SDF text; shadows and blur; theme transition times; list virtualization; drag and drop; tree views; Godot `Control` import (M5).

### Phase 2.7b.1 As built (2026-11-23, ADR-0060)
**Status: done (branch `m2/2.7b-display`). The display half of the in-game UI: the components, layout, text, themes, the draw list and its renderer pass, `ui.inspect`, the `ui_bounds` layer, and the UI/UX role.**

**What landed:**
- **New crate `concordia-ui`** (`#![forbid(unsafe_code)]`, no GPU code; taffy 0.15.1, parley 0.13.0, swash 0.2.10):
  - **Components:** `UiRoot`, `UiNode`, `UiStyle`, `UiThemed`, `UiText`, `UiImage`, `UiNinePatch`, `UiProgress` and `UiSeparator`, each with a `check()` whose error names its field. Single-field bounds are also schema `range`s, so the API refuses them at the boundary (`ValueInvalid` with the pointer).
  - **Gathering:** `UiScene::gather` reads every root's `Parent` subtree. Invalid components are dropped with a reported problem, and the node keeps a default `UiNode`. Hidden subtrees are skipped. Siblings are ordered by (`order`, id).
  - **Layout:** `compute_layout` resolves styles top-down (themes, then inheritance of the text fields). It then lays out each in-flow subtree with taffy, with rounding off; `Stack` is a 1 × 1 grid. Each anchored node is laid out as its own island inside its parent's rect (Godot's anchor, offset and grow rules; the 16 presets as `Anchored::preset`). Finally it shapes text at its final width and records clips, opacity and the draw order (`z`, `order`, id). Roots map their canvas with Godot's `canvas_items` stretch and aspects (`canvas_map`).
  - **Text:** `UiFonts` is a parley font context with the built-in Noto faces loaded from bytes only; there are no system fonts. `shape` handles styled runs, word, anywhere and no wrapping, alignment (parley's), line height as a multiple of the font's, `max_lines` and a height limit, clip, and ellipsis (the longest prefix that still fits with "…").
  - **The glyph atlas:** `GlyphAtlas` rasterizes with swash (unhinted, quarter-pixel x offsets) into shelf-packed `R8` pages of 1,024 px. A page's generation is bumped on every write.
  - **Paint geometry** (pure, in UI space):
    - the seven `TextureRect` stretch modes, with tiles cut at the edges;
    - nine-patches with CSS `border-image` shrinking and Stretch, Tile or TileFit (CSS `round`);
    - the nine progress fill modes; the three radial modes become an angular mask;
    - separators;
    - CSS-angle linear and radial gradients.
  - **`draw_list`** maps UI space to target pixels (the root's map, then the frame's scale), dedupes image textures and adds atlas pages. Nine-patch and region UVs are pulled in half a texel so filtering never samples the neighbouring slice; a golden showed the bleed.
  - **Importers:** `font` (ttf and otf → `AssetKind::Font`; the family comes from the font's `name` table) and `theme` (`.theme.ron` → `AssetKind::Theme`, validated, errors as `classes/<name>/<state>/<field>`).
  - **The engine module:** `UiModule` (`[ui]`) registers the types, holds `UiShared` (fonts, atlas, asset cache) as engine state, serves `ui.inspect`, lists the `ui_bounds` layer and answers the draw hook.
- **`concordia-api`:**
  - the draw-list types (`UiDrawList`, `UiQuad`, `UiPaint`, `UiMask`, `UiStop`, `UiTexture`, `UiTextureFormat`, `UiFrame`) and `UI_BOUNDS_LAYER`;
  - the `EngineModule::ui_draw` hook, and `Engine::ui_draw`, which gathers every module's list in module order.
  - The renderer and the UI never depend on each other.
- **`concordia-assets`:** `TextureFormat::R8`; `AssetKind::Font` and `Theme`.
- **`concordia-render`:**
  - `RenderRequest.ui`.
  - `ui.slang` pulls quads (40 floats each, `encode_ui`) and stops (8 floats) through bindless. Per quad it draws a rounded-box SDF with antialiasing and a border, a solid, linear, radial, texture or glyph paint, the angular mask, and a clip in the fragment shader; output is premultiplied.
  - The UI is drawn **inside the tonemap pass**, after the fullscreen triangle and before the editor overlays. There is no separate graph pass (the refinement had one), so no extra render pass and no load.
  - `UiTextures` caches images and atlas pages by (key, generation); a replaced texture retires with the frame.
  - `capture.screenshot` gains `ui` (default true) and `ui_scale` (0.25–8). `viewport.present` gains `scale`, which is kept for after-tick and navigation redraws. The `ui_bounds` layer outlines every node.
- **`concordia-app`:**
  - `[ui]` adds the module, and every project registers the `font` and `theme` importers.
  - `Player` passes the window's scale factor to `viewport.present`.
- **`concordia-cli`:** `concordia new` writes `[ui]`, and `studio init` adds it when it is missing.
- **Studio (D8):**
  - The UI/UX designer is enabled: colour `#0ea5e9`; writes `ui/**`, `assets/ui/**`, `assets/fonts/**` and scene files; 24 tools, about 17.3k tokens.
  - New skill `concordia-ui`. Its example calls, components and theme are parsed as the real types.
  - Every enabled role's numbers on 2026-11-23:

    | Role | Tools | Tokens |
    |---|---|---|
    | level designer | 37 | 23,144 |
    | QA | 24 | 21,585 |
    | tech artist | 23 | 20,172 |
    | UI/UX | 24 | 17,271 |
    | animator | 22 | 16,890 |
    | sound designer | 24 | 15,138 |
    | composer | 20 | 13,054 |
    | gameplay programmer | 17 | 12,946 |
    | director | 13 | 12,453 |
    | writer | 11 | 9,576 |

    `capture_screenshot` grew by about 66 tokens for each role that has it. All are under 50k.
- **The editor:** `examples/m1-physics-stack` enables `[ui]`, so `just editor-schemas` generates the UI components' schemas and the inspectors follow.
- **Fonts:** Noto Sans v2.015 (four faces) and Noto Sans Mono v2.014, unhinted. They live in `crates/concordia-ui/fonts/` with `OFL.txt` and a `VENDOR.md` recording the upstream release and each file's SHA-256, and are listed in `third_party/README.md`. Following that convention, the refinement's `licenses/fonts.md` wasn't created.

**Differences from the refinement (this As built wins):**
- The UI is drawn in the tonemap render pass rather than its own graph pass.
- Built-in font names use underscores: `builtin:font/noto_sans`, `builtin:font/noto_sans_mono`. Asset names are `[a-z0-9_]`.
- Colours are linear RGBA with the `color` widget, as materials are. The refinement said sRGB in the file.
- `ui.inspect` takes `world`, `entity`, `width` and `height`, with no `scale`. The viewport is in logical pixels and defaults to the first root's canvas.
- The UI goldens live in `concordia-ui`'s tests (suite `ui`, `tests/golden/ui/`), with the renderer as a dev-dependency, so they share the HUD fixture.
- **Dependencies:** parley pulls two versions of `skrifa` / `read-fonts`, and swash pulls a third. These are duplicate warnings only (`multiple-versions = "warn"`).

**Tests** (expected values derived in each header; anything that passed at once or was written after its code is called out below):
- **`concordia-ui` (59 tests):**
  - `validate` (4);
  - `anchors` (3), `stretch` (2), `theme` (4);
  - `text` (7): metrics read by a hand-written sfnt reader (`head`, `hhea`, `OS/2`, `cmap` format 4, `hmtx`);
  - `layout` (8), `paint` (8), `atlas` (2);
  - `draw` (8), including the HUD fixture's draw-list snapshot;
  - `engine` (6): the module, an actor on every edit, the scene round trip, `ui.inspect`, tweens on `UiStyle /opacity`, and hashes identical with and without UI drawing at 1 and 8 threads;
  - `golden` (9): `ui_hud`, `ui_hud_sequence_{0,1,2}` (20/50/90 %), `ui_gradients_borders_radii`, `ui_nine_patch`, `ui_text_runs`, `ui_progress_modes`, `ui_image_modes`, `ui_clip`, plus `capture_screenshot_draws_the_ui_unless_asked_not_to`.
- **`concordia-render` `tests/ui.rs` (2):** encoding, and the push layout against `ui.slang` for MSL and SPIR-V.
- **`concordia-app`:**
  - `tests/ui.rs`: an imported font and a theme reach `ui.inspect`; four Mono "M"s at 20 px are 48 px;
  - `player`: presents carry the scale.
- **`concordia-cli`:** `new` writes `[ui]`; `studio init` adds it.
- **`concordia-studio`:** the ui-ux role and skill tests, the agent file list, and the director's spawn list.
- **`concordia-mcp`:** `ui_ux_is_enabled_with_ui_tools_under_budget` and `server_holds_the_ui_designer_to_its_paths`.
- **Called out:**
  - **Written before their tests, then shown failing on a stub and passing restored (the mutation check):** `UiNode::check`, `UiImage::check`, `UiNinePatch::check` and `half_texel_inset`. The last was fixed after a golden showed the bleed.
  - **Passed at once:**
    - `ui_module_registers_its_method_and_debug_layer` (the module was written just before it);
    - `ui_scene_round_trips_through_scene_ron` (scenes are generic);
    - `tweens_animate_ui_style_opacity` (2.4.4's tweens already cover any numeric field);
    - `ui_push_matches_ui_slang` (a layout guard written with its struct);
    - `ui_ux_is_enabled_with_ui_tools_under_budget` and `server_holds_the_ui_designer_to_its_paths` (written after the role was enabled).
  - **Wrong expectations, fixed on purpose:**
    - Noto Sans Mono's `advanceWidthMax` (1,800) isn't its Latin advance (600); the test now reads `M`'s advance through `cmap` and `hmtx`.
    - A draw-order row; two error kinds (`ValueInvalid` for component values, `InvalidParams` for method params).
    - The region UV test, after the half-texel inset.

**Checklist (2026-11-23):**
- `just lint` and the Tauri shell's clippy: clean.
- `just wasm`, `just native-fixtures`, then `cargo nextest run --workspace --no-fail-fast` with `CONCORDIA_REQUIRE_GPU=1`: 1,682 run, 1,667 passed.
  - The 15 failures were the snapshots and goldens awaiting review, plus `m1_example_modules_follow_core_in_manifest_order`, since fixed (the example now has `[ui]`).
  - Doc tests: clean.
- `just gate-tests M1` and `M3S`: green apart from their pending snapshots (4/4 and 13/13 with snapshots forced).
- `just deny` and the editor's deny (fresh advisory DB): advisories, bans, licenses and sources ok. Only duplicate-version warnings, and the editor's carried-over "advisory not detected".
- `just editor-schemas`: generated the UI types plus `ui.inspect`, `capture.screenshot` and `viewport.present` param changes, committed.
- `just editor-test`: lint, unit and 22 end-to-end tests pass.
- After the author approved and they were blessed: ui, studio, render, app, mcp and cli rerun with 610 run and 610 passed; `just gate-tests M1` passes 4/4 and `M3S` passes 13/13, with nothing forced.

**Snapshots and goldens** (collected with `INSTA_UPDATE=new`; goldens written to `target/golden-candidates/ui/`; shown on the review page; blessed 2026-11-23 after the author approved it, with `assertion_line` stripped):
- new: the HUD draw list, `agent_ui-ux` and the ten UI goldens;
- changed: `agent_director`, the `CLAUDE.md` block, the M3S gate's agents/skills and `CLAUDE.md` block, and the `capture.screenshot` input schema (M1 gate).

**Carry-over (unchanged, still open):**
- the ignored `run_bench_latency_measures_the_e…` test;
- the navmesh layer's re-gather;
- the editor's deny warning;
- ADR-0069's chunk components (M7 7.7);
- physics queries seeing last-step bodies;
- the crowd's sequential steps at 10k (M7 7.7);
- `concordia run` with sound (and now UI) not yet exercised by hand (2.10's sample game).

### Phase 2.7b.2 As built (2026-11-23, ADR-0060)
**Status: done (branch `m2/2.7b-interact`). The interactive half of the in-game UI: the controls, `ui.interact` inside the simulation, focus, scrolling, tooltips, sounds, pointer consumption, `ui.hit_test`, cursor icons, viewport input, and the role and skill updates.**

**What landed:**
- **`concordia-ui`:**
  - **Components:** `UiState` (hovered, pressed, focused, `disabled`, hover ticks, the highlighted row, a thumb `Grab`, the tooltip point), `UiButton` (`ActionOn::Release | Press`), `UiToggle` (`Checkbox | Switch | Button`, `group`, `allow_none`, `label_gap`), `UiSlider` (`snap`: min + round((v − min)/step)·step, clamped), `UiDropdown`, `UiTabs`, `UiTooltip`, `UiScroll` (`ScrollBar::Auto | Always | Never`), `UiFocus`, `UiSounds`, and `UiNode.pointer` (`PointerFilter::Auto | Stop | Ignore`). Signals: `UiPressed`, `UiToggled`, `UiValueChanged`, `UiSelected`, `UiFocusChanged`. Each has a `check()` naming its field.
  - **Parts:** layout records each control's sub-rects once (`UiLayout::parts`: toggle box, slider track and grabber, dropdown face, arrow and open list, tab titles, scroll offset, content and bars; `UiLayout::tips`), from pure geometry in `parts.rs`. Drawing and hit tests both read them. Toggles add their box as left padding, tabs their title row as top padding; only the selected page is laid out. A scroll lays its content out as its own island, unbounded along its scrolling axes and at least its own size, shifted by the clamped offset and clipped.
  - **State styles:** a node's theme state is the first that holds of disabled, pressed (or an on `Button` toggle), hovered, focused.
  - **`ui.interact`** (exclusive, `PrePhysics`, `after_if_present("input.sample")`, `before_if_present("nav.click")`): reads the `ui_*` actions (`UiInput::read`), lays the play world out for the `ui_viewport` action's size (else the first root's canvas) with its own fonts and an asset snapshot taken at play start (`preload`), then runs one tick (`interact`): hover and hover ticks, tooltips, press (focuses; dropdowns open and pick on press, tabs select on press, scroll thumbs grab), drags (sliders, thumbs), release (buttons fire and toggles flip over the widget they were pressed on), the wheel (the innermost scroll, `step` per line), and keys (Tab / Shift-Tab order, arrows to an explicit neighbour or the nearest by distance along + 2 × across, Accept, Cancel, Page keys; sliders, tabs and open dropdowns use their own keys). Changed values and `UiState` are written back; signals and `PlayCue` (from `UiSounds`) are raised on the widget. The pointer is consumed over anything that `stops` it, after a press that closed a list, and while a press is held.
  - **Drawing:** toggle boxes and switches, slider track, fill and grabber, the dropdown face and its arrow (a quad masked to a wedge about its bottom middle; Noto Sans has no arrow glyph), tab titles with the selected one underlined, focus rings (`FOCUS_COLOR`, 2 px outside the box), scroll bars over their root's nodes, and open lists and tooltips above every root.
  - **`ui.hit_test`** `{x, y, world?, width?, height?}` → node, widget, part, path, point and cursor. `ui.inspect` reports each node's `state`; both default to the play input's viewport.
- **`concordia-input`:** `InputEvent::Viewport` and `Binding::Viewport` (Axis2d, logical pixels); `ConsumedActions` (which actions read the pointer; `consume_pointer`, cleared by `input.sample`) and `ActionStates::unconsumed`; the starter map gains `ui_pointer`, `ui_click`, `ui_scroll`, `ui_viewport`, `ui_up`, `ui_down`, `ui_left`, `ui_right`, `ui_accept` (Enter, Space), `ui_cancel`, `ui_next` (Tab), `ui_prev` (Shift-Tab), `ui_page_up` and `ui_page_down`; `KeyCode` gains `PageUp`, `PageDown`, `Home` and `End`.
- **`concordia-terra`:** exclusive systems can order themselves around nodes that may be absent (`FnSystem::after_if_present` / `before_if_present`).
- **`concordia-nav`:** `nav.click` reads the click through `unconsumed`.
- **`concordia-platform`:** `CursorIcon` and `WindowApp::cursor`, applied by the runner after every event; the new keys map from winit.
- **`concordia-app`:** the player sends the window's logical size as `Viewport` input on open, resize and scale change, and asks `ui.hit_test` for the cursor on pointer moves (a project without `[ui]` is asked once). `concordia run`'s viewer shows it.
- **Studio (D8):** the UI/UX designer gains `ui_hit_test` and `input_inject`; QA gains `ui_hit_test`. `concordia-ui` gains "Controls"; `concordia-scripting` gains "Driving the UI"; `concordia-record-replay` gains "Clicking through menus". Every enabled role's numbers on 2026-11-23:

  | Role | Tools | Tokens |
  |---|---|---|
  | level designer | 37 | 23,144 |
  | QA | 25 | 22,752 |
  | UI/UX | 26 | 20,916 |
  | tech artist | 23 | 20,172 |
  | animator | 22 | 16,890 |
  | sound designer | 24 | 15,138 |
  | composer | 20 | 13,054 |
  | gameplay programmer | 17 | 12,946 |
  | director | 13 | 12,453 |
  | writer | 11 | 9,576 |

  All are under 50k.
- **Docs:** ADR-0060 gains "2.7b.2 notes"; ADR-0072 a note on the viewport; the agent guide documents the controls, input, consumption and `ui.hit_test`.

**Differences from the refinement (this As built wins):**
- **The viewport is an action, not a new replay track.** `InputEvent::Viewport` feeds `Binding::Viewport`; replays record it like any action value, so the replay format stays version 0 (no bump), following ADR-0072's rule that raw positions reach the simulation only through actions. It carries no scale (hit tests need only logical pixels).
- **`disabled` lives on `UiState`**, not on `UiButton`: one field for every control, and the only `UiState` field authors set.
- **`UiState.grab` is a `Grab { bar, at }`**, not a point: it must say which bar was grabbed.
- **`UiScroll.horizontal` defaults to `Never`** (vertical `Auto`): a scroll whose content may also grow sideways never wraps its text; set `Auto` to allow it.
- **Ordering:** exclusive systems couldn't declare order, so Terra gained the "if present" constraints; nav doesn't depend on the UI.
- **Dropdown rows pick on press** (Godot's popup picks on release); a drag-select can come later.
- **Assets in the simulation** are a snapshot taken when play starts; an asset first named by a widget spawned later measures as missing there, the same in every run (carry-over).
- **Not built:** wheel scrolling of an open dropdown list, track clicks that page a scroll, theme transition times (as planned), and gamepad focus (later, U10).

**Tests** (expected values derived in each header; anything that passed at once, or was written after its code, is called out below):
- **`concordia-terra`:** `exclusive_systems_order_around_present_nodes_and_ignore_absent_ones`.
- **`concordia-input` `tests/ui_input.rs` (6):** the viewport event and binding, its checks, consumption hiding only pointer-bound actions, the starter map's 16 actions, the new keys.
- **`concordia-platform`:** the new keys and the cursor icons map to winit.
- **`concordia-ui`:**
  - `validate`: `interactive_component_rules_name_their_field`, `interactive_component_defaults`;
  - `controls_layout` (7): state styles, toggle boxes, slider parts, dropdown lists (and flipping above), tabs laying out only the selected page, scroll clamping and clipping, tooltip placement;
  - `controls_draw` (6): checkbox and switch, slider quads, the masked arrow, the focus ring, draw order (bars over content, lists and tips over everything), and the controls fixture's draw-list snapshot;
  - `interact` (19): buttons fire on release inside only, drag-off doesn't fire, disabled ignores input, hover and press state, toggles, radio groups, slider drags and keys, dropdowns, tabs, tooltips, wheel and bar drags, Tab order, arrows (explicit and nearest), Accept, sounds, viewport reflow, consumption, `ui.hit_test` and inspect state;
  - `engine`: the module now lists `ui.hit_test`;
  - `golden` (7 new): `ui_button_states`, `ui_toggle_looks`, `ui_slider`, `ui_dropdown_open`, `ui_tabs`, `ui_scroll`, `ui_tooltip`.
- **`concordia-app`:** `ui_click` (a HUD click never moves the hero; one elsewhere does), `ui_replay` (clicks, a drag, the wheel, Tab and a resize replay exactly, at 1 and 8 threads), `player` (viewport input on open, resize and scale change; the cursor from `ui.hit_test`).
- **`concordia-studio`:** the UI/UX and QA role tests, `gameplay_and_qa_skills_cover_the_ui`, and the `concordia-ui` example test extended to every control and both tools.
- **Mutation checks** (each mutant failed a test; restored files touched): release firing anywhere, the arrow score's ×2, the radio staying on, pressed beating hover, `ConsumedActions` not cleared, a held press not consuming, the scroll clamp, Terra's "after" edges.
- **Called out:**
  - **Passed at once:** `dropdown_row_height_comes_from_the_font` (a check of the header's arithmetic); `click_outside_ui_still_moves` (a regression guard: clicks already moved the hero).
  - **Written after its code:** `replay_with_clicks_drags_and_resize_is_exact_at_1_and_8_threads` (it passed once its project had `[replay]`).
  - **Changed on purpose before passing:** the arrow-focus fixture moved D to (100, 150) so the cross weight of 2 decides a move (with the first layout a weight of 1 gave the same answers); `UiState.grab` became a `Grab` in two tests; `ui_module_registers_its_method_and_debug_layer` now lists two methods; the player's call list gained the `Viewport` injections.

**Checklist (2026-11-23):**
- `just lint` and the Tauri shell's clippy: clean.
- `just wasm`, `just native-fixtures`, then `cargo nextest run --workspace --no-fail-fast` with `CONCORDIA_REQUIRE_GPU=1`: 1,738 run, 1,726 passed; the 12 failures are the snapshots and goldens awaiting review (below). Doc tests: clean.
- `just gate-tests M1` (4/4) and `M3S` (13/13, its pending snapshot forced).
- `just deny` and the editor's deny (fresh advisory DB): advisories, bans, licenses and sources ok.
- `just editor-schemas`: the controls, `ui.hit_test`, `UiNode.pointer`, the inspect report's `state` and `input.inject`'s `Viewport` and keys, committed.
- `just editor-test`: lint, 118 unit and 22 end-to-end tests pass.
- After blessing: ui, input, studio, app, mcp, nav, platform and terra rerun with 602 run and 602 passed; `just gate-tests M1` passes 4/4 and `M3S` 13/13, with nothing forced.

**Snapshots and goldens** (collected with `INSTA_UPDATE=new`; goldens written to `target/golden-candidates/ui/`; shown on the review page; blessed 2026-11-23 after the author approved it, with `assertion_line` stripped):
- new: the controls draw list and the seven control goldens;
- changed: `agent_ui-ux`, `agent_qa-playtester`, the `CLAUDE.md` block (Studio and the M3S gate), and the `input.inject` input schema.

**Carry-over:**
- assets first named by a widget spawned during play measure as missing in the simulation;
- trackpad pixel scrolling arrives as lines (`Scroll` doesn't say which), so the UI multiplies it by `step`;
- the ignored `run_bench_latency_measures_the_e…` test; the navmesh layer's re-gather; the editor's deny warning; ADR-0069's chunk components (M7 7.7); physics queries seeing last-step bodies; the crowd's sequential steps at 10k (M7 7.7);
- `concordia run` with sound and UI not yet exercised by hand (2.10's sample game).

### Phase 2.7b.3 As built (2026-11-23, ADR-0060)
**Status: done (branch `m2/2.7b-text-world`). Text entry, spin boxes, lists, typed text and its replay, the engine and OS clipboards, and world-space UI. Phase 2.7b is complete.**

**What landed:**
- **`concordia-input`:** `InputEvent::Text(TextInput)`: `Insert(String)` (1..=4096 bytes, no control characters but newline and tab) or `Edit { key: EditKey, select }` (`Left`, `Right`, `Up`, `Down`, `Home`, `End`, `Backspace`, `Delete`, `SelectAll`, `Copy`, `Cut`, `Paste`, `Enter`). The sampler keeps the tick's text in order (`typed`) and `input.sample` publishes it as `TypedText`; playback feeds recorded text back (`play_back_text`). `ConsumedActions::consume_keys` hides every key-bound action. The starter map gains `ui_shift` and `ui_ctrl` (18 actions).
- **`concordia-replay`:** each input change gains an optional `text` list; ticks with only typing are recorded; runs play it back.
- **`concordia-platform`:** key presses also become typed text (`text_from_key`): editing keys and Ctrl/Cmd + A, C, X, V are `Edit` commands (Shift selects; repeats kept so held keys repeat), other keys insert the OS's text (control characters dropped; keys without an engine code still insert).
- **`concordia-ui`:**
  - **`UiTextBox`** (single- and multi-line, `placeholder`, `max_len` in characters, `password`, `editable`, `caret` / `anchor` byte offsets on character boundaries, `wrap`) and **`edit`**, a pure function of one typed input and the clipboard: insert over the selection (cut to `max_len`; a single-line box turns newlines to spaces), move and select by character, line start and end, and by logical lines keeping the column; delete; select all; copy and cut (never from a password box); paste; Enter submits or breaks the line. `UiTextChanged {text}` and `UiTextSubmitted {text}`.
  - **Layout:** text lines carry their byte range and caret stops from parley's clusters (`caret_x`, `byte_at`); `Parts::TextBox` has the drawn text (masked for a password, or the placeholder), caret and selection rects, and a single-line box's scroll that keeps the caret 2 px inside. Single-line text is centered vertically.
  - **`UiSpinBox`** works with a `UiTextBox` on the same entity: arrows in a 20 px column at the right, ↑ / ↓ when focused, typed numbers committed (clamped, rounded to `decimals`) on Enter or when focus leaves; the text shows the value otherwise.
  - **`UiList`** (children are items): click selects; in a `multi` list Ctrl toggles and Shift selects the range from the anchor; ↑ / ↓ when focused.
  - **Interaction:** typed text goes to the focused box; a press puts the caret at the nearest character edge, a drag selects, Shift-click extends; while a text box has focus the keyboard is consumed. Copies and cuts write `UiClipboard` (a play-world resource); `ui.clipboard` reads it.
  - **Drawing:** selection (`FOCUS_COLOR`, 0.35), the text or placeholder (half opacity) clipped to the content and scrolled, the caret; spin arrows as wedge-masked triangles; list selections highlighted over their items.
  - **World-space roots:** `compute_layout_seen` places `RootSpace::World` roots by the frame's camera: centered on the projection of the entity's world position (parents included) plus `offset`, at `pixel_size`; hidden behind the camera, past its depth range or entirely off screen; far to near, before screen roots. Without a camera they're skipped.
- **`concordia-api`:** `UiFrame::clip_from_world`.
- **`concordia-render`:** `clip_from_world` (shared with `camera_ray`); captures and presents fill the frame's camera from the capture's view or camera, or the window's.
- **`concordia-app`:** `Clipboard` / `OsClipboard` (arboard 3.7.0 without default features; opened on first use, warned about once if missing). The player turns a paste into the OS clipboard's text (cut to 4096 bytes), or the engine's paste when the OS has none, and after input writes each new `ui.clipboard` generation to the OS once.
- **Licences:** BSL-1.0 allowed in both deny configs (P2), for arboard and `clipboard-win`. On Linux arboard uses `x11rb` in pure Rust: no system library.
- **Studio:** `concordia-ui` covers text boxes, spin boxes, lists and world-space roots; `concordia-record-replay` typing. No tools change. Every enabled role's numbers on 2026-11-23:

  | Role | Tools | Tokens |
  |---|---|---|
  | QA | 25 | 23,260 |
  | level designer | 37 | 23,144 |
  | UI/UX | 26 | 21,424 |
  | tech artist | 23 | 20,172 |
  | animator | 22 | 16,890 |
  | sound designer | 24 | 15,138 |
  | composer | 20 | 13,054 |
  | gameplay programmer | 17 | 12,946 |
  | director | 13 | 12,453 |
  | writer | 11 | 9,576 |

  `input_inject`'s schema grew with `Text`, so QA and UI/UX grew about 500 tokens. All are under 50k.
- **Docs:** ADR-0060 "2.7b.3 notes", an ADR-0072 note on typed text, and the agent guide's text entry, world-space UI and `ui.clipboard`.

**Differences from the refinement (this As built wins):**
- **No replay version bump.** Typed text is an optional per-tick field that's omitted when empty, so files without typing are byte-identical and stay version 0.
- **A spin box is a text box plus arrow parts on one entity**, not child entities the module builds: nothing is spawned behind the author's back.
- **Up and Down move by logical lines** (newlines), not wrapped lines.
- **World roots take no input in M2** and `ui.inspect` / `ui.hit_test` don't report them: neither the simulation nor those methods have a camera.
- **Not built:** IME (as decided, U6), double-click word selection, a blinking caret (no clock in drawing), and scrolling inside a multi-line box (put it in a `UiScroll`).

**Tests** (expected values derived in each header; anything that passed at once or was written after its code is called out below):
- **`concordia-input` `tests/text_input.rs` (6):** typed text on its tick in order, its checks, playback, consumed keys, the modifier actions, serialization.
- **`concordia-replay`:** `typed_text_round_trips_and_is_omitted_when_empty`, `typed_text_is_recorded_and_replayed`.
- **`concordia-platform`:** `key_presses_become_typed_text`.
- **`concordia-ui`:**
  - `text_edit` (10): inserting and editing at the caret, char boundaries, Shift selection and select all, the clipboard, `max_len`, Enter, Up and Down, read-only boxes, and the validation and defaults of the three components;
  - `text_widgets` (10): caret and selection rects, placeholder, scrolling, password masking, spin arrows; typing and keyboard consumption, clicks and drags, copy and paste, spin boxes, lists;
  - `text_draw` (5): selection, text and caret order and clipping, placeholder opacity, spin triangles, list highlights, and the text fixture's draw-list snapshot;
  - `world_ui` (4): a parented root follows its entity, offset and pixel size, hiding, far-to-near order;
  - `engine`: `tweens_animate_a_world_root_offset`, and the module lists `ui.clipboard`;
  - `golden` (4 new): `ui_text_boxes`, `ui_spin_box`, `ui_list`, `ui_world_damage_number`, plus `capture_screenshot_places_world_ui_with_its_camera`.
- **`concordia-app`:** `paste_reads_the_os_clipboard_and_copies_reach_it`; `text_input_is_recorded_and_replayed`.
- **`concordia-studio`:** the UI and replay skill tests extended to the new components and typed text.
- **Mutation checks** (ten of eleven mutants failed a test; restored files touched): `max_len`, password copying, char-boundary steps, Shift ranges, spin clamping, the far-to-near sort, text playback, recording text-only ticks, key consumption, the capture's camera. **Survived (equivalent):** weakening the behind-the-camera `w` check; a point behind the eye also falls outside the depth range (or divides to infinity), so the depth check hides it the same way.
- **Called out:**
  - **Passed at once:** `tweens_animate_a_world_root_offset` (2.4.4's tweens already reach fields inside enum variants).
  - **Written after their code:** `capture_screenshot_places_world_ui_with_its_camera` and `text_input_is_recorded_and_replayed`.
  - **Changed on purpose:** the starter map's action count in `starter_map_has_the_ui_actions` (16 → 18); `ui_module_registers_its_method_and_debug_layer` lists `ui.clipboard`; a weak world-UI test that only checked a JSON path was replaced by the engine tween test; the damage-number golden's fixture (its roots stack their children, so the label centres and the bar has width).

**Checklist (2026-11-23):**
- `just lint` and the Tauri shell's clippy: clean.
- `just wasm`, `just native-fixtures`, then `cargo nextest run --workspace --no-fail-fast` with `CONCORDIA_REQUIRE_GPU=1`: 1,784 run, 1,778 passed; the 6 failures are the snapshots and goldens awaiting review (below). Doc tests: clean.
- `just gate-tests M1` (4/4) and `M3S` (13/13).
- `just deny` and the editor's deny (fresh advisory DB): advisories, bans, licenses and sources ok (BSL-1.0 now allowed).
- `just editor-schemas`: the text widgets, `ui.clipboard`, `InputEvent::Text` and the frame changes, committed.
- `just editor-test`: lint, 118 unit and 22 end-to-end tests pass.
- No new Linux system library (arboard's X11 backend is pure Rust).
- After blessing: ui, input, replay, app, platform, render, studio and mcp rerun with 718 run and 718 passed; `just gate-tests M1` passes 4/4 and `M3S` 13/13, with nothing forced.

**Snapshots and goldens** (collected with `INSTA_UPDATE=new`; goldens written to `target/golden-candidates/ui/`; shown on the review page; blessed 2026-11-23 after the author approved it, with `assertion_line` stripped):
- new: the text fixture's draw list and the four goldens;
- changed: the `input.inject` input schema.

**Carry-over:**
- world roots take no input, and the query methods don't report them;
- assets first named by a widget spawned during play measure as missing in the simulation; trackpad scrolling arrives as lines;
- the ignored `run_bench_latency_measures_the_e…` test; the navmesh layer's re-gather; the editor's deny warning; ADR-0069's chunk components (M7 7.7); physics queries seeing last-step bodies; the crowd's sequential steps at 10k (M7 7.7);
- `concordia run` with sound and UI (typing, the OS clipboard, world UI) not yet exercised by hand (2.10's sample game).

### Phase 2.10 refinement (2026-11-23, against the code after 2.7b.3)
**Status: refined; author decisions L1 and L2 taken (2026-11-23). Sub-phases 2.10a–2.10d below.** Reconciled with D2, the phase table's 2.10 row and the "2.10" test shape above: the shape holds (scripted headless play through `input.inject`, `sim.step` and `query.run`; replay across thread counts; goldens; zero error logs; every role M2 unlocked uses a tool in the sample), and this section names the game and its tests.

**Author decisions (2026-11-23):**
- **L1. The game is "Lamplight"** (a night-watch lamplighter game; chosen over a market courier and a garden defence). Traffic and damage numbers are given real jobs: wagons refill the lamplighter's oil, and lamplight burns the fog wisps.
- **L2. Content comes from our own generators plus CC0 packs**, each third-party file recorded with source, licence and SHA-256.

**What the code taught (checked before writing this):**
- Scripts see only component batches, signals and commands (`wit/concordia-game/world.wit`), never resources. Player input is the `ActionStates` resource, so **no script can read input today** (gap S1).
- No `.cue.ron`, `.buses.ron` or `.theme.ron` file exists in the repo, and there are no built-in sounds; the sample's are the first.
- M2's ADR block (0060–0079) is full (0064 and 0065 stay reserved for M5), so 2.10's decisions are updates to existing ADRs (S1 to ADR-0062; the gate's completion to ADR-0067).
- `just gate-tests M2` selects `binary(/^m2_/)`; there are no `m2_*` gates yet, and `regression.yml` runs gate tests for M-1, M0, M1 and M3S only.

#### Game design: "Lamplight" (working title; `examples/m2-lamplight`)
- **Premise and genre:** a small third-person night-watch action game in an original harbour town. It is not an ARPG: there is no loot, no levels, no click-to-move and no isometric camera.
- **Core loop:**
  - It's dusk. You are the town's lamplighter, walking with the keyboard (WASD/arrows, a chase camera).
  - Fog wisps drift in from the harbour edge, a crowd on the navmesh heading for the town square.
  - Walk to a lamp post and hold **Light**: a reach one-shot animation plays, and its anim event lights the lamp, with a cue and an intensity tween.
  - Lamps burn oil and go out when empty, and lighting one spends your can's oil.
  - **Refill from the oil wagons:** wagons circulate on the lanes, and the lanes brake for people. Step into the road in front of one and it stops; touch it to refill. So traffic is part of the loop, not scenery.
  - **Lamplight burns wisps.** Each lamp has a light-radius sensor. A wisp inside one loses wisp-health every tick, and the losses float above it as orange **world-space damage numbers** that rise and fade with a tween. At 0 the wisp pops, with a cue.
  - **Townsfolk** wander between homes and market stalls (a crowd with an idle/walk graph). A wisp's fear sensor startles them (a one-shot animation) and they flee home.
  - A wisp that reaches the square dims the **Town Spirit** meter, shown as a blue "−n" over the fountain.
- **Win:** dawn bells ring (night length set on the title screen) with Town Spirit above 0.
- **Lose:** Town Spirit reaches 0, and the fog takes the square.
- **Scope of the slice:**
  - one town (harbour edge, square, two streets, a ring road with 2 junctions);
  - 12 lamps, about 300 townsfolk, up to about 200 wisps, 4 oil wagons;
  - a title menu, the HUD, a pause menu and an end screen;
  - no saving: the night-records list lasts for the session.
- **Originality:** our own premise, characters, town and names. Nothing from the port game: a grep of the private markers against the new files is part of the checklist.

#### Every M2 system, and its job in the sample
| System | Use |
|---|---|
| Terra processors, phases, signals, observers, command buffers | the rules run as scripted processors; sensor contacts arrive as signals; wisps and numbers are spawned and despawned through commands |
| Templates, traits, spawners | `templates/{lamp,wisp,townsfolk,oil_wagon,burn_number,spirit_number}.template.ron`; spawners place townsfolk (`Disc`) and harbour wisps (`Box`) by seed, and a timer-driven spawn adds new wisp waves |
| Simulation and representation LOD | distant townsfolk and wisps tick at a LOD rate; far townsfolk switch to instanced, then hidden |
| Scripting, WASM | `scripts/lamplight-rules` (lamps and oil, the burn, Town Spirit, the game state and HUD values) |
| Scripting, native | `scripts/lamplight-fog` (wisp waves and steering targets, townsfolk fear) as a hot-reloaded dylib; the gate calls `script_reload` mid-night |
| Materials (Substrate) | wet cobbles (clear coat over stone: vertical layering); lamp glass (haze and thin film); awning cloth (fuzz); wisps (emissive, with coverage); wagon paint (coat) |
| Animation | the lamplighter's graph (idle/walk blend by speed; a Light one-shot by trigger); the townsfolk graph (walk blend; a Startle one-shot); events: footsteps (cues) and `lit` (lights the lamp); the lantern pole on the hand bone (attachment); tweens for lamp intensity, wisp fade and the numbers |
| Navigation, crowds, traffic | a navmesh from static colliders; townsfolk and wisp crowds; lanes, 2 junctions and wagons that yield and brake for people |
| Replay | the gate records the scripted night and verifies it |
| Audio | cues (footstep, light, sizzle, pop, wagon rumble on a spatial emitter, the dawn bells, UI clicks); `buses.ron` (Master → Music, SFX, Ambience, UI); a streamed Ogg night theme with loop points |
| In-game UI | a theme; the HUD (oil and spirit bars, a night clock, lamps lit); the title menu with a **name text box** (OS clipboard paste), a **night-length spin box** and a **fog-density dropdown**; the pause menu with **volume sliders per bus**; the end screen with a **night-records list**; world-space burn and spirit numbers |
| Physics | the lamplighter's `CharacterController`; sensors on lamps, wisps and wagons; contacts as signals |

#### Content: where it comes from (author: our generators plus CC0 packs)
- **Generators**, committed under `examples/m2-lamplight/tools/`; their outputs are committed with a `LICENSE.md` row "CC0-1.0, own work, the command that regenerates it":
  - `make_characters.py` (Blender 5.2, like `tools/fixtures/make_fbx.py`): the lamplighter and townsfolk mannequins, with our own bone names, clips (Idle, Walk, Light, Startle) and clip events in the sidecars;
  - `make_props.py` (Blender): the wisp and the lantern pole;
  - `make_sounds.py` (Python stdlib synthesis, then `oggenc`, as in `tools/fixtures/make_audio.sh`): sizzle, pop, the light whoosh, bells, the UI click and the music loop;
  - `make_textures.py` (Python stdlib PNG writer, seeded): cobble and stripe masks.
  - Generated files are pinned by SHA-256; a regeneration that differs is a review item, not a silent change.
- **CC0 packs** for what generators make poorly: town buildings, lamp posts, a wagon and a few foley sounds (footsteps, cart rumble), for example from Kenney's CC0 kits.
  - Every file is recorded in `assets/LICENSE.md` with its source URL, pack version, licence and SHA-256.
  - Files are downloaded into an empty scratch directory, checked, and copied in only after the author reviews the picks on the artifact page.
  - No third-party marks in names.
  - CC0 is already allowed by deny.toml; anything else is asked first.
- **Fonts:** the built-in Noto Sans. Spawned labels use built-in fonts, which sidesteps the "assets first named during play measure as missing" carry-over (note kept).

#### Project layout
```
examples/m2-lamplight/
  concordia.toml          # [scene] main, [sim] rate 60 seed, [window], [input] map, [physics], [nav], [anim],
                          # [audio] buses, [ui], [render], [replay], [[scripts.module]] ×2 (wasm rules, native fog)
  scenes/town.scene.ron   # town, lamps, lanes, junctions, spawners, camera, HUD and menu roots
  templates/*.template.ron
  scripts/lamplight-rules/ (WASM)   scripts/lamplight-fog/ (native dylib)
  input/lamplight.input.ron   # the starter UI actions + move, light, pause
  ui/lamplight.theme.ron
  audio/lamplight.buses.ron  audio/cues/*.cue.ron  audio/*.ogg|wav (+ .import.ron)
  materials/*.material.ron  anim/*.animgraph.ron  assets/ (models, textures, LICENSE.md)
  tools/ (generators)  README.md  build/ (git-ignored)
```
The `justfile`'s `wasm`, `native-fixtures` and `lint` recipes learn both crates. A new `just play-sample` runs `concordia run --project examples/m2-lamplight`.

#### Engine gaps the sample needs (fixed test-first in 2.10a)
- **S1. Scripts can't read input.** A `concordia-input` component, `InputActions { watch: Vec<String>, states: BTreeMap<String, ActionState> }`, is filled in `PrePhysics` after sampling by an `input.actions` processor. Consumed actions read as released, so typing a name never moves the lamplighter. Recorded as an update to ADR-0062 (scripts read input through components).
- **S2. Sensors that move with crowd agents and vehicles.** Kinematic sensor bodies that follow `Transform` must raise `ContactBegan`/`ContactEnded`. Prove it with a test, and fix it if it fails.
- **S2b (found by the spike, 2026-11-23). Sensors can't see the player.** The character controller is a Jolt `CharacterVirtual` with no inner body (`jolt-sys/shim/concordia_jolt_ext.cpp`, `cj_character_create`), so no sensor ever reports the lamplighter. Fix: give it a kinematic inner body carrying the entity's user id, so sensors report `ContactBegan`/`ContactEnded` naming the character entity; the character still never collides with its own inner body.
- **Spike notes that shape the content (no engine change):** physics places every collider at its own entity's `Transform` (no hierarchy), so a lamp is two entities (a solid static post and a static light-radius sensor). Jolt never reports sensor–sensor contacts, so wisps are kinematic solid bodies on a `wisp` layer that collides only with the lamp-light and fear sensor layers; townsfolk carry the kinematic fear sensors, and wagons are kinematic sensors (traffic already brakes for people, and a character controller counts as a person). Scripts receive engine signals (`ContactBegan`, `AnimEvent`, `UiPressed`) by registered name.
- **S3. Anything a 2.10a spike finds** while wiring one lamp, one wisp and one wagon end to end (the spike itself is thrown away). Each gap gets a failing test in its own crate, then a fix, and is listed in the As built. If the list grows past about three, stop and re-plan with the author.

#### Engine API, MCP tools and budgets
- **No new methods or tools are planned.** The gate uses existing tools.
- Every As built re-measures all ten enabled roles with `role_tool_budget.rs`. All must stay under 50k (today's largest is QA at 23,260).
- If S3 adds a tool, it is scoped to the roles that need it, and the numbers are reported.

#### Studio roles and skills
- **No role is enabled or disabled.** The VFX artist and modeler stay disabled.
- **`concordia-scripting`** gains "Reading input" (`InputActions`).
- **A new skill, `concordia-sample-lamplight`:** a guided tour of how the sample is built, for the director and the gameplay programmer.
- **Stale notes fixed:** `concordia-write-playtest` says `replay_run` is missing, but it exists; its `assert_eval` note stays.
- Roster and sync snapshots are updated after review.

#### Tests (first list; expected values hand-derived in each test's header)
- **2.10a:**
  - `concordia-input`: `input_actions_component_mirrors_watched_actions`, `consumed_actions_read_as_released`, `input_actions_replay_exactly`;
  - `concordia-physics`: `static_sensor_reports_a_kinematic_body_following_its_transform`, `kinematic_sensor_reports_a_kinematic_body`, `sensors_report_a_character_controller`, `character_never_collides_with_its_own_inner_body`;
  - a script-guest fixture test `wasm_guest_reads_input_actions` (a native build too).
- **2.10b (content):**
  - `every_asset_has_a_licence_row_with_matching_sha256`;
  - `licences_are_in_the_deny_allow_list`;
  - `all_assets_import_without_problems`;
  - `anim_graphs_and_cues_validate`;
  - `generated_files_match_their_pins`.
- **2.10c (game; pure rule tests in the script crates, in-process integration under `tests/integration/m2_lamplight_*.rs`):**
  - the rules: oil burn per second, the light cost, burn damage per tick in light, the pop at 0, spirit loss, dawn win and spirit-0 loss;
  - the loop: a lamp lit by the reach event, a refill on wagon contact, a wagon braking for the lamplighter, townsfolk startle and flee;
  - the UI: name entry with the movement keys consumed, the spin box setting the night length, sliders changing bus gain, the records list after a night;
  - LOD: far townsfolk tick at their rate.
- **2.10d (gate):** see below.
- **Mutation checks:** oil burn, burn-in-light, the win/lose thresholds, consumed input, refill on contact and the LOD rate. Restored files are touched for a fresh mtime.

#### The deterministic gate (`tests/gates/m2_lamplight.rs`, over MCP stdio)
Scripted play uses only `input_inject` (`Text` for the name, UI pointer events for the buttons, `Action` values for `move`/`light` per tick range), `sim_step` and `query_run`.
- `m2_gate_scripted_night_is_won`:
  - type "Ada", start, walk and light 4 lamps, stop a wagon and refill, reach dawn;
  - assert: the state is `Won`, lamps lit ≥ 4, wisps burned > 0, burn numbers spawned, spirit > 0, the records list holds "Ada" (`ui_inspect`).
- `m2_gate_idle_night_is_lost`: start and do nothing; the state is `Lost` before dawn.
- `m2_gate_replay_is_exact_across_runs_and_threads`:
  - record the won night with `replay_record`;
  - `replay_run` verify in processes started with `--threads 1` and `--threads 8`;
  - every checkpoint matches, the final hashes are equal, and two fresh runs give equal hashes.
- `m2_gate_hot_reload_keeps_the_night_running`: `script_reload` on the native module mid-night; the state continues.
- `m2_gate_goldens`: the title screen with the typed name, mid-night (HUD plus world numbers) and the dawn screen, compared with FLIP against `tests/golden/gates/` on the macOS GPU (skipped on Linux, as in M1).
- `m2_gate_audio_mix`: `audio_render` of a window holding a known cue plus the music. Snapshot a per-bus peak/RMS digest, and use `audio_inspect` to show cues fired.
- **Zero error logs:** every gate test ends with `log_tail {"level":"error"}` and expects it to be empty.
- `m2_gate_every_m2_role_uses_a_tool_in_the_sample`: a table from role to tool, checked two ways: the tool is in the role's synced scope, and the gate's MCP client recorded calling it.
  - gameplay programmer → `script_reload`
  - animator → `anim_inspect`
  - sound designer → `audio_inspect`
  - composer → `audio_buses`
  - UI/UX → `ui_inspect`
  - tech artist → `material_get`
  - level designer → `terra_stats` / `nav_path`
  - QA → `replay_run`
- **Wiring:**
  - snapshots and goldens are collected with `INSTA_UPDATE=new` and shown on an artifact page before blessing;
  - the gate binaries join the slow-test filter in `.config/nextest.toml`;
  - `regression.yml` adds `just gate-tests M2`.

#### Optional live Studio gate (`just gate-live M2`: `tools/gates/m2_live.sh` + `tests/gates/m2_live_check.rs`)
- **The brief to the director:** add a **Watchtower lamp** at the harbour. It has twice the radius and burns twice the oil, with its own material, its own "lit" cue, a HUD counter, and a `lamplight-rules` change. QA records a replay and approves.
- **Judged on outcomes only:**
  - the new template validates and spawns;
  - in a scripted night with it lit, more wisps burn than in the baseline;
  - its material and cue load, and the cue fires (`audio_inspect`);
  - the HUD label exists (`ui_inspect`);
  - the deterministic gate still passes against the team's copy;
  - zero error logs;
  - journal actors are `agent:<role>`;
  - QA approved the task;
  - no API keys were in the environment.
- A non-ignored scripted-twin test exercises the checker, as `m3s_live_check` does.

#### Windowed play (by hand, `just play-sample`), closing the carry-over
The author records pass or fail per item in the Status:
- the music loops;
- footsteps, the light cue and sizzle are heard;
- the wagon rumble pans as wagons pass;
- the pause sliders audibly change Music, SFX and Ambience;
- typing, Backspace and Shift-selection work in the name box;
- Cmd+V pastes from the OS clipboard, and Cmd+C copies out to another app;
- the world numbers follow wisps and hide behind the camera;
- the spin box, dropdown and list work.

#### Sub-phases (one PR each, stacked; check whether each base should be `main`)
1. **2.10a `m2/2.10-seams`:** S1, S2 and the spike's S3, plus docs (ADR-0062 update, crate AGENTS/README, agent guide, scripting skill).
2. **2.10b `m2/2.10-content`:** generators, CC0 picks (author reviews them first), imports, materials, anim graphs, cues, buses, theme and input map, plus the content tests.
3. **2.10c `m2/2.10-game`:** the scene, templates, both script modules, the UI, and the rule and integration tests.
4. **2.10d `m2/2.10-gate`:** the M2 gate, goldens, the live gate with its twin, `just play-sample`, the CI wiring, the new skill, and the M2 closing docs.

#### The M2 milestone gate, and what closes M2
- **What runs:** `just gate M2` = `just ci` (lint, wasm, native fixtures, the full suite, deny), then `just gate-tests M2` (`m2_lamplight`, plus `m2_live_check`'s twin test).
- **M2 is closed when all of these hold:**
  1. the gate is green in the manual `regression` workflow on `main` (macOS with the GPU, and Linux without goldens), per ADR-0071;
  2. the author records the windowed checklist;
  3. the M2 plan is updated: Status "Gate passed (date)", the 2.10 As built, and the carry-overs moved to their owners (M7 7.7, M3);
  4. `plans/README.md` shows M2 as "Gate passed" in the index, crate table and ADR registry;
  5. `docs/design.md` and ADR-0067 carry a D2 completion note.
- The live gate is optional: run on the author's subscription, and recorded if it was run.

#### Carry-overs kept noted
- world UI takes no input;
- assets first named during play measure as missing in the UI simulation;
- trackpad scrolling arrives as lines;
- the ignored `run_bench_latency` test;
- the navmesh layer's re-gather;
- the editor's deny warning;
- ADR-0069's chunk components (M7 7.7);
- physics queries seeing last-step bodies;
- the crowd's sequential steps at 10k (M7 7.7).

### Phase 2.10a As built (2026-11-23; ADR-0062 and ADR-0041 updates)
**Status: done (branch `m2/2.10-seams`). The two engine seams the sample needs: scripts read input, and sensors see characters.**

**The spike** (reading the code paths a lamp, a wisp and a wagon would use, instead of a throwaway project) found S1 and one wall beyond the plan, S2b. Everything else the game needs is already reachable: scripts receive engine signals by name, write engine components (`CharacterMotion`, `AnimGraphPlayer`, `UiText`), and spawn templates; traffic already treats characters as people. The content notes it produced (one collider per entity at its own `Transform`, no sensor–sensor contacts) are in the refinement above.

**What landed:**
- **`concordia-input` (S1):**
  - The `InputActions { watch, states }` component, registered by the input module.
  - The `input.actions` exclusive system (`ACTIONS`): `PrePhysics`, after `input.sample` and `ui.interact`, before `nav.click` (each when scheduled).
  - `mirror_action(previous, raw, consumed)`: consumed reads as released with the kind's zero value; edges come from the mirror's own previous state (a consumed press reports `just_released`; one seen again while held reports `just_pressed`; a tap inside a tick keeps both edges).
- **`jolt-sys` and `concordia-physics` (S2b):**
  - Characters get Jolt's kinematic inner body (the same capsule, the character's moving layer, `CjCharacterDesc::user`, mapped to the entity), so sensors report `ContactBegan`/`ContactEnded` naming the character.
  - The shim keeps it for sensors only: `OnContactValidate` rejects every other pair with an inner body, and a `NotInner` body filter makes raycasts and every character's own movement skip inner bodies. Bindings regenerated.
- **Fixture:** `tests/fixtures/script/input-guest` (`inputg`), built by `just wasm` and `just native-fixtures`, and added to `just lint`'s WASM clippy.

**Found while building (this As built wins):**
- **Characters blocked each other.** The first inner body made every character collide with the others' inner bodies. The existing `physics_crowd_agents_keep_apart_and_arrive` caught it ("they did meet" failed: the agents never met). The fix is the `NotInner` filter on the character's move, pinned by the new `characters_still_pass_through_each_other`, which failed first (both stopped 0.3 m short) and then passed.
- **A stale C++ build after mutation checks:** restoring and touching the shim after the last mutant still left the mutated library linked for the next full suite (`raycasts_ignore_a_characters_inner_body` failed there). Touching it again rebuilt `jolt-sys` and the test passed. The memory note on mutation checks now covers C++ shims.
- **S2a needed no engine change:** static and kinematic sensors already report kinematic bodies that follow their `Transform` (the sensor flag sets Jolt's `mCollideKinematicVsNonDynamic`).

**Tests** (expected values derived in each header; anything that passed at once or was written after its code is called out below):
- **`concordia-input` `tests/input_actions.rs` (4):** `input_actions_component_mirrors_watched_actions`, `consumed_actions_read_as_released` (with a stand-in `ui.interact` taking the keyboard on chosen ticks), `mirror_action_follows_the_rules`, `input_actions_is_a_registered_component`.
- **`concordia-replay`:** `input_actions_replay_exactly`.
- **`concordia-app` `tests/input_guest.rs` (2):** `wasm_guest_reads_input_actions`, `native_guest_reads_input_actions_with_the_same_hash` (1 m in 30 ticks at 2 m/s; one press; equal hashes).
- **`concordia-physics`:**
  - `tests/sensors.rs` (6): `static_sensor_reports_a_kinematic_body_following_its_transform`, `kinematic_sensor_reports_a_kinematic_body`, `sensors_report_a_character_controller`, `character_never_collides_with_its_own_inner_body`, `inner_body_reports_no_contacts_with_solid_bodies`, `characters_still_pass_through_each_other`;
  - `raycasts_ignore_a_characters_inner_body` in `tests/query.rs`.
- **`concordia-studio`:** `scripting_skill_reads_input_through_input_actions` (the skill's `InputActions` and `ActionState` mirrors match the engine's).
- **Red run:** the four input tests failed against stubs for the right reasons (the component unregistered, a stub mirror); `sensors_report_a_character_controller` failed with no contacts; `characters_still_pass_through_each_other` failed short; the skill test failed for the missing mirror.
- **Mutation checks** (all nine mutants failed a test; restored files touched; see the stale-build note): watched-name filtering, consumed ignored, no release edge when consumed, no tap edge, `input.actions` not after `ui.interact`, no inner body, validation accepting every pair, rays seeing inner bodies, and `input.actions` unscheduled (both guest tests fail).
- **Called out:**
  - **Passed at once:** `static_sensor_reports_a_kinematic_body_following_its_transform` and `kinematic_sensor_reports_a_kinematic_body` (S2a was already true); `character_never_collides_with_its_own_inner_body` and `raycasts_ignore_a_characters_inner_body` (guards written before the fix, passing by design).
  - **Written after their code:** `input_actions_replay_exactly`, both guest tests (`input.actions` existed; the unscheduling mutant shows they bite), and `inner_body_reports_no_contacts_with_solid_bodies`.
  - **Test mistake fixed:** `schema.get` takes `name`, not `type`; sensors float 0.1 m above the floor so a kinematic one never touches it.

**Studio and docs:**
- **Skill:** `concordia-scripting` gains "Reading input" (`InputActions` and `ActionState` mirrors, `after("input.actions")`, consumed input) and the note that sensors see characters.
- **No tools changed.** Every enabled role's numbers on 2026-11-23 are unchanged from 2.7b.3: QA 25 tools / 23,260 tokens; level designer 37 / 23,144; UI/UX 26 / 21,424; tech artist 23 / 20,172; animator 22 / 16,890; sound designer 24 / 15,138; composer 20 / 13,054; gameplay programmer 17 / 12,946; director 13 / 12,453; writer 11 / 9,576. All are under 50k.
- **ADR-0062 update** (input reaches scripts as a component) and **ADR-0041 update** (characters' inner bodies for sensors).
- **Also updated:** the AGENTS.md of `concordia-input`, `concordia-physics` and `jolt-sys`; the README.md of `concordia-input` and `concordia-physics` (which was empty); the agent guide's "Input and sensors for gameplay"; and `plans/README.md` (crate rows, ADR registry).

**Checklist (2026-11-23):**
- `just lint` and the Tauri shell's clippy: clean.
- `just wasm`, `just native-fixtures`, then `cargo nextest run --workspace --no-fail-fast` with `CONCORDIA_REQUIRE_GPU=1`: 1,799 run, 1,799 passed. Doc tests: clean.
- `just gate-tests M1` (4/4) and `M3S` (13/13).
- `just deny` and the editor's deny (fresh advisory DB): advisories, bans, licenses and sources ok. The editor's `advisory-not-detected` warning is the known carry-over.
- `just editor-schemas`: `InputActions`, `ActionState`, `ActionValue` and `Vec2`, committed.
- `just editor-test`: lint, 118 unit and 22 end-to-end tests pass.
- No new Linux system library (the shim is C++ already built in CI).
- No new or changed snapshots or goldens.

**Carry-over:** unchanged from 2.7b.3, plus the windowed checks that 2.10d's `just play-sample` closes.

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
- Motion matching, IK, retargeting beyond same-skeleton clips, audio DSP effects, rich text, UI theming. *Note (2026-11-20, ADR-0016):* IK, rigs and in-engine keyframing are now M12; motion matching and retargeting stay in the backlog.
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
- **Note (2026-11-20, ADR-0016):** M12 (Rigging v1.0) builds rigs, IK and in-engine keyframing on this crate. It asks 2.4 to leave two seams: a `PoseProcessor` trait run after an entity's animation graph (so `concordia-rig` plugs in without `concordia-anim` depending on it), and clip channels that can be addressed by bone name, so M12's `.clip.ron` text model maps onto the runtime clip. A separate render (presentation) pose, if 2.4 has one, is what M12's Presentation rigs write. Neither changes 2.4's tests.

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

> **Superseded in detail by the "Phase 2.5 refinement (2026-11-22)" section above** (sub-phases 2.5a–2.5d; crowds as per-agent Terra processors in the parallel scheduler; the Godot mapping in M5; area marks from `NavArea`/`NavAreaGrid`). The text below is the original plan.

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
> Split by the 2.7a refinement (2026-11-22): the audio half is "Phase 2.7a refinement" above, which supersedes this text for audio; the UI half is phase 2.7b, refined in "Phase 2.7b refinement" above (2026-11-22), which supersedes this text for the UI.

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
- **Determinism** (Jolt, Detour crowd, animation-timed events, hit-stop time scale, WASM floats, hash-map iteration). *Mitigation:* nav and crowd processors in Terra's deterministic parallel scheduler (2.5 refinement; neighbours read from last-tick state), anim in `FrameEnd`, `SimClock` time scale, NaN canonicalization and fuel in WASM, cross-thread-count replay tests in 2.4/2.5/2.6/2.8, Jolt's deterministic build flags (assumes M1).
- **Visual parity**: custom `light()` toon shading, screen/depth-texture shaders, `WorldEnvironment` features and particles have no M1 equivalent. *Mitigation:* the lighting-model hook (2.3), luma-statistics parity against the game's own measuring harnesses, goldens of our own output blessed after review, particles reported and deferred.
- **tree-sitter-gdscript may lag recent Godot 4.x syntax.** *Mitigation:* vendored grammar, corpus test over every `.gd` file in the reference port, patch upstream.
- **Scope: 12+ crates in one milestone.** *Mitigation:* each crate ships the minimum the slice needs (no IK/retargeting, no audio DSP, no rich text, single navmesh); phases 2.3–2.8 run in parallel; anything not needed by the gate goes to the backlog.
- **Native hot reload on macOS** (dlopen caching, TLS destructors, Rust ABI instability). *Mitigation:* copy-on-load, C-ABI vtable, no statics in game crates, tick-boundary swaps, a static-link fallback that is always tested.
- **Private content vs. CI.** The game's assets cannot go in the open-source repo, so the M2 gate cannot run on public CI. *Mitigation:* the gate runs on a self-hosted runner on the author's machine (or locally) with the two env vars; public CI runs everything else, including all fixture-based tests. Game-specific details stay in the git-ignored `private/` folder.
- **Asset licenses.** Some prop packs in the reference port may be marketplace content. *Mitigation:* the port project is private; the importer records source and license metadata where known (design.md guardrail).
- **Weak oracles.** Several suites are "structural" (e.g. a health-bar suite that asserts shader parameters). *Mitigation:* re-target structural assertions to our equivalents, and rely on playthrough/replay/golden tests for behavior.

## Open questions
*Cleaned up in 2.4b.1 (2026-11-21):* the port questions moved to M5 with the Godot work (ADR-0048, ADR-0067) and are now M5's open questions 11–14; two are answered (where noted); particles and the text stack stay open.

- ~~Grid layers (walkability, terrain type, ground surface, ...) as base64 in `.data.ron`, or as PNG layers beside it?~~ **Moved to M5** (open question 11), with `.data.ron` and the Godot importer.
- ~~Wire encoding across the WIT boundary: reflect-described fixed layout vs. a schema'd format such as postcard?~~ **Answered in 2.6a (ADR-0062):** a fixed layout per type, derived from its shape; postcard was rejected (no random access to rows).
- Particles: a minimal CPU emitter in M2 for loot effects and fire patches, or wait? (Open. *Note (2026-11-20, ADR-0015):* particles are M11's (Sparx), and M11's open question 11 recommends "wait for M11", with no CPU emitter in M2.)
- ~~Should the port project be its own private git repository or a private branch of the game repo?~~ **Moved to M5** (open question 12).
- ~~Do probe scripts run with Godot's `--script` from outside the project reliably?~~ **Moved to M5** (open question 13).
- ~~Should the reference port's grid-to-polygon navmesh builder be ported as an alternative nav source?~~ **Moved to M5** (open question 14); 2.5 builds navmeshes with Recast only.
- ~~Does M0 already provide typed ECS events and timers, or does M2 add them?~~ **Answered by 2.0 and 2.0b:** M2 added them (timers in 2.0, ADR-0063; typed signals and observers in 2.0b, ADR-0069).
- Is `cosmic-text` the right text stack for the shipped game UI, or is a smaller atlas-only rasterizer enough for M2? (Settled at the 2.7b refinement, P1: parley and swash; cosmic-text 0.19 fails `cargo deny` through `ttf-parser`, RUSTSEC-2026-0192. ADR-0060.)

## ADRs to write
- ADR-0067: M2 refinement: engine breadth on Terra's Mass core, Substrate-style materials, Godot mappings to M5, sample-game gate (written).
- ADR-0068: Substrate-style material model (slabs, operators, closure budget), written in phase 2.3a.
- ADR-0069: Terra storage kinds, command buffers, signals and observers (written in phase 2.0b).
- ADR-0070: Terra templates, traits and spawners; the MCP tool-list budget rises to 16k (written in phase 2.0c; the budget superseded by ADR-0073).
- ADR-0073: tool budgets are per agent role; access is admin-configured in the roster; no whole-list budget (written 2026-11-20).
- ADR-0074: animation runtime: one pose per character, clips bound by bone name, sampled in `FrameEnd`; the importer changes (written in phase 2.4.1).
- ADR-0064 and ADR-0065 (port projects, Godot import identity) move to M5 with the port; their numbers stay reserved.

M2 draws from the 0060–0079 block (`plans/README.md`).

- ADR-0060: In-game UI approach (retained-mode ECS UI, taffy + Godot anchor mode, parley + swash; written in 2.7b.1).
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
