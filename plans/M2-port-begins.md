# M2: Port begins

Status: Planned · Depends on: M1 (and through it M0, M-1) · Detail level: **Needs refinement after M1** · Estimated effort: ~22–26 calendar weeks (author + agents, phases partly parallel)

> This plan has full phase breakdowns and test lists, but it was written before M0/M1 exist. When M1's gate passes, do a refinement pass: re-check every M0/M1 API this plan assumes (marked "assumes M1"), re-measure the slice closure with the real `port.scan`, and re-estimate.

Game-specific details for the author's port live in the git-ignored `private/plans/M2-port-begins.md` (if present).

## Goal & context

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
- `concordia-ecs` (events/timers if M0 lacks them; `Sim` vs `Presentation` system sets; `SimClock::time_scale`), `concordia-physics` (character controller over Jolt `CharacterVirtual`, sensor enter/exit events, ray pick), `concordia-render` (GPU skinning, lighting-model extension point for translated `light()`, per-instance material params, UI pass, world-space text/billboards), `concordia-shaders` (compiling translated modules), `concordia-assets` (FBX, audio, anim graph, data docs, sidecars), `concordia-input` (pointer→world pick helper, imported bindings), `concordia-api`/`concordia-mcp`/`concordia-cli` (new methods), `justfile` (`gate M2`, `corpus`, `play-slice`), `deny.toml` (new licenses).

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

### Phase 2.1: Interchange model and Godot text formats

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

### Phase 2.2: GDScript structure tools and the translation workflow

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

### Phase 2.9: The opening-zone slice

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

## Milestone gate

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
- Does M0 already provide typed ECS events and timers, or does M2 add them to `concordia-ecs`?
- Is `cosmic-text` the right text stack for the shipped game UI, or is a smaller atlas-only rasterizer enough for M2?

## ADRs to write

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
