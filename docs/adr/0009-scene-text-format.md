# ADR-0009: Scene text format (canonical RON)
- Status: Accepted
- Date: 2026-11-17

## Context
Scenes and prefabs are the source of truth (principle 2). They must diff and merge cleanly, be readable and editable by people and LLM agents, keep stable ids, and round-trip byte for byte so tools never churn files.

## Decision
Use **RON** with one canonical form, written and checked by `concordia-scene` (`concordia fmt`):
- `Scene(format: 1, id, name, entities: [...])` and `Prefab(format: 1, id, name, root, entities: [...])`; extensions `.scene.ron`, `.prefab.ron`.
- One `Entity(id: "...", components: {...})` block per entity, sorted by id (UUIDv7, so new entities append and merge cleanly).
- Component keys are registry stable names, sorted; struct fields sorted alphabetically; 4-space indent; trailing commas; final newline.
- Floats use the shortest round-trip form for their type (`f32` or `f64`) and always contain a `.` (`1.0`, `1.0e-7`); `-0.0` is preserved; NaN and infinity are rejected on save with the entity, component and JSON Pointer.
- Strings use minimal escaping. `Option` is `Some(x)` / `None`; enums are `Variant`, `Variant(x)`, `Variant(a, b)`, `Variant(field: x)`; tuples, arrays and glam vectors are `(a, b, c)`, inline when every item fits on one line.
- A header comment is regenerated; other comments are not preserved (M0).
- JSON stays the Engine API wire format.

**Implementation:** the reader and writer are our own and type-directed rather than the `ron` crate. Text is parsed into a positioned syntax tree, converted to serde's JSON form using `concordia-reflect` type info, then validated and deserialized; writing goes the other way. This gives exact canonical output (the shape says whether a number is `f32`), errors with line, column, JSON Pointer and the expected shape, and "did you mean" suggestions for component names, with no extra dependency. The grammar follows the public RON specification.

## Consequences
- Files are deterministic, diffable and stable under load/save; `format(format(x)) == format(x)` is tested, including with property tests.
- RON is less familiar to LLMs than JSON. Mitigations: agents edit through MCP rather than raw text; schemas and an annotated example are served as MCP resources; `concordia fmt` normalizes hand edits; errors carry positions and suggestions.
- We maintain a small parser (fuzzed by `reader_never_panics_*` property tests; nesting depth is capped).
- **Revisit trigger:** if the M0 live gate or M1/M2 agent work shows agents editing raw RON incorrectly at a meaningful rate.

## Alternatives considered
- JSON: the highest LLM familiarity, but no comments or trailing commas, so diffs and hand edits are noisier. It remains the API wire format.
- JSON5: a weaker Rust ecosystem.
- YAML: indentation-sensitive merges, implicit typing pitfalls, a very large spec.
- TOML: awkward for deeply nested arrays of tables.
- KDL: low LLM familiarity.
- A custom format: no external tooling or familiarity.
- The `ron` crate for reading and writing: its serializer cannot sort keys or know `f32` from `f64` after JSON conversion, and its errors lack JSON Pointers.
