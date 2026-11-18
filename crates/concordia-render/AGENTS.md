# concordia-render: agent guide

## Purpose
The renderer (ADR-0040): the render graph, forward PBR, cameras, headless capture and `capture.screenshot`. Phase 1.5a holds the render graph; the GPU renderer arrives in 1.5b.

## Public entry points
- `graph::RenderGraph`: `create_texture` (transient), `import_texture` (swapchain, capture target, history), `add_pass` / `add_pass_recorded` with `PassBuilder::uses(texture, Use)` and `side_effect()`.
- `RenderGraph::compile(footprint)` → `CompiledGraph`: `pass_names`, `barriers_before`, `final_barriers`, `placements`, `heap_size`, `memory_plan`, `to_dot`, `execute`.

## Invariants
- Compiling is CPU-only and deterministic: Kahn's order with ties broken by insertion index.
- A pass runs only if it has a side effect, writes an imported texture, or produces something a running pass reads.
- Writers of one texture keep insertion order; readers run after all writers.
- A transient's first use transitions from `Undefined` (marked `aliasing` when it reuses memory). Read-after-read in the same layout gets no barrier. Imported textures end in their `final_layout`.
- Transient memory is placed first-fit by lifetime start, reusing a slot only after its occupant's last use.
- Reading a transient that no pass writes is an error naming the texture.

## How to test
`cargo nextest run --workspace -E 'package(concordia-render)'` (CPU only; insta snapshots for the memory plan and the DOT dump).

## Common tasks
- **Inspect a frame:** `compiled.to_dot(&graph)` gives Graphviz text that agents can read; `memory_plan()` lists transient placements.
- **Add a use kind:** add a `Use` variant, map it in `Use::access` and `dot::name`, and add a barrier test.

## Don'ts
- Don't record barriers by hand inside passes; declare uses and let the graph derive them.
- Don't make compilation depend on hash-map iteration order.

## References
- Yuriy O'Donnell, "FrameGraph: Extensible Rendering Architecture in Frostbite" (GDC 2017).
- Hans-Kristian Arntzen, "Render graphs and Vulkan — a deep dive" (themaister.net, 2017).

## Status
M1 phase 1.5a (render graph). 1.5b adds the GPU renderer and capture.

## Engineering rules
These apply to every change, by humans and agents alike.
1. **TDD on every phase.** Write the tests first, see them fail, implement, then refactor.
2. **SOLID.** Depend on abstractions at crate boundaries; extend through traits rather than by editing callers.
3. **No duplicate functionality.** Before writing any implementation, search for existing functionality (grep, `cargo doc`, crate `AGENTS.md` files). Extend or refactor what exists; never write a parallel version.
4. **Single responsibility at every level:** function, type, module and crate.
5. **Small functions** that do one thing.
6. **Validate parameters.** Every function checks its inputs:
   - Rust: encode invariants in types (newtypes, enums, `NonZero*`); validate at boundaries and return `Result` with a descriptive error; use `debug_assert!` for internal invariants. Public APIs never panic on bad input.
   - TypeScript: strict types, plus runtime validation at API and IPC boundaries.
7. **Short comments.** One-line doc comments on public items; no essays, and no comments that restate the code.
8. **Idiomatic code.**
   - Rust: follow the Rust API Guidelines and keep it clippy-clean; `thiserror` in libraries, `anyhow` only in binaries and tests.
   - Editor UI: idiomatic React and TypeScript, with function components, hooks, strict mode, ESLint and Prettier.
