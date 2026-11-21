# AI-First Open-Source Game Engine: Design & Plan (v0.2)

## Context
Unreal 6 is pushing engineers away. Goal: an open-source engine with Unreal-class *ambition* (results on the level of Nanite, Lumen and Jolt) that is **AI-first** and **easy to work in**. We aren't breaking new research ground. We're bringing game development to the AI world in a way that feels natural. It's built from scratch in Rust, using published techniques and permissively licensed libraries. Makers should be able to bring existing Unreal and Unity content over with little fuss, and direct a configurable **team of specialist AI agents** to build their game. The first proving ground is porting the author's 3D GDScript Godot game. Development is test-driven, long-horizon and ambitious: the author plus AI agents.

## Guiding principles
0. **Works with the agent you already have, with no API keys required (top-tier principle).** The engine never needs to call an LLM API itself. It is a *tool provider* (an MCP server, CLI, skills and agent definitions), and the user's existing agent harness does the thinking: Claude Code on a regular subscription first, other MCP-capable harnesses too. Direct API or local-model backends are optional plugins, never a requirement for building games.
1. **One API, many clients.** The editor, CLI, tests, the agent team and external agents all drive the engine through the same Engine API. No editor-only logic.
2. **Text is the source of truth.** Scenes, prefabs, materials, graphs, settings and agent-team configs are diffable text with stable IDs. Binary files exist only in the build cache.
3. **Closed loop.** Agents can run the engine headless, see results (screenshots, telemetry), act (input injection) and verify (deterministic replay, playtest assertions).
4. **Every mutation is a transaction.** A single command log provides undo/redo, review, rollback and attribution of each change to a person or agent role.
5. **Learn, don't copy.** Clean-room rule: implement from papers and talks, and never port Unreal source. Use MIT/Apache/zlib libraries where they're best in class.
6. **AI-legible codebase.** Small crates, strong types, per-crate AGENTS.md, doc tests and golden-image tests.
7. **TDD everywhere.** Tests are written first, then code to make them pass. Every milestone gate is an executable test.

## Architecture (bottom → top)
| Layer | Approach |
|---|---|
| Platform | Rust; macOS (Metal) first, then Windows/Linux. Consoles later, behind a private NDA layer. |
| Core | Job system, **Terra** (data-oriented archetype ECS growing into a Mass-class large-scale simulation framework, ADR-0010), fixed-step deterministic simulation, **reflection/type registry** (every type self-describes its schema and docs). |
| World & Commands | Command log and transactions, stable entity UUIDs, actor attribution. |
| Engine API | Typed RPC (IPC/WebSocket) plus an **MCP adapter**: query, mutate, run, capture, assert. |
| Assets | Importers → content-addressed derived-data cache; hot reload. Native formats: glTF, USD, images, audio. |
| Rendering | Thin custom RHI over Metal + Vulkan (D3D12 later), render graph, **Slang** shaders. Roadmap: clustered PBR → meshlets with GPU culling, drawn into Forward+ (meshoptimizer; ADR-0100) → a tiled shadow atlas for local lights (M10) → virtual shadow maps → dynamic GI (DDGI-style probes in M4, Photon in M9). |
| Physics / Nav / Anim / Audio | Jolt (C bindings with a safe wrapper); Recast/Detour; own animation system, with procedural rigs, IK and in-engine keyframing in M12 (motion matching later); own mixer and spatial audio (integrating the sound-catalog tool). |
| Scripting | (1) Rust native with hot reload; (2) WASM components on Wasmtime, with the API defined once in WIT so bindings exist for Rust, C/C++, Zig, AssemblyScript, Swift and Go, and batch-oriented APIs; (3) visual graphs stored as text (Blueprint-like), compiled to fast code. |
| Editor | TypeScript/React panels around a native Rust viewport; both are clients of the Engine API. |
| Plugins | **Evolve** (ADR-0011): `evolve.toml` manifests, native (trusted) and WASM (sandboxed) plugins, versioned extension points; plugin API methods become MCP tools automatically. |
| AI modules | Agent Studio (below), generated-content provider interface (pluggable local/cloud models), runtime AI (behavior trees, utility AI, optional LLM planner/dialogue). |

## Content compatibility: Unreal & Unity
- **Unity:** scenes and prefabs are YAML text when "Force Text" serialization is on, and `.meta` files hold GUIDs. Two routes: (a) a direct importer for project folders (YAML, GUID graph, FBX/textures/audio, materials mapped to our PBR); (b) an optional Unity editor package that exports to our interchange format for anything the YAML misses.
- **Unreal:** `.uasset`/`.umap` files are binary, version-specific and proprietary. Primary route: an **exporter plugin that runs inside the user's Unreal editor**, using Unreal's own USD/glTF export plus JSON for levels, materials and metadata, producing our interchange format. Secondary route (experimental): direct `.uasset` reading, learning from MIT-licensed community readers.
- **Logic can't be auto-converted.** Blueprints, Unity C# and GDScript are translated by agents into Rust, WASM or our graphs, then verified with tests and replay.
- **Licensing guardrail:** the importer moves *the maker's own or licensed* content. Whether marketplace content (Fab, Unity Asset Store) may be used in another engine depends on each license; the importer shows this to the user.
- **Interchange format:** USD and glTF for geometry and materials, plus our text scene format. All three importers (Godot, Unity, Unreal) target it.

## Agent Studio: configurable agentic game-dev teams
- **Roles as config files:** a text file per role (e.g. `studio/agents/shader-dev.md`, with frontmatter and a prompt), holding the role, instructions, custom prompts, model/provider, tool and API permission scopes, asset folders it may touch, and review requirements.
- **Default roster** (all editable, removable or duplicated): Director/Producer (orchestrates), Gameplay Programmer, Shader/Tech Artist, 3D Modeler, Animator, VFX Artist, Sound Designer, Composer, Writer/Narrative, Level Designer, UI/UX, QA/Playtester.
- **Team config** (`studio/team.toml`): how many instances of each role, concurrency limits, budgets (tokens/cost), approval gates (which changes need a human or reviewer agent), and escalation rules.
- **Runs inside Claude Code (subscription, no API key):**
  - The engine ships an **MCP server** (`concordia-mcp`) that exposes the Engine API: scene query/mutate, run headless, screenshot, replay, assert, import, and task-board operations.
  - `studio sync` **compiles** the role configs into Claude Code native files: `.claude/agents/<role>.md` (subagents with prompts, tool allowlists and model hints), `.claude/skills/` (engine workflows such as "add a material" or "write a playtest"), `CLAUDE.md` project rules, `.mcp.json` registration, and hooks (e.g. block writes outside a role's folders, run tests after edits).
  - The **main Claude Code session acts as the Director**, sending work to role subagents with Claude Code's own Agent tool and workflows. Team size, roles and prompts all come from the user's config.
  - The editor's Studio panel *visualizes* the task board, command log and agent attribution, and stays in sync over the Engine API. Optionally it launches the user's locally installed, logged-in Claude Code CLI; it never holds credentials.
- **Mechanics:** a task board in text form, so it can be diffed; agents act only through the Engine API/MCP; every change goes through the command log with role attribution; QA agents verify with replay and playtest assertions; the human can approve, reject or redirect from the editor's "Studio" panel or the CLI.
- **Harness-portable:** the role configs are the source of truth. Claude Code is the first compile target; adapters for other MCP-capable harnesses, or an optional direct-API runner, can be added later without changing the configs.
- **Generated content without APIs:** by default agents create content through code: procedural meshes, Slang shaders, Sparx particle effects (text effect files and Slang modules; ADR-0015), synthesized audio, text and dialogue, and driving tools like Blender through their scripting or MCP. Image, 3D and audio generation models are opt-in provider plugins.
- **Runtime AI in shipped games** is separate: players won't have the developer's subscription. Runtime AI therefore uses behavior trees and utility AI by default, with local models or a provider the game developer chooses as opt-in.

## Licensing & governance
Dual MIT/Apache-2.0. Written clean-room policy and DCO sign-off from day one. Foundation governance once there are outside contributors.

## Milestones (each gate is an executable test)
- **M-1: Toolchain & scaffold.** Install rustup and stable Rust, plus clippy, rustfmt, cargo-nextest, the `wasm32-wasip2` target, wasmtime-cli, wasm-tools and cargo-component. Also Node (for the editor UI), Slang (later), and the Xcode command-line tools (Metal). Create the project directory with: `docs/` (design doc, ADRs, clean-room policy), `CLAUDE.md`, `AGENTS.md`, `crates/` (engine), `tests/` (integration/golden), `tools/`, `editor/`, `studio/`, plus CI config and a `justfile`. *Gate:* `cargo nextest run` passes on a seed test, and a WASM component builds and runs under wasmtime from a test.
- **M0: Headless core.** ECS, reflection, command log, text scene format, Engine API and MCP. *Gate:* **this Claude Code session**, with `concordia-mcp` registered in `.mcp.json` and no API key, creates, edits, saves and queries a scene over MCP, with no renderer.
- **M1: See it.** RHI (Metal first), render graph, PBR, glTF import, Jolt, input, and an agent-first editor v1 (files, full MCP coverage, scene tree, component discovery, an agent activity view, renderer debug views; ADR-0048, ADR-0049). *Gate:* golden-image test of a physics scene; an agent captures a screenshot; a live agent builds and runs a scene from nothing through MCP only.
- **Agent Studio first** (ADR-0049). Right after M1's gate, M3's Studio phases (team config, task board, `studio sync`, attribution and enforcement, roster, the Studio panel) run, with a Studio checkpoint gate; the rest of M3 follows M2.
- **M2: Engine breadth** (formerly 'Port begins'; ADR-0048, restructured by ADR-0067). Terra's Mass-style core (processors and phases, command buffers, signals and observers, templates and traits, LOD, spawners), scripting tiers (native and WASM; no ability system), minimal replay, Substrate-style materials and FBX import, animation, navigation with crowds and traffic, audio and in-game UI. The Godot importer, GDScript translation, Godot mappings and the first playable level move to M5. *Gate:* an original sample game in `examples/` uses every M2 subsystem; a headless scripted playthrough, a replay across thread counts and golden screenshots pass in public CI.
- **M3: Closed loop & Studio v1.** Record/replay, playtest assertions, Agent Studio with configurable roles, generated-content plugin. *Gate:* `studio sync` generates `.claude/agents/*` from the team config; in Claude Code (subscription only), the Director session assigns a bug to the Gameplay subagent, which fixes it; the QA subagent proves the fix via replay.
- **M4: Unreal-class visuals.** Meshlet geometry with GPU culling (ADR-0100), virtual shadow maps, dynamic GI; visual graphs; runtime AI.
- **M5: Bring your game.** Unity importer, Unreal exporter plugin and interchange format, and the Godot port (importer, agent GDScript translation, the first level of the reference game; moved from M2 by ADR-0048). *Gate:* a sample Unity project and a sample Unreal project round-trip into playable scenes, and the first level of the Godot game is playable with its replay test passing.
- **M6: Evolve (plugins).** After M2, in parallel with M3. Manifest, resolution and lockfile; native and WASM plugin hosts; extension points; unknown-component preservation. *Gate:* sample native and WASM plugins add components, systems and MCP tools; an agent uses them; disabling them keeps scenes intact.
- **M7: Terra (large-scale simulation).** The M0 ECS was renamed `concordia-terra` right after M0 (Phase 7.0); its Mass-style core moved to M2 (ADR-0067), and M7 keeps the large scale and AI-driven crowds after M4. *Gate:* 100,000 crowd agents from text templates at 60 Hz on the reference machine, deterministic across thread counts.
- **M8: Advanced physics** (ADR-0012). Chaos-class physics on top of Jolt: fracture authoring, clusters and connection graphs, strain, fields, debris and caches; a GPU garment solver (XPBD on `concordia-rhi`) with painted constraints, tethers, body and self collision. Destruction may start after M1; garments need M2 skinning. *Gate:* a fractured wall breaks deterministically at any thread count, and a walking character's cape and skirt simulate on the GPU at ≥ 60 fps matching the CPU reference.
- **M9: Advanced lighting** (ADR-0013). **Photon**, fully dynamic global illumination and reflections in the class of Lumen, rebuilt from Epic's public talks (clean room): mesh and global distance fields, a surface cache of material cards with radiosity, screen-space then distance-field or hardware ray tracing, screen probes with a world-space radiance cache, denoised reflections, and a translucency and fog volume. It is M4's GI stage B; DDGI stays the low tier. *Gate:* indirect light within mean FLIP 0.08 of the path tracer, convergence within 8 frames after a door opens, no leaks through 20 cm walls, and the high tier within 4 ms on the reference machine.
- **M10: Advanced shadows** (ADR-0014). A tiled shadow atlas for point and spot lights: one whole tile per light (six for a point light's cube), sized by importance; per-tile CPU frustum and receiver-aware culling over a BVH; static shadow caching, so a light that hasn't moved never resubmits its static casters. No meshlet pipeline is required; an optional MultiDrawIndirect path serves GPU-driven renderers. Its relation to M4's VSM is an open question in the plan. *Gate:* cached and uncached frames byte-identical, zero static draws while no light moves, and 48 shadowed lights within budget on the reference machine.
- **M11: Advanced particles** (ADR-0015). **Sparx**, a Niagara-style GPU particle runtime without a node-graph editor, timeline or visual debugger: effects of emitters with Spawn and Update stacks of small pure Slang modules over typed Struct-of-Arrays attributes, declared in canonical RON; template codegen into one compute kernel per stage; alive and dead lists with indirect dispatches and draws for sprites, ribbons and meshes; five typed data interfaces; and a hot-reload loop with diagnostics for agents. Particles are presentation, never simulation state. *Gate:* GPU results match a CPU reference, codegen is byte-identical, world hashes ignore particles, a broken module is reported by file and line while the last good kernels keep running, and 1M particles update within budget on the reference machine.
- **M12: Rigging v1.0** (ADR-0016). Procedural rigs and in-engine animation in the spirit of Control Rig, as text: a `.rig.ron` rig is a flat map of named units (controls, get and set transform, two-bone, FABRIK and CCD IK, aim, look-at, constraints, math) with setup, forward and backward events, compiled to an allocation-free CPU program. Simulation rigs are hashed and replayed; Presentation rigs are excluded from the world hash. Agents key control curves into `.clip.ron` clips, preview, bake (at import, to bones or back to controls) and use a pose library, through `rig.*` and `anim.key_*` methods whose edits are undoable transactions; the viewport draws and drags controls. No visual rig editor or timeline panel. *Gate:* analytic and property-tested IK, forward and backward round trips, bit-identical poses across thread counts and replays, a baked clip equal to live evaluation, an MCP-authored rig and wave fully undoable, and 2,000 biped rigs per ms on the reference machine.
- **Later:** world streaming, multiplayer/replication, consoles, AA team workflows.

## Plan hierarchy
This document is the **top-level design**. Each level of planning produces the next:
1. **This plan, executed** → a detailed implementation plan per milestone in `<project root>/plans/`. No engine code yet.
2. **Each milestone plan, executed** → phases of code, built test-first.


## Execution of this plan: produce the milestone plans
1. **Seed the project root** with planning artifacts only:
   - `docs/design.md`: this high-level design.
   - `docs/clean-room.md`: the clean-room policy.
   - `docs/adr/`: first ADRs for the decisions already made (Rust, subscription-first/MCP, scripting tiers, web editor plus native viewport, interchange formats, licensing).
   - `CLAUDE.md`: project rules (TDD, the plan hierarchy, crate conventions, the clean-room rule, principle #0).
   - `plans/README.md`: an index of milestones with their status, dependencies, and the plan template below.
2. **Write one plan per milestone** in `plans/`:
   - `M-1-toolchain-and-scaffold.md`
   - `M0-headless-core.md`
   - `M1-see-it.md`
   - `M2-port-begins.md`
   - `M3-closed-loop-and-studio.md`
   - `M4-unreal-class-visuals.md`
   - `M5-bring-your-game.md`
   - `later-backlog.md`
3. **Milestone plan template.** Each plan contains:
   - Goal and context, scope and non-goals, dependencies on earlier milestones.
   - Crates, modules and directories created or touched.
   - **Phases** (typically 3–8). Each phase has:
     - an objective;
     - deliverables;
     - **tests written first**: specific unit, integration, golden-image and replay tests, with names and the behavior each asserts;
     - implementation notes: key types, APIs, algorithms, and libraries or references (papers/talks);
     - done criteria.
   - **Milestone gate:** the executable acceptance test.
   - Risks and mitigations, open questions, ADRs needed, and a rough effort estimate.
4. **Level of detail:** M-1, M0 and M1 are written at full, ready-to-execute detail. M2–M5 get full phase breakdowns and test lists, flagged for a refinement pass when their predecessor milestone completes, because what we learn earlier will change them.
5. **Consistency pass:** check that crate names, the Engine API and MCP surface, the scene format and test conventions agree across all milestone plans, and that each gate is reachable from its phases.
6. Save project and preference memories: TDD, the plan hierarchy, subscription-first, ambitious long-horizon scope, and the Godot port target.

Writing the plans may use parallel subagents (one per milestone, sharing this design and the template), followed by my own consistency pass. No toolchain installs or engine code happen in this step; those begin when `M-1-toolchain-and-scaffold.md` is executed.

## Verification
**For this plan's execution:**
- `plans/` contains all eight files, and each follows the template.
- Every phase lists its tests before its implementation notes.
- Every milestone gate is an executable test.
- Crate and API names are consistent across plans.
- `CLAUDE.md` and `docs/` exist.

**For later milestones:** the milestone gates above. CI runs: fmt, clippy, nextest, doc tests, WASM component tests; golden-image tests from M1; deterministic replay tests from M3.

## Reference port
The first proving ground is the author's Godot 4.6 isometric ARPG: hundreds of GDScript files, hundreds of scenes, custom shaders, glTF/FBX models, procedural zones, a C++ GDExtension and a headless test suite. The test suite is the oracle for the port. Specifics are private (`private/port-target.md`, git-ignored).

## Dev machine facts (2026-11-17)
macOS 26.2 on Apple Silicon (arm64). Installed: rustup 1.27 with **rustc 1.77.2 (outdated; must update)**, Node 24.12, npm 11, git 2.45, Homebrew, Xcode with the Metal toolchain, Python 3.12, Blender 5.2.1 LTS. Missing: pnpm, just, wasmtime, wasm-tools, cargo-component, cargo-nextest, Slang.
