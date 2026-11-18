# Milestone Plans

The top-level design is in [`../docs/design.md`](../docs/design.md). This folder holds one **implementation plan per milestone**. Executing a milestone plan produces code, test-first, one phase at a time.

> The engine is **Concordia** (ADR-0008). Crates use the prefix `concordia-`, the CLI binary is `concordia`, and the MCP server is `concordia`.

## Index

| Milestone | Plan | Depends on | Detail | Status |
|---|---|---|---|---|
| M-1 Toolchain & scaffold | [M-1-toolchain-and-scaffold.md](M-1-toolchain-and-scaffold.md) | none | Ready to execute | Gate passed |
| M0 Headless core | [M0-headless-core.md](M0-headless-core.md) | M-1 | Ready to execute | Gate passed |
| M1 See it | [M1-see-it.md](M1-see-it.md) | M0 | Ready to execute | In progress (phase 1.5b) |
| M2 Port begins | [M2-port-begins.md](M2-port-begins.md) | M1 | Needs refinement after M1 | Planned |
| M3 Closed loop & Studio v1 | [M3-closed-loop-and-studio.md](M3-closed-loop-and-studio.md) | M2 (Studio phases may start after M0) | Needs refinement after M2 | Planned |
| M4 Unreal-class visuals | [M4-unreal-class-visuals.md](M4-unreal-class-visuals.md) | M1 (render), M2 (graphs) | Needs refinement after M3 | Planned |
| M5 Bring your game | [M5-bring-your-game.md](M5-bring-your-game.md) | M2 (importer framework) | Needs refinement after M4 | Planned |
| M6 Evolve (plugins) | [M6-evolve-plugins.md](M6-evolve-plugins.md) | M2 (parallel with M3) | Needs refinement after M2 | Planned |
| M7 Terra (large-scale simulation) | [M7-terra.md](M7-terra.md) | M0 (Phase 7.0 rename, done); M1, M2, M4 (simulation features) | Needs refinement after M4 | In progress (Phase 7.0 done; rest after M4) |
| Later | [later-backlog.md](later-backlog.md) | none | Backlog | n/a |

Status values: `Planned` → `Refined` → `In progress (phase N)` → `Gate passed`.

---

## Milestone plan template

Every milestone plan uses these sections, in this order:

```
# M<n>: <Title>
Status · Depends on · Detail level · Estimated effort

## Goal & context
## Scope / Non-goals
## Dependencies (what must already exist, from which milestone)
## Crates, modules & directories (created / touched)
## Phases
### Phase <n>.<k>: <name>
- Objective
- Deliverables
- Tests first        (named tests with the behavior each asserts; written and failing BEFORE implementation)
- Implementation notes (key types, APIs, algorithms, libraries, papers/talks)
- Done criteria
## Milestone gate (the executable acceptance test: exact command and what it asserts)
## Risks & mitigations
## Open questions
## ADRs to write
## Effort estimate
```

Phase numbering: `M0` phases are `0.1`, `0.2`, …; `M-1` phases are `-1.1`, `-1.2`, ….

---

## Shared conventions (all plans MUST use these names)

### Repository layout (project root = this folder's parent)
```
Cargo.toml            # workspace
rust-toolchain.toml   # pinned stable channel
justfile              # task runner entry points: just test, just lint, just gate M0, ...
CLAUDE.md, AGENTS.md
docs/                 # design.md, clean-room.md, adr/NNNN-title.md, guides
plans/                # this folder
crates/<crate-name>/  # every Rust crate; each has src/, tests/, AGENTS.md, README.md
wit/                  # WIT packages for the WASM gameplay API (package: concordia:game@0.1.0)
editor/               # Tauri v2 app: editor/ui (TypeScript + React + Vite, pnpm), editor/src-tauri
studio/               # default Agent Studio config: team.toml, agents/<role>.md, skills/
tests/                # cross-crate integration, gates/, golden/, replays/, fixtures/
tools/                # dev scripts, exporter plugins (tools/unreal-exporter, tools/unity-exporter)
third_party/          # vendored C/C++ sources (Jolt, Recast) with LICENSE files
examples/             # small sample projects used by tests and docs
```

### Canonical crate names (`crates/`)
| Crate | Purpose | First milestone |
|---|---|---|
| `concordia-core` | math (glam re-export), ids (`EntityId` UUIDv7), time, tracing/logging, job system | M0 |
| `concordia-reflect` (+ `concordia-reflect-derive`) | type registry, `#[derive(Reflect)]`, JSON Schema export, doc strings | M0 |
| `concordia-terra` | **Terra**, the entity system (ADR-0010): own archetype ECS, queries, systems, schedules, fixed-step deterministic loop; renamed from `concordia-ecs` in M7 Phase 7.0 and grown into large-scale simulation in M7 | M0 |
| `concordia-command` | command log, transactions, undo/redo, `Actor` attribution | M0 |
| `concordia-scene` | text scene/prefab format: read, write, canonical formatting, stable IDs | M0 |
| `concordia-api` | Engine API: typed request/response types, JSON-RPC 2.0 server (stdio + WebSocket), sessions | M0 |
| `concordia-mcp` | MCP server binary, a thin adapter over `concordia-api` | M0 |
| `concordia-cli` | the `concordia` binary: `concordia new/run/headless/test/import/studio ...` | M0 |
| `concordia-platform` | windowing and OS integration (winit), file watching | M1 |
| `concordia-input` | input actions, bindings, input injection API | M1 |
| `concordia-rhi` | RHI traits and types; backends `concordia-rhi-metal` (M1), `concordia-rhi-vulkan` (M1 stretch / M2) | M1 |
| `concordia-shaders` | Slang compilation, reflection, shader cache | M1 |
| `concordia-render` | render graph, PBR clustered forward+, materials, cameras, capture | M1 |
| `concordia-assets` | asset database, importers registry, content-addressed derived-data cache (`.concordia/ddc`), hot reload | M1 |
| `concordia-import-gltf` | glTF 2.0 importer | M1 |
| `concordia-physics` (+ `jolt-sys`) | Jolt via its C API, safe wrapper, ECS integration | M1 |
| `concordia-audio` | mixer, spatial audio, streaming | M2 |
| `concordia-anim` | skeletons, clips, blending, state machines | M2 |
| `concordia-nav` (+ `recast-sys`) | navmesh build/query, agents/crowds | M2 |
| `concordia-script-wasm` | Wasmtime component host for `wit/` world `concordia:game/gameplay` | M2 |
| `concordia-import-godot` | Godot 4 `.tscn`/`.tres`/`project.godot` importer, GDScript→Rust translation support | M2 |
| `concordia-ui` | in-game UI (HUD, menus) | M2 |
| `concordia-replay` | input/state recording, deterministic replay (minimal input log in M2), playtest assertions (M3) | M2 |
| `concordia-studio` | Agent Studio: role configs, team, task board, `studio sync` → Claude Code files | M3 |
| `concordia-gen` | generated-content provider interface (procedural first, opt-in model providers) | M3 |
| `concordia-graph` | visual graphs stored as text, compiler to fast code | M4 |
| `concordia-ai` | runtime AI: behavior trees, utility AI, optional LLM planner plugins | M4 |
| `concordia-import-unity`, `concordia-import-unreal` | importers targeting the interchange format | M5 |
| `concordia-interchange` | interchange model (USD/glTF + scene text) shared by importers | M2 (introduced with the Godot importer, generalized in M5) |
| `concordia-testkit` | dev-dependency helpers: FLIP golden compare, `gpu_test!`, bless support | M1 |
| `concordia-script-native` | tier-1 hot-reload host for Rust game crates | M2 |
| `concordia-script-guest` | guest SDK for `wasm32-wasip2` gameplay components | M2 |
| `concordia-import-fbx` (+ `ufbx-sys`) | FBX import via ufbx (MIT) | M2 |
| `concordia-translate` | logic-translation tooling (GDScript in M2; C#/Blueprint front ends added in M5) | M2 (extracted and generalized in M5) |
| `concordia-cluster` (+ `meshopt-sys`) | cluster/DAG/page building for virtualized geometry | M4 |
| `concordia-pathtrace` | reference path tracer (test oracle; must not depend on `concordia-render`) | M4 |
| `concordia-ai-llm-local` | opt-in example local-LLM provider; not a default workspace member | M4 |
| `concordia-evolve` | Evolve plugin system: manifests, resolution, lockfile, trust, orchestration over the native and WASM hosts | M6 |
| `concordia-evolve-api` | stable extension-point API that native plugins link (WIT package `concordia:evolve` for WASM plugins) | M6 |
| `concordia-terra-sim` | Terra templates and traits, processing phases, LOD, signals, spawners, representation | M7 |
| `concordia-terra-crowd` | Terra crowds and traffic over `concordia-nav` | M7 |

**Test package:** `tests/` is the `concordia-tests` package (`publish = false`, `autotests = false`, explicit `[[test]]` targets). Shared helpers (MCP test client, binary locators, gate checkers) live in its library, `tests/src/`.

New crates may be proposed in a plan's "Open questions" section; they are added here during the consistency pass.

### Text formats & file extensions
- Format: **RON**, with canonical formatting (sorted keys, one entity per block, stable UUIDv7 ids, trailing commas). ADR-0009 (M0) confirms or replaces it.
- `.scene.ron`: scenes. `.prefab.ron`: prefabs. `.material.ron`: materials. `.input.ron`: input maps (M1). `.animgraph.ron`: animation graphs (M2). `.data.ron`: typed data documents (M2). `.replay.ron`: replays (v0 input log in M2, extended in M3). `.playtest.ron`: assertion suites (M3). `.score.ron`: procedural music scores (M3). `.graph.ron`: visual graphs (M4). `.template.ron`: Terra entity templates (M7). `evolve.toml`: Evolve plugin manifest; `evolve.lock`: project plugin lockfile (M6). `.cmdlog.jsonl`: command journal, at `.concordia/journal/<scene-id>.cmdlog.jsonl` (M0).
- **Exception (ADR in M3):** Studio role files (`studio/agents/<role>.md`) and tasks (`studio/tasks/T-0001-<slug>.task.md`) are Markdown with **YAML frontmatter**, to match Claude Code's native agent format.
- **Ids:** entities created in the engine get UUIDv7. **Imported** entities get name-based UUIDv5, derived from the source id plus a project import namespace, so re-importing is deterministic.
- **Asset references:** `"asset:<uuid>"`, `"asset:<uuid>#<sub-path>"`, `"builtin:<kind>/<name>"`. DDC layout: `.concordia/ddc/<2 hex>/<blake3 key>`.
- Project manifest: `concordia.toml` in a game project's root.
- Assets keep their source formats (`.glb`, `.png`, `.wav`, ...). Per-asset import settings go in a sidecar file `<file>.import.ron`, holding a stable asset id.

### Engine API & MCP naming
- The Engine API is JSON-RPC 2.0. Method names are `domain.verb`, e.g. `scene.open`, `scene.save`, `entity.spawn`, `entity.despawn`, `component.get`, `component.set`, `query.run`, `schema.get`, `txn.begin`, `txn.commit`, `txn.undo`, `txn.redo`, `sim.play`, `sim.pause`, `sim.step`, `capture.screenshot`, `input.inject`, `asset.import`, `replay.record`, `replay.run`, `assert.eval`, `task.list/create/update` (M3), `log.tail`.
- MCP tools map 1:1, with `.` replaced by `_` (e.g. `entity_spawn`). Every MCP tool's input schema is **generated from `concordia-reflect`**, never hand-written.
- Every mutating call carries an `actor` (grammar `<human|agent|system|tool|script>:<name>[#n]`). It may be omitted on the wire when the session has a default (`session.open`), but the recorded actor is always present. `human:*` actors are not accepted over MCP. The MCP server is named `concordia` in `.mcp.json`, so Claude Code tools appear as `mcp__concordia__<tool>`.
- **Edit world vs play world:** the command log records mutations of the **edit world** only. Simulation (physics, scripts, AI) runs in a separate **play world** (`world: "edit" | "play"` param on read methods) and never mutates the edit world. Injected input and play-world outcomes are recorded by `concordia-replay`, not the command log.
- **Additional methods agreed in the plans** (MCP names follow the same rule): M0: `session.open`, `scene.new`, `scene.info`, `entity.get`, `component.patch`, `component.remove`, `txn.abort`, `txn.history`, `sim.stop`. M1: `asset.list`, `physics.raycast`, `events.subscribe`. M2: `port.*`, `shader.translate`, `script.reload`, `nav.build`, `nav.path`. M3: `task.get/claim/comment/approve/reject`, `studio.roster/status/sync/release_actor`, `audit.record`, `gen.list/run`. M4: `graph.*`, `ai.inspect/trace`, `render.stats`, `render.set_debug_view`, `perf.capture`, `capture.reference`. M5: `import.scan/run/report/licenses`, `translate.*`.
- Errors: JSON-RPC codes −32001…−32012, with `error.data = { kind, suggestions[], ... }` (defined in M0). MCP resources use `concordia://` URIs.

### Testing conventions (TDD)
- Runner: `cargo nextest run`. Doc tests: `cargo test --doc`. Editor UI: `vitest` + Playwright.
- Unit tests live in the crate (`#[cfg(test)]`); crate integration tests in `crates/<c>/tests/`; cross-crate tests in `tests/`.
- Test names describe behavior: `spawned_entity_has_stable_uuid`, `undo_restores_previous_component_value`.
- **Gate tests** go in `tests/gates/m<n>_*.rs` (`M-1` → prefix `m_minus1`) and run with `just gate M<n>`, which runs `just ci` first. A milestone is done only when its gate passes in CI.
- **Live-agent gates:** `#[ignore]` tests named `m<n>_live_*`, run with `just gate-live M<n>`. They drive the real `claude -p` (never `--bare`, which bypasses the subscription login), strip API-key env vars from the child process, and assert outcomes, never transcripts.
- **Private-content gates:** gates that need the reference port content (M2, parts of M4) keep their test file in `tests/gates/` but read the game through `CONCORDIA_PORT_SRC`/`CONCORDIA_PORT_PROJECT`. They run on the self-hosted runner, the author's Mac, never in public CI, and fail (never skip) when their env var is set but the content is missing.
- **Reference hardware:** the author's Apple M3 Pro (18-core GPU, 18 GB, Metal 4). It is the self-hosted GPU runner and the perf reference (`tests/perf/reference.ron`). Perf tests report `NotReference` elsewhere, and gate perf tests fail on it.
- Golden images: `tests/golden/<suite>/<name>[@<class>].png`, compared with a perceptual metric (NVIDIA FLIP) and per-test tolerance. They're refreshed with `just bless` (`CONCORDIA_BLESS=1`; never allowed when `CI=true`), and failure artifacts go to `target/golden-failures/`.
- Snapshot tests for text output (scene files, schemas, generated agent files): `insta`.
- Property tests for serialization and ECS invariants: `proptest`.
- Determinism tests: run the same fixed-step simulation twice (and across thread counts), then compare world-state hashes.
- Every phase lists its tests before its implementation notes, and those tests are committed failing first.

### Principles to honor in every plan
0. Subscription-first: nothing requires an LLM API key. Agents act through MCP, the CLI and Claude Code native files.
1. One API: the editor, CLI, tests and agents all use the Engine API.
2. Text is the source of truth.
3. Closed loop: headless, capture, inject, replay, assert.
4. Every mutation is a transaction with an actor.
5. Clean room (see `docs/clean-room.md`).
6. AI-legible: small crates, `AGENTS.md` per crate, doc comments on every public item.
7. TDD.

---

## ADR number registry
To avoid collisions, each milestone draws ADR numbers from its own block. Use the next free number in your block, and add a row here when you write one.

| Block | Owner | Assigned |
|---|---|---|
| 0001–0008 | Design | 0001 Rust · 0002 subscription-first · 0003 scripting tiers · 0004 editor · 0005 interchange · 0006 licensing · 0007 TDD/plans · 0008 name (Concordia) |
| 0009 | M0 | 0009 scene text format (written: canonical RON, own reader/writer) |
| 0010–0019 | Design additions | 0010 Terra entity system · 0011 Evolve plugin system |
| 0020–0029 | M-1 | 0020 toolchain pinning · 0021 WASM guest build path · 0022 CI and supply chain |
| 0030–0039 | M0 | 0030 own archetype ECS (written: safe column storage) · 0031 Engine API error model (written) · 0032 edit world vs play world (written) · 0033 MCP adapter (written: rmcp pin, protocol versions) |
| 0040–0059 | M1 | 0040–0045 as named in M1's plan (0040, 0041, 0042, 0044, 0045 written) · 0046 engine modules (written) |
| 0060–0079 | M2 | |
| 0080–0099 | M3 | (incl. YAML-frontmatter exception for Studio files) |
| 0100–0119 | M4 | (incl. perf testing on reference hardware) |
| 0120–0139 | M5 | |
| 0140–0159 | M6 | 0140–0142 as named in M6's plan |
| 0160–0179 | M7 | 0160–0161 as named in M7's plan |

## Consistency pass log (2026-11-17)
- Merged each plan's proposed conventions into the sections above.
- Test package unified as `concordia-tests` (M1 had `concordia-gates`; M0 and M3 helper paths moved to `tests/src/`).
- ADRs renumbered into blocks (M0: 0010–0012 → 0030–0032; M1: 0010–0015 → 0040–0045).
- M-1 pin must be ≥ 1.88 (`rmcp`).
- M4 Phase 4.13 depends on the `caster` archetype being ported after M2's slice (brute and creep only), with a `brute` fallback.
- M5: CUE4Parse (Apache-2.0) is allowed as a reference, with NOTICE attribution.
- The reference machine is fixed as the author's M3 Pro.

## M0 phase 0.0 log (2026-11-17)
- ADR-0009: RON confirmed by the author (written as Accepted in phase 0.5).
- ADR-0033 added to M0's block (MCP adapter).
- `rmcp` pinned `=3.11.0`; MCP protocol 2026-07-28 is stateless (no `initialize`), 2025-11-25 still supported.
- Crate `AGENTS.md` required sections extended with *Public entry points*, *Common tasks* and *Don'ts* (template and hygiene test updated).
- M0 gate tests are written at the start of phase 0.7 and excluded from the default nextest run until phase 0.8.
- Phase 0.3: ECS storage is safe Rust (ADR-0030), so the Miri decision is moot; no nightly toolchain installed.

## Terra and Evolve additions (2026-11-17)
- Author decisions: the entity system is **Terra** (ADR-0010): `concordia-ecs` is renamed `concordia-terra` right after the M0 PR merges (M7 Phase 7.0) and grows Mass-style features in M7. The plugin system is **Evolve** (ADR-0011), milestone M6, native and WASM plugins from local paths and pinned git sources.
- Numbering: M6 and M7 are new milestone numbers, not schedule positions. M6 starts after M2 (in parallel with M3); M7's simulation features start after M4. Existing milestones keep their numbers.
- Done 2026-11-18 (M7 Phase 7.0): the crate is `concordia-terra`, and the M1–M5 plans, design doc and crate docs use the new name; only ADRs and plan history mention `concordia-ecs`.
- Actor grammar gains `plugin:<id>` in M6 (ADR-0011).
- The M3 `concordia-gen` provider seam and the later-backlog "optional plugin crate" items become Evolve plugins once M6 lands.

## M1 start (2026-11-18)
- M1 is reconciled with M0 as built, and its dependencies are re-checked (see the M1 plan's *Execution notes*). Author decisions D1–D9 are recorded there. OpenGL is rejected as a backend (ADR-0040's alternatives).
- New Phase 1.0, the engine-module seam (ADR-0046): feature crates add types, methods, play systems and play-time state through `EngineModule`. Each engine has its own method registry; `concordia_api::methods()` stays the core set.
- `sim.*` accept an attribution-only `actor`, and `sim.step` logs `sim stepped`. Input injection and physics act only on the play world and are never journaled (ADR-0032).
