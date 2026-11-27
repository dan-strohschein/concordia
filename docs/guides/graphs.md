# Gameplay graphs (for agents)

A graph is a `.graph.ron` file: nodes joined by data links and exec (control-flow) links. You write it as text, check it, and fix what the diagnostics name. (M4 4.9 gives the format, the node library and the checker; 4.10a compiles pure Function graphs to WASM; running graphs in the engine and the MCP tools follow in 4.10b–c.)

## Kinds
| Kind | What it is | Rule |
|---|---|---|
| `Event` | Entry points run by the engine | At least one event node (`event.on_tick`, `on.<Signal>`) |
| `Function` | Callable; `signature.pure` forbids side effects | Its outputs come from one `graph.output` |
| `Behavior` | A behavior tree | Exactly one `behavior.root` |
| `Utility` | A consideration: a score in [0, 1] | Pure; each f32 output straight from `utility.curve` or `math.saturate` |

## The format
```ron
Graph(
    exec: [("<on_tick id>", "out", "<branch id>", "in")],
    id: "<UUIDv7>",
    kind: Event,
    layout: {},
    nodes: {
        "<UUIDv7>": (inputs: {"cond": Link("<other id>", "out")}, op: "branch"),
        "<UUIDv7>": (inputs: {"b": Value(Float(10.0))}, op: "math.lt"),
    },
    signature: (inputs: [], outputs: [], pure: false),
    variables: [(default: Some(Int(0)), name: "ticks", ty: "i32")],
    version: 1,
)
```
- Ids are UUIDv7 (text order is creation order). An input is `Link(node, output pin)` or `Value(...)`: `Bool`, `Int`, `Float`, `Text`, `Vec2`–`Vec4`. Integers also fit `f32`; text fits strings, asset references and enum variants.
- `layout` holds editor positions only; it never changes the graph's semantic hash. The printer sorts everything, so diffs stay small.
- Examples: `crates/concordia-graph/tests/graphs/ok` (ten that check clean) and `broken` (one problem each).

## Nodes
- Components: `get.<C>`, `set.<C>`, `get.<C>.<field>`, `set.<C>.<field>`; `entity` is optional (the graph's own entity by default).
- Signals: `raise.<S>` (inputs `target` and each field) and `on.<S>` (outputs `entity`, `signal` and each field; a field named like a fixed pin is `field_<name>`).
- Variables: `var.get.<name>` (output `value`), `var.set.<name>` (input `value`), typed by `variables`.
- Signature: `graph.input` outputs and `graph.output` inputs are the signature's pins.
- Core: flow (`branch`, `sequence`, `for_each`, `gate`, `query.each`), `entity.self`, `math.*` (generic over `Numeric` = i32, i64, f32, Vec2–4, or `Scalar` = i32, i64, f32), `logic.*`, `conv.*`, `vector.*`, `behavior.*`, `utility.curve`. The full list with pin types is the core snapshot `crates/concordia-graph/tests/snapshots/library__core_library.snap`.

## Running graphs
- List the graph in `concordia.toml`: `[[scripts.module]] kind = "graph", path = "graphs/enemy.graph.ron"`. It compiles to WebAssembly when the project loads and runs like a script module (same sandbox, fuel and faults).
- Attach it to an entity by adding `graph.<name>.Vars` (`{}` gives every variable its default). Each event node is a processor: `event.on_tick` every tick, `on.<Signal>` per delivered signal, in order.
- Edit the file, then `script.reload`: variables keep their values (by name and type), new ones start at their defaults. A graph that doesn't check keeps the old version running and the error lists the diagnostics. Reading or writing components the graph didn't touch before needs a new play session (as for scripts).
- Compiled in 4.10: the graph's own entity (its components and fields, variables, `dt`, signal fields), math, logic, conversions, vectors, `branch`, `sequence`, `gate`, `set`, `raise` on itself. Not yet (`unsupported_node`): other entities, `query.each`, `for_each`, exec loops.

## Diagnostics
Each is `{ code, node_id, pin, message, suggestion }` (JSON Schema from `concordia-reflect`). Codes: `unknown_op`, `unknown_variable`, `unknown_type`, `unknown_pin`, `unknown_node`, `type_mismatch`, `unconnected_input`, `data_cycle`, `impure_in_pure`, `generic_unresolved`, `kind_rule`, `exec_mismatch`, and from the compiler `unsupported_node` (a node or type it can't lower yet). Fix the named node and pin; a `suggestion` lists the closest valid names.
