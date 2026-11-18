# Concordia Engine API: guide for agents

The Engine API is how every client (agents, the CLI, the editor, tests) drives the engine. Over MCP the same methods appear as tools named with `_` instead of `.` (`entity.spawn` → `entity_spawn`). Everything here works headless and needs no API key.

## Workflow
1. `session.open {"client": "claude-code", "actor": "agent:claude-code"}` sets the default actor for this session. (The MCP server does this for you.)
2. `scene.new {"path": "scenes/arena.scene.ron", "name": "Arena"}` or `scene.open {"path": ...}`.
3. Discover shapes with `schema.get {}` (index), `schema.get {"name": "Transform"}` (component) or `schema.get {"name": "entity.spawn"}` (method).
4. Edit with `entity.spawn`, `component.set` (whole value), `component.patch` (merge patch, e.g. `{"scale": [2, 2, 2]}`), `component.remove`, `entity.despawn`. Group edits with `txn.begin {"label": ...}` … `txn.commit {"txn": ...}` so they undo as one step.
5. Check your work with `entity.get`, `component.get` and `query.run`.
6. `scene.save {}` writes canonical RON. `txn.undo` / `txn.redo` work across restarts when the file is unchanged.
7. Simulate without touching the scene: `sim.step {"ticks": 60}`, then read with `"world": "play"`; `sim.stop {}` discards the play world.

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
| `entity.spawn` | yes | `{components, id?}` → `{id}` |
| `entity.despawn` | yes | `{id}` → `{}` |
| `entity.get` | | `{id, world?}` → `{id, components}` |
| `component.get` | | `{entity, component, world?}` → `{value}` |
| `component.set` | yes | `{entity, component, value}` → `{previous}` |
| `component.patch` | yes | `{entity, component, patch}` → `{value}` |
| `component.remove` | yes | `{entity, component}` → `{previous}` |
| `query.run` | | `{with, without?, where?, select?, world?, limit?, cursor?}` → `{rows: [{id, components}], next_cursor?}` |
| `schema.get` | | `{name?}` → `{schema}` |
| `txn.begin` | yes | `{label}` → `{txn}` |
| `txn.commit` / `txn.abort` | yes | `{txn}` → `{ops}` |
| `txn.undo` / `txn.redo` | yes | `{}` → `{txn, label, by}` |
| `txn.history` | | `{limit?}` → `{entries}` |
| `sim.play` / `sim.pause` / `sim.stop` | | `{rate_hz?}` / `{}` → `{state, tick}` |
| `sim.step` | | `{ticks}` → `{tick, world_hash}` |
| `capture.screenshot` | | `{width?, height?, camera? \| view?, world?}` → `{image, width, height, sim_tick, frame}` (projects with `[render]`) |
| `viewport.pick` | | `{view, x, y, width, height, world?}` → `{entity, distance}` (projects with `[render]`) |
| `viewport.present` | | `{view, width, height, world?}` → `{presented}` (the editor's native viewport only) |
| `log.tail` | | `{since?, level?, limit?}` → `{records, next}` |
| `events.subscribe` / `events.unsubscribe` | | `{topics: ["edit" \| "scene" \| "sim"]}` → `{topics}` (WebSocket only; not an MCP tool) |

`events.subscribe` makes the WebSocket connection receive JSON-RPC notifications, `{"method": "events.event", "params": {"topic", ...}}`:
- `edit`: one per committed transaction, undo or redo, with `method`, `txn`, `label`, `actor`, `entities` and `undo`;
- `scene`: `event` is `opened`, `new` or `saved`, plus the `path`;
- `sim`: `method`, `tick` and `state`.

Edits made by other clients arrive too, which is how the editor sees agents work. Over stdio or MCP, poll `log.tail` instead. If a client falls more than 1,024 events behind, newer events are dropped, so re-read state with `query.run`.

`capture.screenshot` renders offscreen at the requested size (default 1280×720) from the highest-priority `Camera`, unless `camera` names one. MCP clients receive `image` as PNG image content and the other fields as text. Pass `world: "play"` after `sim.step` to see the simulation. To be visible, a scene needs a `Transform` + `Camera` entity, `MeshRenderer` entities (e.g. `{"mesh": "builtin:mesh/cube", "material": "builtin:material/default"}`) and usually a `DirectionalLight`.

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
| −32602 | `InvalidParams` | match `expected` at `pointer` |

## Journal
Every commit, undo, redo, abort and save is appended to `.concordia/journal/<scene-id>.cmdlog.jsonl` with its actor (format: `crates/concordia-command/AGENTS.md`).

## Transports
- stdio: one JSON-RPC message per line, one session per process.
- WebSocket: `ws://127.0.0.1:<port>/?token=<token>`, localhost only; the URL and token are in `.concordia/headless.json` (mode 0600). Browser origins other than localhost are rejected.
