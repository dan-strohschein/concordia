# Concordia Engine API: guide for agents

The Engine API is how every client (agents, the CLI, the editor, tests) drives the engine. Over MCP the same methods appear as tools named with `_` instead of `.` (`entity.spawn` → `entity_spawn`). Everything here works headless and needs no API key.

## Workflow
1. `session.open {"client": "claude-code", "actor": "agent:claude-code"}` sets the default actor for this session. (The MCP server does this for you.)
2. `scene.new {"path": "scenes/arena.scene.ron", "name": "Arena"}` or `scene.open {"path": ...}`.
3. Discover shapes with `schema.get {}` (index), `schema.get {"name": "Transform"}` (component) or `schema.get {"name": "entity.spawn"}` (method). The index's `components` list gives each component's `category`, a one-line `description` and whether it is `addable`; a component's schema carries its `default`.
4. Edit with `entity.spawn`, `component.add` (the default value, then patch what differs), `component.set` (whole value), `component.patch` (merge patch, e.g. `{"scale": [2, 2, 2]}`), `component.remove`, `entity.despawn`. Group edits with `txn.begin {"label": ...}` … `txn.commit {"txn": ...}` so they undo as one step.
5. Check your work with `entity.get`, `component.get` and `query.run`.
6. `scene.save {}` writes canonical RON. `txn.undo` / `txn.redo` work across restarts when the file is unchanged. `txn.history` shows who changed which entities and when; `txn.undo_to {"txn": ...}` rewinds back to and including a commit.
7. Simulate without touching the scene: `sim.step {"ticks": 60}`, then read with `"world": "play"`; `sim.stop {}` discards the play world.

## Building a scene (M1)
1. Look around: `project.files {}` lists the project's files with their kind and import status.
2. Bring in a model: `asset.import {"path": "models/crate.glb"}` (glTF `.gltf`/`.glb` or `.fbx`), then `prefab.instantiate {"asset": "asset:<id>", "translation": [0, 0, 0]}` (one undo step). Both formats give the same sub-assets: `mesh/<mesh>/<part>`, `material/<name>` (one slab each), `texture/<name>`, `skin/<index>` and `clip/<name>`. FBX files are converted to meters, Y-up while importing, and each animation stack becomes a clip starting at 0; a missing external FBX texture is skipped with a warning in `log.tail`.
3. Add ready-made entities: `entity.templates {}` lists them; `entity.create {"template": "Cube", "translation": [0, 0.5, 0]}`, `{"template": "Directional Light"}`, `{"template": "Camera"}`. Group with `"parent"`, copy with `entity.duplicate`, regroup with `entity.reparent`. For physics, `component.add` `RigidBody` and `Collider` (a 1 m box by default), then `component.patch` the shape; for a sky, `entity.spawn` an entity with `Environment` (no `Transform` needed) and an imported `.hdr` as its `map`; sky light is scaled by the camera's exposure, so under daylight exposure (EV 15) its `intensity` is in the thousands. Materials are `.material.ron` files (see Materials below): write one with `material.set` and set the returned id as a `MeshRenderer`'s `material`. (`entity.spawn` still takes raw components.)
4. Look at it: `capture.screenshot {}` returns an image (768 × 432 by default; pass `width`/`height` for detail, `camera` or `view` to choose the viewpoint, `"world": "play"` to see the simulation, `view_mode` for a debug view such as `"wireframe"`, `"normals"`, `"overdraw"` or `"light_clusters"`). `render.stats {}` then reports that frame's cost (draws, triangles, CPU and GPU ms, GPU memory) and the materials simplified to fit the closure budget: use it to find what makes a scene slow or why something is missing. `viewport.pick` tells you which entity is at a pixel of that image; `viewport.drop` where something placed there would land.
5. Run it: `sim.step {"ticks": 120}`, `input.inject` to press actions, `physics.raycast` to probe, then read with `"world": "play"`.
6. `scene.save {}`.

## Templates, spawners and crowds (M2)
- A template is a `templates/**/*.template.ron` file (named by its file stem) or a module built-in. It composes **traits** (Rust-defined, configured by parameters, e.g. `"Movement": (velocity: (0.0, 0.0, 2.0))`) and plain **component** values, which override what traits add. Two traits adding the same component is an error naming both. `entity.templates {}` lists every template with its traits, components and any `problem`; `schema.get {"name": "Movement"}` shows a trait's parameters.
- One entity in the edit world: `entity.create {"template": "goblin"}` (one undo step).
- Many entities in a level: add a `Spawner` to an entity, `component.set {"entity": <id>, "component": "Spawner", "value": {"template": "goblin", "count": 200, "seed": 1, "distribution": {"Disc": {"radius": 20.0}}}}`. The scene keeps only the spawner; its entities appear, deterministically, when play starts (`Grid {spacing}`, `Box {half_extents}` or `Disc {radius}`, around the spawner's transform).
- Many entities right now, for a test: `terra.spawn_batch {"template": "goblin", "count": 1000, "seed": 2}` spawns into the play world (never the scene, never journaled); `terra.despawn_batch {"with": ["Hostile"]}` clears them.
- Check what Terra did: `terra.stats {"world": "play"}` (counts, skipped commands, the schedule's phases and parallel waves) and `terra.inspect {"entity": <id>, "world": "play"}` (archetype, chunk, each component's storage kind: dense, shared or tag, and pending signals).

## Replays (M2, projects with `[replay]`)
- Record: `replay.record {"path": "tests/replays/jump.replay.ron"}` starts a fresh play world from the open scene; drive it with `input.inject` and `sim.step` (or `sim.play`); `replay.record {"action": "stop"}` writes the file. It holds the input changes per tick (after binding, so rebinding keys doesn't break it) and world hashes every `checkpoint_every` ticks (default 60).
- Check: `replay.run {"path": ...}` replays it headless on a fresh play world and compares every checkpoint. `ok: true` proves the same behavior; otherwise `divergence` names the first checkpoint `tick` that differs, the `last_matching_tick`, and the `parts` that differ (component names, `@entities`, `@clock`, `@rng`, `@signals`). The play world stays at the end for `"world": "play"` reads.
- `mode: "resimulate"` replays the inputs on the current code and returns new `checkpoints` (prove a fix). A replay recorded with another scene, seed, engine or script build is refused with `ReplayMismatch`; `"force": true` runs it anyway and lists the `mismatches`.
- Nothing about replays is journaled or changes the scene. CLI: `concordia replay record <path> --ticks N [--inputs events.json]`, `concordia replay run <path> [--threads N] [--force]`.

## Materials (M2, projects with `[assets]`)
- A material is named **nodes** plus a **root** (ADR-0068). A node is a `Slab` (one BSDF closure: base color, metallic, specular, roughness, textures, emission, `transmittance`, and the optional `haze`, `fuzz`, `subsurface` and `thin_film`) or an operator over other nodes by name: `Vertical {top, bottom}` (layering, e.g. a clear coat), `Horizontal {a, b, mix, mix_texture}` (side by side) or `Coverage {node, coverage}`. Every node must be used by the root. `schema.get {"name": "Material"}` lists every field and range.
- Read: `material.get {"path": "assets/materials/steel.material.ron"}` → `{material, migrated, id}`. M1's flat files read as one slab named `surface` with `migrated: true`; saving rewrites them as layered text.
- Write: `material.set {"path": ..., "material": {...}}` (the whole material; the file is created if missing). It validates, writes canonical text, imports it and returns `{id, written}` (`written: false` if nothing changed); the renderer draws the new content on the next frame. An invalid material is refused with `InvalidParams`, naming the `node.field` (`data.field`, `data.node` and a pointer into the params).
- Like `asset.import`, it changes a file, not the scene: it is attributed in `log.tail` ("material saved", with the actor) and is not undoable. In the Studio, the path must be inside the role's write paths.
- Cost: each slab use is one closure; a material over the platform's `closure_budget` is simplified and listed in `render.stats` `downgraded_materials`. `capture.screenshot {"view_mode": "closures"}` shows the count per pixel.

## Animation (M2, projects with `[anim]` and `[assets]`)
- Imported glTF and FBX models with skins give their mesh entities `Skinned {skin: "asset:<id>#skin/<n>"}`; their clips are sub-assets `asset:<id>#clip/<name>` (`asset.list` shows them).
- Play one: `prefab.instantiate` the model, then `component.set` `AnimPlayer` `{"clip": "asset:<id>#clip/<name>"}` (optional `time`, `speed` (default 1, negative plays backward), `looping` (default true)) on the entity with `Skinned`. The clip may come from another file if its bone names match (an animation-only download).
- Playing (`sim.step`/`sim.play`), the play world's entity gets a `Pose` (`bones`: each joint's local transform, in skin order) and `AnimPlayer.time` advances at the fixed step (scaled by the time scale). Read both with `component.get` and `"world": "play"`; poses are part of the world hash, so replays check them.
- Rendering: `capture.screenshot {"world": "play"}` (and the viewport while playing) draws each character in its current pose, skinned on the GPU, with skinned shadows; the viewport blends poses between ticks. The edit world has no poses, so it draws the bind pose. An extra mesh part of a model follows its parent's pose.
- The clips a play session can use are loaded when play starts: every clip in the same file as a player's clip. Anything that fails to load is logged (`log.tail`, target `concordia::anim`) and that character holds its pose. Nothing about animation is journaled or changes the scene.

## Rules
- Every mutating call needs an actor: `"actor": "agent:<role>"` or the session default. Grammar: `<human|agent|system|tool|script>:<name>[#n]`, lowercase names.
- Entity ids are UUID strings. If you pass a name instead, the error tells you the matching id.
- Component values are JSON in serde's shape: structs are objects, glam vectors are arrays (`Vec3` = `[x, y, z]`, `Quat` = `[x, y, z, w]`), enums are `"Variant"` or `{"Variant": payload}`.
- While your transaction is open, other sessions' edits fail with `TxnConflict`. Open transactions abort after 120 s or when the session closes.
- The simulation never changes the edit world (ADR-0032).

## Methods
| Method | Mutates | Params → Result |
|---|---|---|
| `session.open` | | `{client, actor?}` → `{session, concordia_version, protocol}` |
| `scene.new` | yes | `{path, name, discard?}` → `{scene_id, path}` |
| `scene.open` | | `{path, discard?}` → `{scene_id, entity_count, history: "restored"\|"stale"\|"none"}` |
| `scene.save` | | `{path?}` → `{path, bytes, hash}` |
| `scene.info` | | `{}` → `{scene_id, name, path, dirty, entity_count, components_in_use}` |
| `scene.text` | | `{}` → `{path, text}`: the open scene as canonical RON, as `scene.save` would write it, without saving |
| `project.files` | | `{dir?}` → `{files: [{path, kind, size, asset, importer}]}` (projects with `[assets]`; skips hidden, git-ignored and sidecar files) |
| `asset.import` | | `{path}` → `{id, cached, sub_assets}` (import or re-import a project file: `.gltf`, `.glb`, `.fbx`, `.png`, `.jpg`, `.hdr`, `.material.ron`; idempotent; refer to it as `"asset:<id>"`; changed meshes, textures and materials redraw on the next frame) |
| `asset.list` | | `{}` → `{assets: [{id, path, importer, sub_assets}]}` |
| `material.get` | | `{path}` → `{material, migrated, id}` (a `.material.ron` as a layered material; M1 files read as one slab, `migrated: true`) |
| `material.set` | | `{path, material}` → `{id, written}` (validates, writes canonical text, imports; refused with the `node.field`; attributed in `log.tail`, not undoable) |
| `entity.spawn` | yes | `{components, id?}` → `{id}` |
| `entity.despawn` | yes | `{id, recursive?}` → `{}` (`recursive: true` also removes all descendants, one undo step) |
| `entity.templates` | | `{}` → `{templates: [{name, description, traits, components, problem?}]}` (ready-made entities: Empty, Cube, Sphere, Plane, Camera, Directional/Point/Spot Light, the project's `templates/**/*.template.ron`, plus any from plugins) |
| `entity.create` | yes | `{template, parent?, name?, translation?}` → `{id}` (spawns a template; sibling names stay unique, "Cube 2") |
| `entity.duplicate` | yes | `{ids}` → `{ids}` (copies with all descendants, new ids, references remapped; one undo step) |
| `entity.reparent` | yes | `{id, parent \| null, keep_world?}` → `{}` (keeps the world pose by default; rejects cycles) |
| `prefab.instantiate` | yes | `{asset, parent?, translation?}` → `{root, entities}` (an imported model's hierarchy, one undo step; joins an open transaction) |
| `entity.get` | | `{id, world?}` → `{id, components}` |
| `component.get` | | `{entity, component, world?}` → `{value}` |
| `component.add` | yes | `{entity, component}` → `{value}` (inserts the default; fails if the entity has it already or the component has no default, e.g. `Parent`, `Environment`) |
| `component.set` | yes | `{entity, component, value}` → `{previous}` |
| `component.patch` | yes | `{entity, component, patch}` → `{value}` |
| `component.remove` | yes | `{entity, component}` → `{previous}` |
| `query.run` | | `{with, without?, where?, select?, world?, limit?, cursor?}` → `{rows: [{id, components}], next_cursor?}` |
| `schema.get` | | `{name?}` → `{schema}` (no name: `{types, components: [{name, category, description, addable}], methods}`) |
| `txn.begin` | yes | `{label}` → `{txn}` |
| `txn.commit` / `txn.abort` | yes | `{txn}` → `{ops}` |
| `txn.undo` / `txn.redo` | yes | `{}` → `{txn, label, by}` |
| `txn.undo_to` | yes | `{txn}` → `{undone, by}` (undoes that commit and every later one, latest first; `txn.redo` replays them) |
| `txn.history` | | `{limit?}` → `{entries: [{seq, kind, txn, target, actor, label, ops, time_ms, entities, undone}]}` |
| `sim.play` / `sim.pause` / `sim.stop` | | `{rate_hz?}` / `{}` → `{state, tick}` |
| `sim.step` | | `{ticks}` → `{tick, world_hash}` |
| `terra.spawn_batch` | | `{template, count, seed?, distribution?, origin?}` → `{count, entities}` (play world only, starting it if needed; `entities` lists the first 1,000; fails naming any processor a trait needs that no module provides) |
| `terra.despawn_batch` | | `{entities?, with?}` → `{count}` (play world: these ids, and every entity with all the `with` components) |
| `terra.stats` | | `{world?}` → `{entities, archetypes, columns, shared_values, skipped_commands, phases: [{phase, waves}]}` (`phases` only for the play world; names in one wave run in parallel) |
| `terra.inspect` | | `{entity, world?}` → `{archetype, chunk, components: [{name, storage}], pending_signals: [{name, count}]}` |
| `input.inject` | | `{events: [{tick?, event}], actor?}` → `{accepted, next_tick}` (raw input into the play world, e.g. `{"event": {"Key": {"key": "Space", "pressed": true}}}`; the project's `.input.ron` maps it to actions) |
| `replay.record` | | `{action?: start\|stop, path?, checkpoint_every?, actor?}` → `{state, path, tick, inputs, checkpoints}` (projects with `[replay]`; start needs `path`) |
| `replay.run` | | `{path, mode?: verify\|resimulate, force?, actor?}` → `{ok, ticks, matched, divergence, mismatches, checkpoints, world_hash, elapsed_ms}` |
| `script.reload` | | `{module?, actor?}` → `{modules: [{name, backend, version, migrated, edit_txn}]}` (projects with `[scripts]`; loads what is on disk, never builds) |
| `physics.raycast` | | `{origin, direction, max_distance, layers?, world?}` → `{hit: {entity, point, normal, distance} \| null}` |
| `capture.screenshot` | | `{width?, height?, camera? \| view?, world?, overlays?, view_mode?}` → `{image, width, height, sim_tick, frame}` (projects with `[render]`; `overlays: true` adds the editor overlays; `view_mode`: `lit` (default), `unlit`, `wireframe`, `normals`, `albedo`, `roughness_metallic`, `overdraw`, `shadow_cascades`, `light_clusters`, and for layered materials `closures`, `haze`, `fuzz`, `subsurface`, `thin_film`) |
| `render.stats` | | `{}` → `{frame: {frame, width, height, view_mode, cpu_ms, gpu_ms, draws, culled, draw_calls, triangles, punctual_lights, shadows_cached, memory_bytes, closure_budget, downgraded_materials: [{material, before, after, merges: [{node, kind, slabs, dropped}]}]} \| null}` (the last frame drawn, by a capture or the editor; `gpu_ms` may be null for a moment; a material with more closures than the budget is drawn simplified and listed here) |
| `viewport.pick` | | `{view, x, y, width, height, world?}` → `{entity, distance}` (projects with `[render]`) |
| `viewport.drop` | | `{view, x, y, width, height, world?}` → `{position, entity}` (where a dragged-in asset lands: the mesh under the pointer, else the ground plane y = 0) |
| `viewport.present` | | `{view, width, height, world?, seq?, input_time_ms?}` → `{presented}` (the editor's native viewport only); **editor only, not an MCP tool** |
| `viewport.navigate` | | `{orbit?, pan?, zoom?, preset?, focus?, toggle_ortho?}` → `{view}` (moves the editor's camera; **editor only, not an MCP tool**) |
| `viewport.overlays` | | `{selection?, gizmo?: {entity, hovered?}, grid?, scale?}` → `{overlays}` (what the engine draws over the editor viewport); **editor only, not an MCP tool** |
| `viewport.gizmo` | yes | `{action: begin\|move\|end\|cancel, entity?, axis?, x?, y?, width?, height?}` → `{translation, axis}` (Move-gizmo drags; a begin without an axis grabs the shown gizmo's handle under the pointer, or misses); **editor only, not an MCP tool** |
| `viewport.stats` | | `{clear?}` → `{frames: [{seq, input_time_ms, cpu_ms, submitted_ms, presented_ms}]}` (recent native-viewport frames; times in Unix ms); **editor only, not an MCP tool** |
| `log.tail` | | `{since?, level?, limit?}` → `{records, next}` |
| `events.subscribe` / `events.unsubscribe` | | `{topics: ["edit" \| "scene" \| "sim" \| "pending" \| "log" \| "studio" \| "asset"]}` → `{topics}` (WebSocket only; not an MCP tool) |
| `task.create` | yes | `{title, role, description?, priority?, acceptance?: [{assert}], depends_on?, links?}` → `{task}` (projects with `[studio]`; the role must be enabled, see `studio.roster`) |
| `task.get` | | `{id}` → `{task, history}` (history comes from the Studio journal, oldest first) |
| `task.list` | | `{status?, role?, assignee?}` → `{tasks}` |
| `task.claim` | yes | `{id}` → `{task}` (todo or rejected → in_progress, assigned to you) |
| `task.update` | yes | `{id, status?, title?, description?, priority?, acceptance?, links?, depends_on?, assignee?}` → `{task}` (in_progress → in_review when your work is ready) |
| `task.comment` | yes | `{id, text}` → `{task}` (a note: what you did, evidence, questions) |
| `task.approve` | yes | `{id, comment?}` → `{task}` (in_review → done; only reviewers the team's gates allow, else `NotPermitted`) |
| `task.reject` | yes | `{id, comment}` → `{task}` (in_review → rejected; blocked for a human after `max_attempts_per_task` rejections) |
| `studio.roster` | | `{}` → `{team, director, main_session_actor, roles: [{role, mcp_tools, write, ..., color, agent_color}], disabled: [{role, reason}]}` (`color` is the editor's `#rrggbb` for the role; `agent_color` is the Claude Code color its agent file gets) |
| `studio.status` | | `{}` → `{tasks: {<status>: count}, pending_approvals, instances: [{role, slot, agent}], last_sync: {ts_ms, actor, changed} \| null, budgets}` (`budgets` are the team's limits; usage is counted by `concordia mcp`) |
| `studio.activity` | | `{since?, actor?, role?, task?, kind?, limit?}` → `{entries, next}` (task changes, reviews and agents' file edits, oldest first) |
| `studio.sync` | yes | `{check?, force?}` → `{changes: [{path, action, diff?}], refused, stale}` (regenerates `.claude/agents`, `.claude/skills`, the settings hooks, `.mcp.json` and the `CLAUDE.md` block from `studio/`; usually run by a human) |
| `studio.doctor` | | `{}` → `{checks: [{id, status: ok\|note\|fail, text}], ok}` (Claude Code installed and logged in, an API key noted but never needed, `concordia` on PATH, Claude Code files in sync); **editor only, not an MCP tool** |
| `studio.ask` | yes | `{to, text, task?, refs?}` → `{question}` (ask an enabled role or `human`; `text` ≤ 280 chars; returns at once with `Q-0001`: don't wait, carry on or report you're blocked; see `concordia-team-comms`) |
| `studio.answer` | yes | `{id, text}` → `{question}` (≤ 500 chars; the addressed role, the director or a human; an escalated question takes only a human answer; the first answer closes it) |
| `studio.escalate` | yes | `{id, reason}` → `{question}` (director only, when no role can answer; re-addresses it to `human` and blocks its unfinished task until a human answers) |
| `studio.questions` | | `{status?, to?, from?, task?, since?, limit?}` → `{questions: [{id, from, to, task, refs, text, asked_ms, status, answer?, escalation?}], next}` (oldest first) |

`events.subscribe` makes the WebSocket connection receive JSON-RPC notifications, `{"method": "events.event", "params": {"topic", ...}}`:
- `edit`: one per committed transaction, undo or redo, with `method`, `txn`, `label`, `actor`, `entities` and `undo`;
- `scene`: `event` is `opened`, `new` or `saved`, plus the `path`;
- `sim`: `method`, `tick` and `state`;
- `pending`: one per call that changes the world inside an open transaction (`pending: true`, with `method`, `txn`, `label`, `actor` and that call's `entities`), and one when a transaction is aborted (`aborted: true` and its `entities`). Use it to show work in progress, such as a gizmo drag, before the commit;
- `log`: `latest` (the newest log record's seq) after a call wrote log records; fetch them with `log.tail`;
- `asset`: one per file `material.set` writes or `asset.import` imports, with its `path`, `id` and `actor` (the editor redraws and refreshes the Inspector);
- `studio` (projects with `[studio]`): one per new Studio journal entry (task changes, reviews, file edits, and `question_asked`, `question_answered` and `question_escalated`), with its `seq`, `ts_ms`, `actor`, `kind` and the kind's fields.

Edits made by other clients arrive too, which is how the editor sees agents work. Over stdio or MCP, poll `log.tail` instead. If a client falls more than 1,024 events behind, newer events are dropped, so re-read state with `query.run`.

`capture.screenshot` renders offscreen at the requested size (default 768×432) from the highest-priority `Camera`, unless `camera` names one. MCP clients receive `image` as PNG image content and the other fields as text. Pass `world: "play"` after `sim.step` to see the simulation. To be visible, a scene needs a `Transform` + `Camera` entity, `MeshRenderer` entities (e.g. `{"mesh": "builtin:mesh/cube", "material": "builtin:material/default"}`) and usually a `DirectionalLight`.

`query.run` `where` entries are `{"path": "Transform/translation/0", "op": "gt", "value": 4}` with `op` one of `eq ne lt le gt ge contains`. Rows are sorted by id; pass `next_cursor` back as `cursor` for the next page (default limit 100).

## Errors
Errors are JSON-RPC errors whose `data` says what to do next (ADR-0031): `kind`, `suggestions` (best first), and where useful `pointer`, `expected` (the schema at the pointer), `example`, `see` (a `concordia://` resource), plus kind-specific fields such as `holder` for `TxnConflict`.

| Code | Kind | Typical fix |
|---|---|---|
| −32001 | `EntityNotFound` | find ids with `query.run {"with": ["Name"]}` |
| −32002 | `ComponentNotRegistered` | use a suggested name; list names with `schema.get {}` |
| −32003 | `ValueInvalid` | fix the value at `pointer` to match `expected` |
| −32004 | `ActorRequired` / `ActorMalformed` | pass `"actor": "agent:<role>"` |
| −32005 | `TxnConflict` | wait for `holder` to commit or abort, then retry |
| −32006 | `NothingToUndo` / `NothingToRedo` | check `txn.history` |
| −32007 | `SceneIo` | check the path and permissions |
| −32008 | `SceneParse` | fix the file at `line`/`col`, or run `concordia fmt` |
| −32009 | `NoSceneOpen` | `scene.new` or `scene.open` |
| −32010 | `SceneDirty` | `scene.save`, or pass `"discard": true` |
| −32011 | `NotPlaying` | `sim.step` or `sim.play` first |
| −32012 | `TxnNotFound` | use the id from `txn.begin`; it may have timed out |
| −32013 | `GpuUnavailable` | run `concordia gpu-probe`; rendering needs Metal (macOS) in M1 |
| −32014 | `TaskNotFound` | list tasks with `task_list` |
| −32015 | `TaskTransition` | follow todo → in_progress (`task_claim`) → in_review → done (`task_approve`) |
| −32016 | `NotPermitted` | a Studio gate or your role's scope refuses this; see `studio_roster` |
| −32017 | `EngineExited` | the shared engine's host (editor or another server) exited; call again to reconnect, then `scene_open` your scene (unsaved edits are gone) |
| −32018 | `ScriptFailed` | fix the module the message names, rebuild it, then `script.reload` |
| −32019 | `ReplayMismatch` | open the scene and build the replay was recorded with (see `mismatches`), or pass `"force": true` |
| −32602 | `InvalidParams` | match `expected` at `pointer` |

## Journal
Every commit, undo, redo, abort and save is appended to `.concordia/journal/<scene-id>.cmdlog.jsonl` with its actor (format: `crates/concordia-command/AGENTS.md`).

## Transports
- stdio: one JSON-RPC message per line, one session per process.
- MCP with Agent Studio (`[studio]`): every engine call made from Claude Code is stamped by the Studio hook with your role. Each agent instance gets its own session and actor (`agent:<role>#n`). Calls outside your role's tools or paths fail with `NotPermitted`, and so do edits when the hooks aren't running. Don't pass `actor`; it is ignored.
- WebSocket: `ws://127.0.0.1:<port>/?token=<token>`, localhost only; the URL and token are in `.concordia/headless.json` (mode 0600). Browser origins other than localhost are rejected.
- One engine per project (ADR-0082): `concordia mcp`, the editor and `concordia headless --ws` attach to the live engine named in `.concordia/headless.json`, or host one and publish that file. Your edits show live in the editor, and the editor's in yours. If the host exits, the next call fails with `EngineExited`; the call after it reconnects.
