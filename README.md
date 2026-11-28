<div align="center">

# ✦ Concordia

### The game engine your AI team already knows how to use.

**Open source · Written from scratch in Rust · AI-first · Aiming for Unreal-class results**

[![CI](https://github.com/dan-strohschein/concordia/actions/workflows/ci.yml/badge.svg)](https://github.com/dan-strohschein/concordia/actions/workflows/ci.yml)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)
![Rust](https://img.shields.io/badge/rust-1.100-orange)
![macOS](https://img.shields.io/badge/macOS-available-success)
![Linux](https://img.shields.io/badge/Linux-Vulkan%20preview-yellow)
![Windows](https://img.shields.io/badge/Windows-mid--2027-lightgrey)
![No API key](https://img.shields.io/badge/LLM%20API%20key-not%20required-brightgreen)

</div>

> [!NOTE]
> **"Concordia" is a code name.** The engine gets its real name at launch. Commands, crates and
> paths that say `concordia` will be renamed then, and this README will tell you how to migrate.

> [!WARNING]
> **Pre-alpha.** Concordia is under heavy, test-driven development. The headless core, the renderer
> (Metal and Vulkan, with meshlets, virtual shadow maps, temporal upscaling and dynamic GI), the
> editor, Agent Studio, visual gameplay graphs, runtime AI and the gameplay systems of M2 work today
> (see [Capabilities](#-capabilities-available-today) and [Roadmap](#-roadmap)). It is not ready to
> ship a game yet. It is ready to explore, break and help build.

---

## What is it?

Most engines were designed for a person clicking around an editor. AI agents were added later, if at
all, and they have to squint at screenshots and guess at menus.

Concordia starts from the other end. **Every part of the engine can be driven, seen and checked by an
AI agent**, through the same typed API the editor uses. Scenes are text you can diff. Every change is
an undoable transaction that records *who* made it, human or agent. Agents can run the game headless,
take screenshots, press buttons, and prove that a bug is fixed with a deterministic replay.

Then it goes one step further: you can direct a whole **studio of specialist AI agents** (a Director,
gameplay programmer, tech artist, animator, sound designer, QA and more) that build your game
together, each one limited to its own tools and folders.

And it runs on the AI subscription you already have.

<table>
<tr>
<td width="33%" valign="top">

### 🧠 AI-first
An MCP server, a CLI and generated Claude Code agents, skills and hooks. Your agent gets the whole
engine as tools.

</td>
<td width="33%" valign="top">

### 🔑 No API keys
The engine never calls an LLM. Your own agent harness does the thinking. Claude Code on a regular
subscription is the first target.

</td>
<td width="33%" valign="top">

### 🚀 Unreal-class ambition
Meshlet geometry, virtual shadow maps, fully dynamic GI, Mass-style crowds, GPU particles, procedural
rigs: rebuilt clean-room from public papers and talks.

</td>
</tr>
</table>

---

## ✨ Features

### Built for agents (and the humans who direct them)

- **One API, many clients.** The editor, the CLI, tests and agents all use the same Engine API. There
  is no editor-only logic, so anything you can do with a mouse, an agent can do too.
- **MCP server built in.** Register it in `.mcp.json` and your agent can create scenes, spawn entities,
  import models, step the simulation, take screenshots and read render stats.
- **Self-describing.** Every component and method publishes a JSON Schema with documentation, so an
  agent can find out what exists without reading the source.
- **Closed loop.** Run headless, see the result (screenshots, debug views, telemetry), act (input
  injection, pointer rays) and verify (deterministic replays, playtest assertions).
- **Every change is a transaction.** A single command log gives you undo/redo across restarts, a full
  history, and attribution of every edit to a person or an agent role.

### 🎬 Agent Studio: your configurable game-dev team

- **Roles are text files.** Each role has its own prompt, tool allowlist, model hint, the folders it
  may write to, and its review rules. Add, remove, duplicate or rewrite any of them.
- **A default roster** of Director/Producer, Gameplay Programmer, Shader/Tech Artist, 3D Modeler,
  Animator, VFX Artist, Sound Designer, Composer, Writer, Level Designer, UI/UX and QA/Playtester.
- **`concordia studio sync`** compiles your team into native Claude Code files: subagents, skills,
  project rules, MCP registration and enforcement hooks.
- **A task board in plain text**, with claims, comments, approvals and rejections, plus questions
  agents can ask the team (and you).
- **Guardrails that hold.** Hooks block writes outside a role's folders, and the engine checks every
  mutation against the role that made it.

### 🧱 Engine core

- **Terra**, a data-oriented archetype ECS with a Mass-style framework on top: processors with access
  declarations, deterministic parallel scheduling, shared and chunk components, tags, command buffers,
  signals and observers, templates and traits, spawners, and simulation plus representation LOD.
- **Deterministic by design.** A fixed-step simulation whose world hash is identical across thread
  counts and replays.
- **Text is the source of truth.** Scenes, prefabs, materials, animation graphs and team configs are
  canonical RON with stable IDs. Binary data lives only in a content-addressed build cache.
- **Gameplay in three tiers:** native Rust with hot reload; sandboxed WASM components (AOT-compiled
  on Wasmtime, API defined once in WIT, so you can write gameplay in Rust, C/C++, Zig, Go, Swift or
  AssemblyScript); and **visual graphs stored as text**, compiled to sandboxed WebAssembly and edited
  by agents and humans side by side.

### 🎨 Rendering, physics and the rest

- **A custom RHI and render graph** on **Metal and Vulkan** (one backend-neutral renderer; the Vulkan
  backend runs the whole render suite on Mesa's software Vulkan in CI), **Slang** shaders, clustered
  forward PBR and image-based lighting.
- **Meshlet geometry with GPU culling:** meshes split into meshlets offline, drawn with mesh shaders
  where the GPU has them (indirect compute elsewhere), culled on the GPU by frustum, normal cone and
  two-pass hierarchical-Z occlusion.
- **Virtual shadow maps** for the sun, spot and point lights, cached page by page, with soft
  contact-hardening shadows; cascaded shadow maps remain as a fallback.
- **Temporal anti-aliasing and upscaling:** our own TAA/TAAU on every backend, and MetalFX on Apple
  GPUs, from motion vectors for rigid and skinned meshes.
- **Dynamic global illumination** from probes that follow the camera, traced with hardware ray queries
  where available (a software BVH elsewhere), measured against path-traced references.
- **A reference path tracer** (GPU, with a CPU fallback that cross-checks it) that renders the ground
  truth the real-time renderer is measured against, plus per-pass GPU timing and perf budgets.
- **Substrate-style layered materials:** slabs with haze, fuzz, subsurface and thin film, combined by
  layering, mixing and coverage under a per-pixel closure budget.
- **Jolt physics** with character controllers, sensors, contact events and raycasts.
- **Animation:** GPU skinning, clips, blend trees and state machines with Godot `AnimationTree`
  semantics, and retargeting by bone name.
- **Navigation:** Recast/Detour navmeshes, click-to-move, crowds with avoidance and LOD, and lane traffic.
- **Import:** glTF and FBX today, with Unity, Unreal and Godot importers on the roadmap.
- **Runtime AI without an LLM:** behavior trees, utility scoring and perception (sight, hearing,
  damage, memory), deterministic and replayable, with an optional, sandboxed LLM planning hook.
- **Audio:** our own mixer and spatial audio.
- **Editor:** React panels around a native viewport in a Tauri shell, laid out like Godot (Metal on
  macOS; Vulkan in an X11 window on Linux). It edits the same live scene your agents do.

---

## ✅ Capabilities available today

What is built, tested and usable on `main` (macOS on Metal; Linux on Vulkan as a preview). Each
item is covered by tests in its crate, and each milestone by an executable gate.

<details open>
<summary><b>🤖 Agents and tools</b></summary>

- **Engine API** with a JSON Schema for every method and component, served to agents as an **MCP
  server** (`concordia-mcp`), the `concordia` CLI and the editor alike.
- **Headless runs** with screenshots, render stats, debug views, log tailing and input injection.
- **Command log:** every change is an undoable transaction with its actor (human or agent role),
  persisted across restarts; text files (graphs) are undoable byte for byte too.
- **Deterministic replays** (format 2) that verify tick by tick at any thread count, with
  checkpoints, field-level diffs, re-blessing and **playtest assertions** (`within 300 ticks: …`).
- **Agent Studio:** a configurable team of role agents compiled to Claude Code subagents, skills,
  rules and enforcement hooks; a text task board, questions, per-role tool and path scopes and
  per-role tool budgets.

</details>

<details>
<summary><b>🧱 Core and simulation</b></summary>

- **Terra ECS** (archetypes, Mass-style processors with declared access, deterministic parallel
  scheduling, shared and chunk components, tags, command buffers, signals and observers, templates
  and traits, spawners, simulation and representation LOD).
- **Reflection** for every type (schemas, defaults, ranges, canonical hashing, a compact wire format).
- **Canonical text formats** (RON) for scenes, prefabs, materials, animation graphs, gameplay graphs,
  behavior trees and team configs; binary data only in a content-addressed cache.
- **Scripting:** native Rust modules and sandboxed WASM components (fuel limits, NaN
  canonicalization, AOT cache), hot reload with migration of changed components.
- **Physics:** Jolt (rigid bodies, characters, sensors, contacts, raycasts), deterministic.
- **Animation:** GPU skinning, clips, blend trees and state machines, retargeting, animation events.
- **Navigation and crowds:** Recast/Detour navmeshes, path following, click-to-move, crowds with
  avoidance, lane traffic.
- **Audio:** an own mixer, buses, spatial emitters and cues; **UI:** in-game controls and text.
- **Import:** glTF and FBX models and textures, procedural content generators.

</details>

<details>
<summary><b>🎨 Rendering</b></summary>

- **Backends:** Metal (macOS) and Vulkan (Linux preview; the full render suite runs under Mesa
  lavapipe with validation in CI), behind one RHI with bindless resources, a render graph and per-pass
  GPU timing.
- **Materials:** clustered forward PBR, image-based lighting, Substrate-style layered materials
  (haze, fuzz, subsurface, thin film).
- **Geometry:** offline meshlets, GPU scene buffers, mesh-shader and compute/indirect meshlet paths,
  GPU frustum, normal-cone and two-pass HZB occlusion culling; a 22-million-triangle canyon test scene.
- **Shadows:** virtual shadow maps for directional, spot and point lights with page caching and a
  static/dynamic split, soft shadows from sized lights (PCSS); cascaded shadow maps as fallback.
- **Anti-aliasing and upscaling:** own TAA and TAAU (jitter, history, exposure, motion vectors for
  rigid and skinned meshes), MetalFX on Apple GPUs.
- **Global illumination:** dynamic DDGI-style probes with irradiance and visibility, a scrolling
  camera-following lattice or fixed volumes, traced through a software BVH or hardware ray queries
  (acceleration structures on Metal). Matches 8-bounce path-traced references closely; it is still
  over its frame budget, so it is on by default only in the lighting example.
- **Reference path tracer** (GPU, with a CPU cross-check) and FLIP-based image tests against it.

</details>

<details>
<summary><b>🕸️ Visual gameplay graphs</b></summary>

- **`.graph.ron`** graphs (Event, Function, Behavior, Utility kinds) as diffable text, with UUIDv7
  node ids and a semantic hash that ignores layout.
- **A node library generated from the engine:** get/set for every component and field, raise/receive
  for every signal, math, logic, vectors, flow control and `#[graph_node]` Rust functions.
- **A type checker** whose diagnostics name the node and pin to fix, with suggestions.
- **Compiled to sandboxed WebAssembly** that reads and writes component columns in place, with an
  interpreter that matches it bit for bit; graphs run as script modules with fuel limits and hot
  reload that keeps their variables.
- **Authoring over MCP** (`graph_list_nodes`, `graph_apply`, `graph_check`, `graph_compile`,
  `graph_read`, `graph_trace`) and a **Graph panel** in the editor (React Flow), live as agents edit,
  with every edit attributed and undoable. Studio skills teach agent roles to author graphs.
- Today a graph works on its own entity; reading other entities, queries and loops come later.

</details>

<details>
<summary><b>🧠 Runtime AI</b></summary>

- **Brains:** behavior trees from `.bt.ron` files over a typed blackboard: sequences, selectors,
  parallels, utility and random selectors, conditions that interrupt (abort modes), cooldowns, loops,
  time limits; tasks for movement (navigation), animation, facing, casting and running graphs.
- **Utility scoring:** response curves (linear, polynomial, logistic, logit, piecewise), IAUS
  compensation, momentum, seeded top-N selection.
- **Perception:** sight cones with line of sight through physics, hearing with falloff, damage,
  factions and a decaying memory that feeds the blackboard.
- **Deterministic:** every choice is an `ai.Decision` signal (assertable in playtests); 500 agents give
  the same world hash at any thread count; time slicing keeps the per-tick cost bounded
  (about 0.5 ms for 500 agents in a debug build). `ai.inspect` and `ai.trace` show what a brain is doing.
- **Optional LLM planning:** a `Plan` task can ask a model *the game ships with* to choose among
  actions, validated against a schema and falling back to the tree; answers are recorded so replays
  need no model. The default build contains no model runtime or network client, and a developer's
  Claude subscription is never used at runtime.

</details>

<details>
<summary><b>🖥️ Editor</b></summary>

- A Tauri shell with React panels around a native viewport (Metal on macOS; Vulkan in an X11 window on
  Linux): scene tree, inspector with generated widgets, file system, material inspector, gizmos,
  output, agent activity, team questions, the Studio board and the Graph panel. It edits the same live
  scene as your agents, through the same API.

</details>

---

## ⚖️ How it compares

Unreal, Unity and Godot are excellent, mature engines with huge communities, and today they can do far
more than Concordia can. Here is where we are going differently.

| | **Concordia** | **Unreal** | **Unity** | **Godot** |
|---|---|---|---|---|
| **License** | MIT or Apache-2.0 | Source-available, proprietary EULA | Proprietary | MIT |
| **Royalties / fees** | None, ever | Royalty above a revenue threshold | Paid tiers above a revenue threshold | None |
| **Core language** | Rust | C++ | C++ (closed) | C++ |
| **Gameplay code** | Rust, WASM (many languages), visual graphs | C++, Blueprints | C# | GDScript, C#, GDExtension |
| **Designed for AI agents** | ✅ From day one: MCP, schemas, headless, replay, attribution | ➖ Via plugins | ➖ Via plugins | ➖ Via plugins |
| **AI dev team built in** | ✅ Agent Studio | ❌ | ❌ | ❌ |
| **Needs an LLM API key** | ❌ Never | n/a | n/a | n/a |
| **Scenes as diffable text** | ✅ Always (canonical RON) | ❌ Binary `.umap` | ➖ YAML with "Force Text" | ✅ `.tscn` |
| **Every edit attributed and undoable** | ✅ Command log | ➖ Editor undo | ➖ Editor undo | ➖ Editor undo |
| **Deterministic replay** | ✅ Across thread counts | ➖ Replay system, not deterministic | ❌ | ❌ |
| **High-end rendering** | 🛠️ Largely in (meshlets with GPU culling, virtual shadow maps, TAAU, probe GI); LODs and GI performance next | ✅ Nanite, Lumen | ✅ HDRP | ➖ Improving |
| **Game AI** | ✅ Behavior trees, utility, perception; deterministic and replayable | ✅ Behavior trees, EQS | ➖ Packages | ➖ Add-ons |
| **Large-scale simulation** | 🛠️ Terra (Mass-style), in progress | ✅ Mass | ✅ DOTS/ECS | ➖ |
| **Platforms** | macOS now; Linux preview (Vulkan); Windows mid-2027 | Everything | Everything | Everything |
| **Maturity** | Pre-alpha | Decades | Decades | 10+ years |

✅ yes · ➖ partly · ❌ no · 🛠️ being built

---

## 💡 Why choose Concordia?

- **You build games with AI agents, or want to.** Concordia is the only engine where agents are
  first-class users rather than an afterthought. They don't guess; they query schemas, take
  screenshots, run replays and prove their work.
- **You want to own your engine.** MIT or Apache-2.0, no royalties, no runtime fees, no license that
  can change under you. Fork it, ship it, sell it.
- **You want a codebase you (and your agents) can actually read.** Small crates, strong types, an
  `AGENTS.md` in every crate, tests for everything. Changing the engine is as approachable as making
  the game.
- **You care about reviewable work.** Text scenes, a full command log and per-role attribution mean
  every change made by every agent can be diffed, reviewed and rolled back.
- **You want high-end results without high-end lock-in.** The roadmap aims at Unreal-class geometry,
  lighting, shadows, particles and destruction, all in the open.
- **You're bringing a game with you.** Importers for Unity, Unreal and Godot content are planned, and
  agents translate the logic, verified with tests and replays.

---

## 📦 Install

> [!TIP]
> **Our goal: one command to install, one command to build, one command to test.** The installers are
> still being written. Until they land, the "build from source" steps below are what works today, and
> `just doctor` tells you exactly what is missing.

### 🍎 macOS (available now)

**Requirements:** a Mac with Apple Silicon running macOS 26 or later, and Xcode with the Metal toolchain.

<details open>
<summary><b>Coming soon: the one-line install</b></summary>

```sh
# Planned, not yet available
curl -fsSL https://<launch-domain>/install.sh | sh     # or: brew install concordia
concordia new my-game && cd my-game && concordia run
```

</details>

<details open>
<summary><b>Build from source (works today)</b></summary>

**1. Install the prerequisites** (one time):

```sh
xcode-select --install                     # command-line tools (install full Xcode for Metal)
brew install rustup just node git
rustup-init -y                             # the repo pins its exact Rust version for you
corepack enable pnpm                       # pnpm for the editor UI
cargo install --locked cargo-nextest cargo-deny cargo-insta wasm-tools wasmtime-cli cargo-component
```

**2. Get the code and check your setup:**

```sh
git clone https://github.com/dan-strohschein/concordia.git
cd concordia
just doctor                                # lists anything missing and how to fix it
```

**3. Build:**

```sh
cargo build --workspace                    # the engine, the CLI and the MCP server
just slang-fetch                           # the pinned Slang shader compiler
```

**4. Test:**

```sh
just test                                  # every test and doc test
just lint                                  # rustfmt + clippy, warnings are errors
just ci                                    # everything CI runs
just gate M1                               # a full milestone gate
```

**5. Install the `concordia` CLI** on your `PATH`:

```sh
cargo install --path crates/concordia-cli
```

**6. Make something:**

```sh
concordia new my-game                      # project, scene, .mcp.json and Agent Studio
cd my-game
concordia run                              # open the window and play
concordia studio sync                      # generate your Claude Code agent team
claude                                     # start Claude Code: it sees the engine as tools
```

Or open the editor on a sample project, from the repo:

```sh
just editor examples/m1-physics-stack
```

</details>

### 🐧 Linux (preview: Vulkan, build from source)

The engine, its Vulkan renderer, `concordia run` windows (X11 and Wayland) and the editor's native
viewport (X11) build from source on Linux, and CI tests them on Mesa's software Vulkan (lavapipe)
under the Khronos validation layers. They have not yet been re-tested on real Linux GPUs, and
packages and an installer arrive in mid-2027.

**Requirements:** a Vulkan 1.3 driver (Mesa, NVIDIA or AMD), Rust (pinned by the repo), Node and pnpm
for the editor. On Ubuntu 24.04:

```sh
sudo apt-get install libvulkan1 mesa-vulkan-drivers vulkan-validationlayers libasound2-dev
# the editor (Tauri on WebKitGTK):
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev librsvg2-dev libxdo-dev libssl-dev
```

**Build, test and run:**

```sh
git clone https://github.com/dan-strohschein/concordia.git && cd concordia
just doctor                                # checks Rust, Node and the rest
just slang-fetch                           # the pinned Slang shader compiler
cargo build --workspace
just test
cargo install --path crates/concordia-cli
concordia gpu-probe --backend vulkan       # what your GPU offers
concordia run --project examples/m1-physics-stack
just editor examples/m1-physics-stack      # under Wayland: GDK_BACKEND=x11 for the native viewport
```

`CONCORDIA_BACKEND=vulkan` (or `[render] backend = "vulkan"` in a project) picks Vulkan explicitly;
`auto`, the default, is Metal on macOS and Vulkan elsewhere. `CONCORDIA_VULKAN_VALIDATION=strict`
turns on the validation layers and fails on any error they report.

<details>
<summary><b>Coming in mid-2027: packages</b></summary>

```sh
# Planned, not yet available
curl -fsSL https://<launch-domain>/install.sh | sh     # plus .deb, .rpm and Flatpak packages
```

</details>

### 🪟 Windows (coming mid-2027)

Windows support (Vulkan first, Direct3D 12 later), the editor and an installer arrive in mid-2027. The plan:

```powershell
# Planned, not yet available
winget install Concordia                             # or the .msi installer

# Build from source
git clone https://github.com/dan-strohschein/concordia.git; cd concordia
just doctor
cargo build --workspace
just test
cargo install --path crates/concordia-cli
```

---

## 🤖 Use it with Claude Code

No API key, no extra billing: Concordia is a tool provider, and your subscription does the thinking.

1. `concordia new my-game` writes a `.mcp.json` that registers the engine's MCP server.
2. `concordia studio sync` turns `studio/` into `.claude/agents/`, skills, rules and hooks.
3. Start `claude` in the project. Your session is the **Director**; it hands work to the role
   subagents, and they act only through the engine's API.

Then ask for things like *"Build an arena with a physics stack of crates, take a screenshot, and save
the scene"*, and watch it happen live in the editor.

> [!IMPORTANT]
> If `ANTHROPIC_API_KEY` is set in your shell, Claude Code bills the API instead of your subscription.
> `just doctor` warns you about it.

The agent guide to the Engine API is at [`docs/guides/agents-engine-api.md`](docs/guides/agents-engine-api.md).

---

## 🗺️ Roadmap

Every milestone ends in an executable gate test.

| Milestone | What it brings | Status |
|---|---|---|
| **M-1** Toolchain & scaffold | Pinned toolchain, CI, WASM components | ✅ Gate passed |
| **M0** Headless core | ECS, reflection, command log, text scenes, Engine API, MCP | ✅ Gate passed |
| **M1** See it | Metal RHI, render graph, PBR, glTF, Jolt, input, editor v1 | ✅ Gate passed |
| **M3s** Agent Studio | Team config, task board, `studio sync`, enforcement, Studio panel | ✅ Checkpoint passed |
| **M2** Engine breadth | Terra Mass core, scripting, replay, materials, FBX, animation, navigation, crowds, traffic, audio, UI | ✅ Gate passed |
| **M3** Closed loop & Studio v1 | Replays with playtest assertions, generated-content plugins, Studio v1 | ✅ Gates passed (closing) |
| **M4** Unreal-class visuals | Path-traced references, **Vulkan**, meshlets with GPU culling, virtual shadow maps, dynamic GI, visual graphs, runtime AI | 🛠️ Closing: everything below is in except meshlet LODs (a follow-up) and the gate (the Lamplight enemy built from graphs and AI) |
| **M5** Bring your game | Unity importer, Unreal exporter, Godot port | Planned |
| **M6** Evolve | Native and WASM plugins whose APIs become MCP tools | Planned |
| **M7** Terra at scale | 100,000 crowd agents at 60 Hz, deterministic | Planned |
| **M8** Advanced physics | Destruction and GPU garments | Planned |
| **M9** Photon | Fully dynamic GI and reflections | Planned |
| **M10** Advanced shadows | Tiled shadow atlas with static caching | Planned |
| **M11** Sparx | GPU particles written in Slang | Planned |
| **M12** Rigging | Procedural rigs and in-engine keyframing | Planned |
| **M13** Advanced Godot features | The AnimationTree features M2 left out (one-shot fades, transition options, advance expressions, curves) | Planned |
| **M14** CPU-only games | Games that render on the CPU, from M4's path-tracing core | Planned |
| **Mid-2027** | **Linux packages and Windows** | Planned |

The full design is in [`docs/design.md`](docs/design.md), the decisions behind it in
[`docs/adr/`](docs/adr/), and each milestone's plan in [`plans/`](plans/).

---

## 🤝 Contributing

Humans and AI-assisted workflows are equally welcome, under the same rules:

- **Test-first, always.** Write the tests, watch them fail, then make them pass.
- **Clean room.** Never contribute code copied, ported or translated from Unreal or any other
  non-open-source engine. Learn from papers, talks and permissively licensed code. See
  [`docs/clean-room.md`](docs/clean-room.md).
- **Subscription-first.** No change may require an LLM API key.
- **Sign off your commits** (`git commit -s`) under the Developer Certificate of Origin.

Start with [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md).

## 📜 License

Dual-licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

---

## 🏛️ Standing on the shoulders of giants

Concordia is fully inspired by **Unreal**, **Unity** and **Godot**. Their teams showed the world what a
game engine can be, and shared an enormous amount of what they learned in public talks, papers and
documentation. We're grateful to every one of them.

**And it's built in a clean room.** We learn from what they've published, never from their code. Not
one line of Concordia is copied, ported or translated from another engine's source. Every idea is
rebuilt from scratch, in the open, so the engine is free for everyone to use, forever. See
[`docs/clean-room.md`](docs/clean-room.md) for the rules we follow.

We also build on outstanding open-source work, including [Jolt Physics](https://github.com/jrouwe/JoltPhysics),
[Recast & Detour](https://github.com/recastnavigation/recastnavigation),
[Wasmtime](https://github.com/bytecodealliance/wasmtime), [Slang](https://github.com/shader-slang/slang), [ufbx](https://github.com/ufbx/ufbx) and [Tauri](https://github.com/tauri-apps/tauri). Thank you.

<div align="center">

**Built in the open by humans and AI agents, in concord.** ✦

</div>
