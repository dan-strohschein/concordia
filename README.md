<div align="center">

# ✦ Concordia

### The game engine your AI team already knows how to use.

**Open source · Written from scratch in Rust · AI-first · Aiming for Unreal-class results**

[![CI](https://github.com/dan-strohschein/concordia/actions/workflows/ci.yml/badge.svg)](https://github.com/dan-strohschein/concordia/actions/workflows/ci.yml)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)
![Rust](https://img.shields.io/badge/rust-1.100-orange)
![macOS](https://img.shields.io/badge/macOS-available-success)
![Linux](https://img.shields.io/badge/Linux-mid--2027-lightgrey)
![Windows](https://img.shields.io/badge/Windows-mid--2027-lightgrey)
![No API key](https://img.shields.io/badge/LLM%20API%20key-not%20required-brightgreen)

</div>

> [!NOTE]
> **"Concordia" is a code name.** The engine gets its real name at launch. Commands, crates and
> paths that say `concordia` will be renamed then, and this README will tell you how to migrate.

> [!WARNING]
> **Pre-alpha.** Concordia is under heavy, test-driven development. The headless core, the renderer,
> the editor and Agent Studio work today; gameplay systems are landing phase by phase (see
> [Roadmap](#-roadmap)). It is not ready to ship a game yet. It is ready to explore, break and help build.

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
- **Scripting in three tiers:** native Rust with hot reload; sandboxed WASM components (AOT-compiled
  on Wasmtime, API defined once in WIT, so you can write gameplay in Rust, C/C++, Zig, Go, Swift or
  AssemblyScript); and, on the roadmap, visual graphs stored as text.

### 🎨 Rendering, physics and the rest

- **A custom RHI and render graph** (Metal today, Vulkan next), **Slang** shaders, clustered forward
  PBR, cascaded shadow maps and image-based lighting.
- **Substrate-style layered materials:** slabs with haze, fuzz, subsurface and thin film, combined by
  layering, mixing and coverage under a per-pixel closure budget.
- **Jolt physics** with character controllers, sensors, contact events and raycasts.
- **Animation:** GPU skinning, clips, blend trees and state machines with Godot `AnimationTree`
  semantics, and retargeting by bone name.
- **Navigation:** Recast/Detour navmeshes, click-to-move, crowds with avoidance and LOD, and lane traffic.
- **Import:** glTF and FBX today, with Unity, Unreal and Godot importers on the roadmap.
- **Audio** (in progress right now): our own mixer and spatial audio.
- **Editor:** React panels around a native viewport in a Tauri shell, laid out like Godot. It edits the
  same live scene your agents do.

---

## ⚖️ How it compares

Unreal, Unity and Godot are excellent, mature engines with huge communities, and today they can do far
more than Concordia can. Here is where we are going differently.

| | **Concordia** | **Unreal** | **Unity** | **Godot** |
|---|---|---|---|---|
| **License** | MIT or Apache-2.0 | Source-available, proprietary EULA | Proprietary | MIT |
| **Royalties / fees** | None, ever | Royalty above a revenue threshold | Paid tiers above a revenue threshold | None |
| **Core language** | Rust | C++ | C++ (closed) | C++ |
| **Gameplay code** | Rust, WASM (many languages), visual graphs (planned) | C++, Blueprints | C# | GDScript, C#, GDExtension |
| **Designed for AI agents** | ✅ From day one: MCP, schemas, headless, replay, attribution | ➖ Via plugins | ➖ Via plugins | ➖ Via plugins |
| **AI dev team built in** | ✅ Agent Studio | ❌ | ❌ | ❌ |
| **Needs an LLM API key** | ❌ Never | n/a | n/a | n/a |
| **Scenes as diffable text** | ✅ Always (canonical RON) | ❌ Binary `.umap` | ➖ YAML with "Force Text" | ✅ `.tscn` |
| **Every edit attributed and undoable** | ✅ Command log | ➖ Editor undo | ➖ Editor undo | ➖ Editor undo |
| **Deterministic replay** | ✅ Across thread counts | ➖ Replay system, not deterministic | ❌ | ❌ |
| **High-end rendering** | 🛠️ On the roadmap | ✅ Nanite, Lumen | ✅ HDRP | ➖ Improving |
| **Large-scale simulation** | 🛠️ Terra (Mass-style), in progress | ✅ Mass | ✅ DOTS/ECS | ➖ |
| **Platforms** | macOS now; Linux and Windows mid-2027 | Everything | Everything | Everything |
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

### 🐧 Linux (coming mid-2027)

The headless engine already builds and tests on Linux in CI. The Vulkan renderer, editor and
installer arrive in mid-2027. The plan:

```sh
# Planned, not yet available
curl -fsSL https://<launch-domain>/install.sh | sh     # plus .deb, .rpm and Flatpak packages

# Build from source
git clone https://github.com/dan-strohschein/concordia.git && cd concordia
just doctor        # checks Vulkan drivers, Rust, Node and the rest
cargo build --workspace
just test
cargo install --path crates/concordia-cli
```

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
| **M2** Engine breadth | Terra Mass core, scripting, replay, materials, FBX, animation, navigation, crowds, traffic, **audio**, UI | 🛠️ In progress |
| **M3** Closed loop & Studio v1 | Playtest assertions, generated-content plugins | Planned |
| **M4** Unreal-class visuals | Meshlets with GPU culling, virtual shadow maps, dynamic GI, visual graphs, runtime AI | Planned |
| **M5** Bring your game | Unity importer, Unreal exporter, Godot port | Planned |
| **M6** Evolve | Native and WASM plugins whose APIs become MCP tools | Planned |
| **M7** Terra at scale | 100,000 crowd agents at 60 Hz, deterministic | Planned |
| **M8** Advanced physics | Destruction and GPU garments | Planned |
| **M9** Photon | Fully dynamic GI and reflections | Planned |
| **M10** Advanced shadows | Tiled shadow atlas with static caching | Planned |
| **M11** Sparx | GPU particles written in Slang | Planned |
| **M12** Rigging | Procedural rigs and in-engine keyframing | Planned |
| **Mid-2027** | **Linux and Windows** | Planned |

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
