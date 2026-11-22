# M7: Terra (large-scale entity simulation)
Status: Phase 7.0 done (2026-11-18). **ADR-0067: phases 7.1–7.6 and the basic crowd and traffic work of 7.7 moved to M2** (2.0, 2.0b, 2.0c, 2.4b, 2.5); the `terra.*` tools arrive with them. M7 keeps large-scale performance, AI-driven crowd and traffic behavior (after M4) and its gate (7.9), planned after M4 · Depends on: M0 (ECS), M1 (render instancing), M2 (nav and Detour crowds), M4 (runtime AI, virtualized geometry) · Detail level: **Needs refinement after M4** · Estimated effort: ~12 weeks (range 10–16) after 7.0, author plus agents

> Outline plan. Phase 7.0 (the rename) is small and fully specified; phases 7.1–7.9 are refined when M4 lands, because representation and behavior depend on M1/M4 rendering and M4 runtime AI.

## Goal & context
**Terra** is Concordia's entity system: the archetype ECS from M0, renamed from `concordia-ecs` to `concordia-terra`, grown into a framework for simulating very large numbers of entities (crowds, traffic, swarms, ecosystems) the way Unreal's Mass framework does. It is designed from public documentation and talks about that framework's concepts only (clean room), and from the ECS literature already cited by M0.

What Terra adds on top of the M0 store:
- **Entity templates and traits:** reusable, text-defined compositions of components (traits) that spawn thousands of entities in batches.
- **Processors:** systems that declare their component access, so the scheduler can run non-conflicting processors in parallel (the parallel system scheduling M0 deferred), grouped into processing phases.
- **Shared and chunk components:** per-archetype shared data and per-chunk data (e.g. LOD buckets), plus tags.
- **Deferred structural changes:** command buffers that batch spawns, despawns and component moves at phase boundaries.
- **LOD:** simulation LOD (lower tick rates and cheaper processors far away) and representation LOD (full entity → instanced mesh → nothing).
- **Signals and observers:** named per-entity events and add/remove observers (the change detection M0 deferred).
- **Spawners and representation:** placing large populations, switching how they are drawn.
- **Crowds and traffic:** Detour-based avoidance at scale and lane-based traffic, with behavior from M4 runtime AI.

Concordia rules still apply: determinism (same inputs, same hash, any thread count), text as source of truth (templates are `.template.ron`), AI-first tooling (`terra.*` API methods with generated schemas), and the edit/play split (Terra simulation runs in the play world; templates and spawner definitions are edited through the command log).

## Scope / Non-goals
**In scope:** the rename; templates and traits; processors with access declarations and parallel scheduling; processing phases; shared/chunk components and tags; deferred command buffers; simulation and representation LOD; signals and observers; spawners; representation switching to instanced rendering; crowd and traffic integration; `terra.*` Engine API methods and debug views; performance and determinism gates.

**Non-goals (M7):** networking and replication of Terra entities (backlog: multiplayer); world-partition streaming of populations (backlog: large worlds); GPU-driven simulation; a smart-object system (open question).

## Dependencies (what must already exist, from which milestone)
| Need | From |
|---|---|
| Archetype store, queries, `SimLoop`, `WorldHash`, reflection, command log, scenes | M0 |
| Render graph, instanced draws, input | M1 |
| Navmesh, Detour crowds, animation (for representation) | M2 |
| Runtime AI (behavior trees, utility AI), meshlet GPU culling for dense crowds (ADR-0100) | M4 |
| Evolve (optional): Terra traits and processors can ship as plugins | M6 |

## Crates, modules & directories (created / touched)
```
crates/concordia-terra/        the M0 ECS, renamed; gains access declarations, parallel scheduler, shared/chunk
                               components, tags, command buffers, observers
crates/concordia-terra-sim/    templates and traits, processing phases, LOD, signals, spawners, representation
crates/concordia-terra-crowd/  crowd and traffic over concordia-nav
crates/concordia-api/          terra.* methods
```

## Phases

### Phase 7.0: Rename `concordia-ecs` to `concordia-terra` (right after the M0 PR merges)
- **Objective:** adopt the Terra name while only four crates depend on the ECS.
- **Tests first:** the existing `concordia-ecs` suite (31 tests, model proptest, determinism, perf smoke) passes unchanged under the new crate name; a hygiene test asserts no `concordia-ecs` / `concordia_ecs` references remain outside ADRs and plan history.
- **Implementation notes:** `git mv crates/concordia-ecs crates/concordia-terra`; update the workspace, dependents (`concordia-command`, `concordia-scene`, `concordia-api`, `concordia-mcp`), `AGENTS.md`/README, `plans/README.md` and the M1–M5 plans (the consistency pass replaces `concordia-ecs` with `concordia-terra`). No API changes. The ECS benchmark baseline moves with the crate.
- **Done criteria:** `just gate M0` green; all docs say Terra.
- **Done (2026-11-18):** `git mv` keeps history; the hygiene test `ecs_is_called_terra_everywhere` allows the old name only in ADRs and plan history (M0 plan, M7 plan, `plans/README.md`).

### Phase 7.1: Processors, access declarations & parallel scheduling (moved to M2 phase 2.0)
- **Tests first:** `conflicting_processors_never_run_concurrently`; `independent_processors_run_in_parallel`; `declared_order_constraints_are_respected`; `schedule_cycle_is_an_error_naming_the_cycle`; `world_hash_identical_for_1_and_8_threads_with_parallel_processors`.
- **Notes:** processors declare reads/writes (components, resources); the scheduler builds a conflict graph per phase and runs a deterministic topological order; results never depend on thread timing.

### Phase 7.2: Shared components, chunk components, tags & command buffers (moved to M2 phase 2.0b)
- **Tests first:** `shared_component_is_stored_once_per_archetype_value`; `chunk_component_is_per_chunk_and_survives_moves`; `tags_filter_queries_without_storage`; `deferred_spawns_apply_at_phase_end_in_deterministic_order`; `model_proptest_with_command_buffers_matches_the_model`.

### Phase 7.3: Templates & traits (moved to M2 phase 2.0c; done, ADR-0070)
- **Tests first:** `template_composes_traits_into_an_archetype`; `trait_conflicts_are_reported_with_both_traits`; `template_roundtrips_byte_identical`; `batch_spawn_of_100k_from_template_is_deterministic`; `template_schemas_come_from_reflection`.
- **Notes:** `.template.ron` holds traits and defaults; traits are reflected types plus the processors they require.

### Phase 7.4: Simulation & representation LOD (moved to M2 phase 2.4b; done, ADR-0076)
- **Tests first:** `far_entities_tick_at_their_lod_rate`; `lod_transitions_are_hysteresis_damped`; `simulation_lod_depends_only_on_deterministic_inputs`; `representation_lod_follows_the_camera_without_affecting_world_hash`.
- **Notes:** simulation LOD is driven by LOD reference points that are part of simulation state (e.g. the player in the play world), never by the render camera, so replays stay deterministic; representation LOD may use the camera.

### Phase 7.5: Signals & observers (moved to M2 phase 2.0b)
- **Tests first:** `signal_raised_this_tick_is_delivered_next_tick_in_entity_order`; `observers_fire_on_add_and_remove_once` (both done in M2 2.0b, ADR-0069); `signals_survive_lod_changes` (moved to M2 2.4b, with LOD).

### Phase 7.6: Spawners & representation switching (moved to M2 phases 2.0c and 2.4b; done)
- **Tests first:** `spawner_places_count_with_seeded_distribution`; `representation_switches_between_full_instanced_and_hidden`; `instanced_representation_draws_100k_in_one_pass` (GPU, golden image).

### Phase 7.7: Crowds & traffic (basic crowds and lane traffic moved to M2 phase 2.5; AI-driven behavior and 10k+ scale stay here)
- **Tests first:** `crowd_of_10k_agents_avoids_overlap`; `traffic_follows_lanes_and_yields_at_junctions`; `behavior_from_runtime_ai_drives_terra_agents`; `crowd_is_deterministic_across_runs_and_threads`.
- **Notes:** builds on M2 2.5c's crowds (per-agent Terra processors in the parallel scheduler, N1) and M4 runtime AI. `crowd_of_10k_agents_avoids_overlap` is a perf test on the reference machine and lives here (M2 2.5 refinement); M2 proves overlap at 1,000 agents and determinism at 10,000. The columnar extraction below is the natural first user of ADR-0069's chunk components (per-chunk "moved" flags).
- **Render extraction at crowd scale (from M2 2.4b.2).** Today extraction builds one `DrawItem` per entity and the scene cache diffs them: a 100k crowd that moves every tick costs about 113 ms to re-extract (`crowd_100k`, render `BASELINE.md`), far over a 60 Hz frame. Make it columnar and incremental: copy changed transform columns per archetype straight into instance data, and touch mesh, material and representation only when they change. This replaces the per-entity path rather than adding a second one.
  - **Tests first:** `moving_crowd_extraction_matches_full_extraction` (same draws as a fresh `RenderWorld::extract`); `unchanged_entities_are_not_reextracted`; every render golden unchanged.
  - **Target:** 100k instanced entities that all moved this tick: extraction, interpolation and the frame together fit the 60 Hz budget on the reference machine, measured with `crowd_100k`; exact split set at refinement.

### Phase 7.8: Agent tooling & debug views (the basic `terra.*` tools move to M2 phase 2.0c; scale views stay here)
- **Tests first:** `terra_methods_have_generated_schemas_and_mcp_tools`; `terra_stats_reports_counts_lod_buckets_and_processor_times`; `agent_spawns_a_crowd_from_a_template_over_mcp`.
- **Notes:** `terra.spawn_batch`, `terra.despawn_batch`, `terra.stats`, `terra.inspect`, `terra.set_debug_view`.

### Phase 7.9: Gate assembly, perf baseline, docs pass

## Milestone gate
`just gate M7` (`tests/gates/m7_*.rs`):
1. Over MCP (no API key), an agent creates a `.template.ron` for a pedestrian, a spawner for 100,000 of them on a test map, and runs `sim.step`.
2. Determinism: world hashes at ticks 1, 100 and 1000 are identical for 1 and 8 threads and across two runs.
3. Performance (reference machine, `tests/perf/reference.ron`): 100,000 crowd agents simulate at 60 Hz with simulation LOD, and the moving crowd renders within the same frame budget (7.7's extraction target); budget set at refinement.
4. LOD: entities beyond the LOD distance tick at their reduced rate; representation switches without changing the world hash.
5. Golden image of the instanced crowd (FLIP tolerance).

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| Parallel scheduling breaks determinism | Conflict-graph scheduling with deterministic order; thread-count hash tests in every phase |
| LOD makes replays diverge | Simulation LOD only from simulation-state inputs; representation LOD excluded from the hash |
| Storage changes regress M0 performance | Keep the M0 benchmarks and perf smoke; compare against `BASELINE.md` |
| Rename churn | Do it in 7.0 while only four crates depend on the ECS |

## Open questions
1. Smart objects (reservable interaction points) in Terra, or in M4 runtime AI?
2. Should traits be able to ship as Evolve plugins from day one (needs M6)?
3. GPU simulation for very large swarms: backlog or a Terra phase?

## ADRs to write
- ADR-0010 (design): Terra, the entity system (written with this plan).
- ADR-0160: Processor access declarations and deterministic parallel scheduling.
- ADR-0161: Simulation LOD and determinism.

## Effort estimate
| Phase | Estimate |
|---|---|
| 7.0 Rename | 1 day |
| 7.1 Processors & parallel scheduling | 2.5 weeks |
| 7.2 Shared/chunk components, tags, command buffers | 2 weeks |
| 7.3 Templates & traits | 1.5 weeks |
| 7.4 LOD | 1.5 weeks |
| 7.5 Signals & observers | 1 week |
| 7.6 Spawners & representation | 1.5 weeks |
| 7.7 Crowds & traffic | 1.5 weeks |
| 7.8 Agent tooling | 0.5 week |
| 7.9 Gate & docs | 0.5 week |
| **Total** | **~12 weeks (10–16) after 7.0** |

## Proposed convention additions
- Crates `concordia-terra` (renamed from `concordia-ecs`), `concordia-terra-sim`, `concordia-terra-crowd`.
- File format `.template.ron` (Terra entity templates).
- API methods `terra.spawn_batch/despawn_batch/stats/inspect/set_debug_view`.
- ADR block 0160–0179 for M7.
