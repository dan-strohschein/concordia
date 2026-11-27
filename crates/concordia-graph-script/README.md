# concordia-graph-script

Runs gameplay graphs inside the engine. A `.graph.ron` Event graph is checked, compiled to WebAssembly (or run by the interpreter) and loaded as a script module, so it schedules, faults and reloads exactly like a script. Attaching a graph to an entity is adding its `graph.<name>.Vars` component.

See `AGENTS.md` for invariants, tests and references.
