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
2. Bring in a model: `asset.import {"path": "models/crate.glb"}`, then `prefab.instantiate {"asset": "asset:<id>", "translation": [0, 0, 0]}` (one undo step).
3. Add ready-made entities: `entity.templates {}` lists them; `entity.create {"template": "Cube", "translation": [0, 0.5, 0]}`, `{"template": "Directional Light"}`, `{"template": "Camera"}`. Group with `"parent"`, copy with `entity.duplicate`, regroup with `entity.reparent`. For physics, `component.add` `RigidBody` and `Collider` (a 1 m box by default), then `component.patch` the shape; for a sky, `entity.spawn` an entity with `Environment` (no `Transform` needed) and an imported `.hdr` as its `map`; sky light is scaled by the camera's exposure, so under daylight exposure (EV 15) its `intensity` is in the thousands. Materials are `.material.ron` files (`MaterialData` in RON): `asset.import` one and set it as a `MeshRenderer`'s `material`. (`entity.spawn` still takes raw components.)
4. Look at it: `capture.screenshot {}` returns an image (768 × 432 by default; pass `width`/`height` for detail, `camera` or `view` to choose the viewpoint, `"world": "play"` to see the simulation, `view_mode` for a debug view such as `"wireframe"`, `"normals"`, `"overdraw"` or `"light_clusters"`). `render.stats {}` then reports that frame's cost (draws, triangles, CPU and GPU ms, GPU memory): use it to find what makes a scene slow or why something is missing. `viewport.pick` tells you which entity is at a pixel of that image; `viewport.drop` where something placed there would land.
5. Run it: `sim.step {"ticks": 120}`, `input.inject` to press actions, `physics.raycast` to probe, then read with `"world": "play"`.
6. `scene.save {}`.

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
| `asset.import` | | `{path}` → `{id, cached, sub_assets}` (import or re-import a project file; idempotent; refer to it as `"asset:<id>"`) |
| `asset.list` | | `{}` → `{assets: [{id, path, importer, sub_assets}]}` |
| `entity.spawn` | yes | `{components, id?}` → `{id}` |
| `entity.despawn` | yes | `{id, recursive?}` → `{}` (`recursive: true` also removes all descendants, one undo step) |
| `entity.templates` | | `{}` → `{templates: [{name, description, components}]}` (ready-made entities: Empty, Cube, Sphere, Plane, Camera, Directional/Point/Spot Light, plus any from plugins) |
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
| `input.inject` | | `{events: [{tick?, event}], actor?}` → `{accepted, next_tick}` (raw input into the play world, e.g. `{"event": {"Key": {"key": "Space", "pressed": true}}}`; the project's `.input.ron` maps it to actions) |
| `physics.raycast` | | `{origin, direction, max_distance, layers?, world?}` → `{hit: {entity, point, normal, distance} \| null}` |
| `capture.screenshot` | | `{width?, height?, camera? \| view?, world?, overlays?, view_mode?}` → `{image, width, height, sim_tick, frame}` (projects with `[render]`; `overlays: true` adds the editor overlays; `view_mode`: `lit` (default), `unlit`, `wireframe`, `normals`, `albedo`, `roughness_metallic`, `overdraw`, `shadow_cascades`, `light_clusters`) |
| `render.stats` | | `{}` → `{frame: {frame, width, height, view_mode, cpu_ms, gpu_ms, draws, culled, draw_calls, triangles, punctual_lights, shadows_cached, memory_bytes} \| null}` (the last frame drawn, by a capture or the editor; `gpu_ms` may be null for a moment) |
| `viewport.pick` | | `{view, x, y, width, height, world?}` → `{entity, distance}` (projects with `[render]`) |
| `viewport.drop` | | `{view, x, y, width, height, world?}` → `{position, entity}` (where a dragged-in asset lands: the mesh under the pointer, else the ground plane y = 0) |
| `viewport.present` | | `{view, width, height, world?, seq?, input_time_ms?}` → `{presented}` (the editor's native viewport only); **editor only, not an MCP tool** |
| `viewport.navigate` | | `{orbit?, pan?, zoom?, preset?, focus?, toggle_ortho?}` → `{view}` (moves the editor's camera; **editor only, not an MCP tool**) |
| `viewport.overlays` | | `{selection?, gizmo?: {entity, hovered?}, grid?, scale?}` → `{overlays}` (what the engine draws over the editor viewport); **editor only, not an MCP tool** |
| `viewport.gizmo` | yes | `{action: begin\|move\|end\|cancel, entity?, axis?, x?, y?, width?, height?}` → `{translation, axis}` (Move-gizmo drags; a begin without an axis grabs the shown gizmo's handle under the pointer, or misses); **editor only, not an MCP tool** |
| `viewport.stats` | | `{clear?}` → `{frames: [{seq, input_time_ms, cpu_ms, submitted_ms, presented_ms}]}` (recent native-viewport frames; times in Unix ms); **editor only, not an MCP tool** |
| `log.tail` | | `{since?, level?, limit?}` → `{records, next}` |
| `events.subscribe` / `events.unsubscribe` | | `{topics: ["edit" \| "scene" \| "sim" \| "pending" \| "log" \| "studio"]}` → `{topics}` (WebSocket only; not an MCP tool) |
| `task.create` | yes | `{title, role, description?, priority?, acceptance?: [{assert}], depends_on?, links?}` → `{task}` (projects with `[studio]`; the role must be enabled, see `studio.roster`) |
| `task.get` | | `{id}` → `{task, history}` (history comes from the Studio journal, oldest first) |
| `task.list` | | `{status?, role?, assignee?}` → `{tasks}` |
| `task.claim` | yes | `{id}` → `{task}` (todo or rejected → in_progress, assigned to you) |
| `task.update` | yes | `{id, status?, title?, description?, priority?, acceptance?, links?, depends_on?, assignee?}` → `{task}` (in_progress → in_review when your work is ready) |
| `task.comment` | yes | `{id, text}` → `{task}` (a note: what you did, evidence, questions) |
| `task.approve` | yes | `{id, comment?}` → `{task}` (in_review → done; only reviewers the team's gates allow, else `NotPermitted`) |
| `task.reject` | yes | `{id, comment}` → `{task}` (in_review → rejected; blocked for a human after `max_attempts_per_task` rejections) |
| `studio.roster` | | `{}` → `{team, director, main_session_actor, roles: [{role, mcp_tools, write, ...}], disabled: [{role, reason}]}` |
| `studio.status` | | `{}` → `{tasks: {<status>: count}, pending_approvals}` |
| `studio.activity` | | `{since?, actor?, role?, task?, kind?, limit?}` → `{entries, next}` (task changes, reviews and agents' file edits, oldest first) |
| `studio.sync` | yes | `{check?, force?}` → `{changes: [{path, action, diff?}], refused, stale}` (regenerates `.claude/agents`, `.claude/skills`, the settings hooks, `.mcp.json` and the `CLAUDE.md` block from `studio/`; usually run by a human) |

`events.subscribe` makes the WebSocket connection receive JSON-RPC notifications, `{"method": "events.event", "params": {"topic", ...}}`:
- `edit`: one per committed transaction, undo or redo, with `method`, `txn`, `label`, `actor`, `entities` and `undo`;
- `scene`: `event` is `opened`, `new` or `saved`, plus the `path`;
- `sim`: `method`, `tick` and `state`;
- `pending`: one per call that changes the world inside an open transaction (`pending: true`, with `method`, `txn`, `label`, `actor` and that call's `entities`), and one when a transaction is aborted (`aborted: true` and its `entities`). Use it to show work in progress, such as a gizmo drag, before the commit;
- `log`: `latest` (the newest log record's seq) after a call wrote log records; fetch them with `log.tail`.

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
| −32602 | `InvalidParams` | match `expected` at `pointer` |

## Journal
Every commit, undo, redo, abort and save is appended to `.concordia/journal/<scene-id>.cmdlog.jsonl` with its actor (format: `crates/concordia-command/AGENTS.md`).

## Transports
- stdio: one JSON-RPC message per line, one session per process.
- MCP with Agent Studio (`[studio]`): every engine call made from Claude Code is stamped by the Studio hook with your role. Each agent instance gets its own session and actor (`agent:<role>#n`). Calls outside your role's tools or paths fail with `NotPermitted`, and so do edits when the hooks aren't running. Don't pass `actor`; it is ignored.
- WebSocket: `ws://127.0.0.1:<port>/?token=<token>`, localhost only; the URL and token are in `.concordia/headless.json` (mode 0600). Browser origins other than localhost are rejected.
- One engine per project (ADR-0082): `concordia mcp`, the editor and `concordia headless --ws` attach to the live engine named in `.concordia/headless.json`, or host one and publish that file. Your edits show live in the editor, and the editor's in yours. If the host exits, the next call fails with `EngineExited`; the call after it reconnects.
