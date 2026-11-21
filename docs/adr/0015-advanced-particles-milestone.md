# ADR-0015: Advanced particles milestone (M11): Sparx
- Status: Proposed (author's direction, 2026-11-20)
- Date: 2026-11-20
- Builds on: ADR-0009 (canonical RON), ADR-0045 (Slang), ADR-0063 (determinism and presentation state), ADR-0068 (materials), ADR-0069 (signals), ADR-0072 (replay), ADR-0073 (per-role tool budgets); relates to ADR-0014 (M10's indirect draws) and ADR-0100 (M4's indirect fallback)

## Context
The engine has no particles. M1 deferred them, M2 reports the port's particle effects as unmapped and asks whether to wait for "an M4 VFX graph" (M4 plans no VFX graph kind), and the default Studio team ships its VFX artist disabled.

Unreal's Niagara is the reference for what a modern particle system does: systems of emitters, stacks of modules over typed attributes, and data interfaces into the world. Most of its cost is in tools for humans: a node-graph editor, a graph compiler with a scratch pad, a CPU/GPU bytecode VM, timeline scrubbing and a visual debugger. Concordia's users work through AI agents, which are good at writing small text files and small pure functions, and bad at none of the parts that remain.

## Decision
1. **A new milestone, M11 "Advanced particles: Sparx"** (`plans/M11-sparx-particles.md`), is added to `docs/design.md`'s milestone list.
2. **Keep Niagara's model, drop its tools.** Effects hold emitters; emitters run ordered Spawn and Update stacks of modules over typed Struct-of-Arrays attributes; a few typed data interfaces (transforms, scene depth, curves, textures, gameplay parameters) bring world state in. There is no node-graph editor, timeline scrubbing, visual debugger, scratch pad or graph compiler.
3. **Text first.** Effects are canonical RON (`.vfx.ron`), with the schema generated from `concordia-reflect`. Modules are a reflected descriptor (declared reads, writes, parameters and interfaces) plus one pure Slang function.
4. **Template code generation, not a compiler.** A string-template assembler concatenates an emitter's modules into one Spawn and one Update compute kernel, with `#line` directives so errors name the module's file and line. Kernels are cached by content in the DDC.
5. **GPU only.** Everything runs as GPU compute on persistent buffers, with alive and dead lists, atomic counters and indirect dispatches and draws; the frame path never reads back. There is no CPU particle VM. Particles that gameplay depends on are ordinary Terra entities in native code. A CPU reference simulator exists only as a test oracle.
6. **Particles are presentation.** GPU particle state is never read by the simulation and never hashed. The `Effect` component and `VfxBurst` signal are simulation data like any other; runtime playback state is presentation-only (ADR-0063). Effect durations are computed from their definitions, so gameplay never waits on the GPU. Replays reproduce particles on the same machine and build because their triggers are reproduced.
7. **An agent loop.** Hot reload keeps the last good kernels on errors; `vfx.validate` (no GPU needed) and `vfx.stats` report diagnostics with file and line, counts and GPU time. They go only into the VFX artist's scope (ADR-0073).
8. **Deferred:** multi-pass simulation stages (grids, fluids), further data interfaces, and drawing CPU-simulated entities with Sparx renderers.
9. **One indirect-draw command set** is shared with M10 (10.7) and M4 (4.3): whichever milestone lands first adds it, and the others reuse it.
10. **Clean room:** Niagara concepts come only from Epic's public documentation and talks; techniques come from open papers and permissively licensed code. No Unreal source is read.
11. **ADR block:** M11 draws ADR numbers from 0240–0259.

## Consequences
- A proposed GPU-free crate, `concordia-sparx` (format, layouts, modules, codegen, data-interface trait, CPU reference, the `vfx` engine module), and a `concordia-render::sparx` module for the GPU runtime.
- New Engine API methods `vfx.validate` and `vfx.stats` from the `vfx` engine module (`[vfx]`); projects without it keep their tool set. `render.stats` gains particle totals.
- The VFX artist role is enabled in the default team once M11 lands, with a new `concordia-author-vfx` skill.
- M2's open question on particles gets a recommended answer, "wait for M11" (M11 open question 11); `docs/design.md`'s "VFX graphs" become Sparx effect files.
- Effects that need fluid or grid simulation wait for a later stage ADR.

## Alternatives considered
- **A Niagara-style graph editor and compiler.** The most capable option, and the most costly; agents write text and Slang directly as well as they would write graphs.
- **A dual CPU/GPU bytecode VM.** It doubles every module and the test surface for a rare case that Terra entities already serve.
- **A minimal CPU emitter first (M2's open question).** Cheap, but it would be a second particle system to retire.
- **An M4 `.graph.ron` VFX kind.** It would tie particles to M4's graph compiler and its schedule, for no gain over declarative stacks.
- **JSON or YAML effect files.** The engine's text formats are canonical RON (ADR-0009), and agents already get a generated JSON Schema.
