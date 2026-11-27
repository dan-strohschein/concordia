---
name: concordia-author-graph
description: "Authors gameplay graphs (.graph.ron Event graphs) as text through the engine: lists nodes with graph_list_nodes, edits with graph_apply, fixes graph_check diagnostics, compiles, attaches the graph to entities with component_set, runs it and traces what it did with graph_trace."
when_to_use: "Use when a mechanic is simple per-entity logic (on tick or on a signal: read and set the entity's own components, branch, raise a signal) and fits a graph better than a script, or when a task names a .graph.ron file."
user-invocable: false
---
# Authoring a gameplay graph

A graph is a `.graph.ron` file under `graphs/`: nodes (by UUIDv7 id) joined by data links and exec (control-flow) links. An **Event** graph runs on every entity that has its `graph.<name>.Vars` component: each `event.on_tick` node every tick, each `on.<Signal>` node once per delivered signal. Graphs compile to sandboxed WebAssembly and reload when they change. Humans edit the same file in the editor's Graph panel; every edit, yours or theirs, is one undo step in the command log with its actor.

## Steps
1. See what you can use: `mcp__concordia__graph_list_nodes {}`. Each op lists its pins and types: `get.<C>.<field>` / `set.<C>.<field>` for components, `raise.<S>` / `on.<S>` for signals, `math.*`, `logic.*`, `conv.*`, `vector.*`, `branch`, `sequence`, `gate`, `entity.self`, `var.get.<v>` / `var.set.<v>`.
2. Write it with `mcp__concordia__graph_apply`: a path, a `kind` for a new file, and edits in order. Give nodes UUIDv7 ids (or omit `id` and read `added`):
   ```json
   {"path": "graphs/regen.graph.ron", "kind": "Event", "edits": [
     {"AddNode": {"id": "0193b0f2-0000-7000-8000-000000000001", "op": "event.on_tick", "at": [0, 0]}},
     {"AddNode": {"id": "0193b0f2-0000-7000-8000-000000000002", "op": "get.Health.value", "at": null}},
     {"AddNode": {"id": "0193b0f2-0000-7000-8000-000000000003", "op": "math.add", "at": null}},
     {"AddNode": {"id": "0193b0f2-0000-7000-8000-000000000004", "op": "set.Health.value", "at": null}},
     {"Connect": {"from": "0193b0f2-0000-7000-8000-000000000002", "from_pin": "value", "to": "0193b0f2-0000-7000-8000-000000000003", "to_pin": "a"}},
     {"Connect": {"from": "0193b0f2-0000-7000-8000-000000000001", "from_pin": "dt", "to": "0193b0f2-0000-7000-8000-000000000003", "to_pin": "b"}},
     {"Connect": {"from": "0193b0f2-0000-7000-8000-000000000003", "from_pin": "out", "to": "0193b0f2-0000-7000-8000-000000000004", "to_pin": "value"}},
     {"ConnectExec": {"from": "0193b0f2-0000-7000-8000-000000000001", "from_pin": "out", "to": "0193b0f2-0000-7000-8000-000000000004", "to_pin": "in"}}
   ]}
   ```
   Other edits: `RemoveNode`, `Disconnect`, `DisconnectExec`, `SetValue` (a constant: `{"Float": 2.0}`, `{"Int": 3}`, `{"Bool": true}`, `{"Vec3": [0, 1, 0]}`), `SetVariable` (`{"name": "ticks", "ty": "i32", "default": {"Int": 0}}`), `RemoveVariable`, `SetPosition`. Build in several calls; each returns the graph's `diagnostics`.
3. Fix what the diagnostics name: each has a `code`, the `node_id` and `pin`, and a `suggestion` with the closest valid names. `mcp__concordia__graph_check {"path": ...}` checks again; `mcp__concordia__graph_read {"path": ...}` shows the file and who last changed each node (read it before editing a graph someone else is working on).
4. Compile it: `mcp__concordia__graph_compile {"path": ...}` lists its processors, what they read and write, and the `vars` component that attaches it. Nodes it can't compile yet (other entities' components, `query.each`, `for_each`, exec loops) are `unsupported_node` diagnostics: keep the graph to the entity's own state, or ask for a script.
5. Attach it to an entity: `mcp__concordia__component_set {"entity": "<id>", "component": "graph.regen.Vars", "value": {}}` (`{}` gives every variable its default). The graph must be listed in `concordia.toml` as `[[scripts.module]] kind = "graph"`; ask the director if it isn't.
6. Run and check it: `mcp__concordia__sim_step {"ticks": 60}`, then `mcp__concordia__query_run {"with": ["graph.regen.Vars"], "select": ["Health"], "world": "play"}`. See what it did: `mcp__concordia__graph_trace {"module": "regen", "entity": "<id>", "ticks": 2}` (the statements that ran, in order, and every pin's value).
7. Prove it with an assertion: `mcp__concordia__assert_eval {"assertions": ["within 120 ticks: name(\"Hero\").Health.value >= 10"]}`.

## Rules
- Edit graphs through `graph_apply`, not by rewriting the file: the edit is undoable, attributed to you, and checked.
- Keep a graph small and per-entity; a graph that needs other entities, loops or heavy math is a script (`concordia-scripting`).
- A reload keeps variables by name and type; a graph that starts touching components it didn't before needs a new play session (`sim_stop`, then step again).
