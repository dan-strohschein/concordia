# Later backlog
Status: Backlog · Depends on: varies per item · Detail level: Backlog (one entry per item) · Estimated effort: not estimated

Work that is deliberately outside M-1…M5 but that the design must not block. Each item gives a short rationale and the **earliest milestone it could start after**. "Start after" is a floor, not a schedule. When an item is picked up, it gets its own plan in `plans/` using the milestone template in `plans/README.md`, and an ADR if it changes `docs/design.md`.

Rules that still apply to everything here: subscription-first (no item may *require* an LLM API key), clean room, TDD with an executable gate, text as source of truth, every mutation through `concordia-command` with an `Actor`.


> **Evolve (M6):** items below described as "optional plugin crates" (Steamworks, MetalFX/DLSS, relays, local-model providers) ship as Evolve plugins once M6 lands.

## 0. Plugins
- **Evolve plugin registry** (after M6). Public index of plugins with signing, yanking, version search and license metadata; `concordia plugin add <id>` resolves through it. Needs ADR-0141 trust rules.
- **Binary native plugin distribution** (after M6). Prebuilt plugins per platform, signed; needs a stable native ABI story beyond M2's C-ABI vtable.
## 1. Large worlds and streaming
- **World partition and cell streaming** (after M4). Split scenes into spatial cells streamed by distance and budget; `.scene.ron` gains cell references. Needs the asset DDC (M1) and virtualized geometry (M4) to be worth doing at scale.
- **Large-world coordinates** (after M1). f64 world positions with camera-relative rendering or origin rebasing; decide early because it touches `concordia-core` math types and the scene format.
- **HLOD and proxy-mesh generation** (after M4). Automatic merged far-field proxies per cell, cached in `.concordia/ddc`; builds on meshoptimizer cluster LODs.
- **Streaming-aware physics and navigation** (after M2). Jolt bodies and Recast tiles load/unload with cells; navmesh tiles stitched across cell borders.
- **Data layers** (after M3). Toggleable scene layers (e.g. quest states, seasons) as text, scriptable over the Engine API.
- **Procedural zone pipeline at scale** (after M2). the reference port's procedural zones/biomes as a first real large-world consumer.

## 2. Multiplayer and replication
- **Replication model on top of the ECS and command log** (after M3). Reflected components (`concordia-reflect`) declare replication; the command log is the natural unit for authoritative updates.
- **Client prediction and rollback** (after M3). The fixed-step deterministic loop (M0) and replay (M3) make rollback netcode feasible; determinism tests extend across machines.
- **Dedicated headless server builds** (after M0). The engine already runs headless; package a server target without renderer crates.
- **Transport** (after M3). QUIC (e.g. `quinn`, MIT/Apache) for reliable/unreliable channels; platform relay (Steam Networking) as an optional plugin.
- **Network test harness** (after M3). Multiple headless instances in one test with simulated latency/loss, asserted through `replay.run` and `assert.eval`.

## 3. Platforms: D3D12 and consoles
- **D3D12 RHI backend** (after M1; realistically after M4 when the RHI is stable). `concordia-rhi-d3d12` behind the existing RHI traits; Windows parity tested with golden images.
- **Vulkan backend maturity and Linux/Windows parity** (after M1). `concordia-rhi-vulkan` is M1 stretch / M2; parity CI on Linux and Windows runners with GPU.
- **Console support via a private NDA layer** (after M4). Platform code lives in separate private repositories that implement public platform/RHI traits; nothing NDA-covered ever enters the open repo. Needs a stable platform boundary and governance (section 8).

## 4. Mobile and XR
- **iOS / iPadOS** (after M1). Metal backend reuse; needs touch input in `concordia-input`, mobile-tier render paths, app packaging.
- **Android** (after the Vulkan backend). Vulkan RHI plus Android activity glue; thermal and memory budgets drive a mobile render profile.
- **OpenXR** (after M4). Stereo rendering in the render graph, XR input actions; Vision Pro via visionOS APIs as a separate investigation.

## 5. AA team workflows
- **Multi-user editing** (after M3). Several editors and agents share one session over the Engine API; the command log with actor attribution becomes the sync stream; conflict handling via locks or CRDT/OT on canonical text scenes.
- **Scene diff and merge tooling** (after M0). A git merge driver and semantic diff for `.scene.ron`/`.prefab.ron` using stable UUIDv7 ids, so text-first scenes merge cleanly.
- **Large binary assets: Git LFS** (after M1). LFS rules for `.glb`, `.png`, `.wav` etc.; the importer and DDC must work with LFS pointers and partial checkouts.
- **Perforce (Helix Core) integration** (after M3). Checkout/lock hooks for the editor and agents; many AA studios require it.
- **Shared derived-data cache and build farm** (after M1). Remote content-addressed DDC so teams and CI don't re-import or recompile shaders.
- **Review and approval workflows** (after M3). Studio approval gates extended to human reviewers in a team, with per-folder ownership.

## 6. Localization and accessibility
- **String tables and localization pipeline** (after M2). Text-based string tables, ICU MessageFormat via `icu4x` (Unicode-3.0 license), pseudo-localization for testing, agent-assisted translation through the user's harness.
- **Text shaping, bidi and font fallback** (after M2). `concordia-ui` text via `cosmic-text`/`rustybuzz` (MIT/Apache) for RTL and complex scripts.
- **In-game accessibility** (after M2). Subtitles and captions, full input remapping, colorblind modes, scalable UI, reduced motion; expose as engine settings with tests.
- **Editor accessibility** (after M1). Keyboard navigation and screen-reader support in the React UI; native viewport described via AccessKit where applicable.

## 7. Agent Studio: harnesses and runners
- **Adapters for other MCP-capable harnesses** (after M3). `studio sync` targets beyond Claude Code (e.g. Codex CLI, Gemini CLI, Cursor, opencode) from the same role configs; root `AGENTS.md` is already harness-neutral.
- **Optional direct-API runner** (after M3). Opt-in plugin that orchestrates roles with the user's own API key or a local model; never required, never default (ADR-0002).
- **Local-model provider plugins for generated content** (after M3). `concordia-gen` providers for local image/3D/audio models, opt-in.
- **Studio analytics** (after M3). Per-role throughput, review rejection rates, budget use, visualized in the editor's Studio panel.

## 8. Name and governance
- **Engine rename** (after M3, before the first public release). Pick a name, check trademarks and crate/domain availability, then do the mechanical rename from ADR-0008 (`concordia-` prefix, `concordia` binary, `concordia:game` WIT package, `concordia.toml`).
- **Foundation governance** (once there are outside maintainers). Choose a home (e.g. an existing open-source foundation or fiscal host), write a charter, maintainers file, RFC process, code of conduct and security policy.
- **Public release readiness** (after M3). Contributor docs, issue templates, security disclosure process, Unreal plugin-terms review for `tools/unreal-exporter` (see `docs/clean-room.md`).

## 9. Tool integrations from the author's ecosystem
- **sound-catalog export target** (after M2). Add an "concordia" exporter to the Swift sound-catalog app: copies audio and writes `<file>.import.ron` sidecars with stable asset ids and catalog metadata, consumed by `concordia-assets` / `concordia-audio`.
- **Level-editor equivalent** (after M2). Replace the reference port's Godot level-editor plugin (grid painting of navmesh levels) with an editor panel over the Engine API plus an importer for its level resources; agents can paint levels through the same API.
- **Blender bridge** (after M1). A Blender add-on (Python; GPL-licensed add-on code stays a separate optional tool per the clean-room policy) or MCP-driven workflow to export straight into a project with sidecars.

## 10. Performance and profiling tools
- **CPU profiling** (after M0). `tracing` spans exported to Tracy (`tracing-tracy`) and Chrome trace JSON; `profile.capture` Engine API method so agents can read profiles.
- **GPU profiling** (after M1). Timestamp queries per render-graph pass, Metal frame-capture hooks, per-pass budgets shown in the editor.
- **Memory profiling** (after M1). Allocation tracking per subsystem and asset; leak checks in CI for headless runs.
- **Performance regression CI** (after M1). Benchmarks (`criterion` or `divan`) and frame-time captures of reference scenes, with thresholds; agents get structured reports when a change regresses.
- **Lit-scene cost at scale in the editor** (after M1; author priority: large scenes must not destroy performance). The M1 gate scene (16 point lights, an HDRI sky, materials) drops the 10k-entity browser-style orbit from 107 to 92 fps median (CPU p50 1.49 → 1.90 ms, GPU p50 4.44 → 4.88 ms, about twice the missed 120 Hz refreshes; measured 2026-11-19, see the M1 plan, 1.16 As built). Profile where the time goes (light culling, the shadow and IBL passes, CPU extraction per frame), set a budget for a large lit scene and add it to the bench and perf CI.

## 11. Packaging and shipping
- **Cooking and packaging** (after M2). `concordia package`: cook assets from the DDC into platform packs, strip editor-only data, produce a runnable build per platform.
- **Steam distribution** (after the packaging item). Steamworks (achievements, cloud saves, overlay, depots) as an optional plugin crate, because the SDK is proprietary and must not be a core dependency; SteamPipe upload scripts.
- **Code signing and notarization** (after the packaging item). macOS notarization, Windows signing, reproducible build manifests.
- **Crash reporting** (after the packaging item). Minidumps plus symbol upload; backend pluggable and opt-in.
- **Mod packaging** (after M2). WASM components plus content packs, sandboxed by the M2 scripting host; mod manifests as text.

## 12. Rendering and simulation beyond M4
- **Hardware ray tracing and a reference path tracer** (after M4). Ground truth for golden images and GI validation.
- **Upscaling and frame generation** (after M4). FSR (MIT) in core; MetalFX and DLSS as optional platform plugins.
- **Terrain, foliage, water, volumetrics** (after M4). Each a separate plan; terrain ties into world partition.
- **Motion matching, full-body IK, physics-based animation** (after M2, realistically after M4). Extends `concordia-anim`.
- **Web export via WebGPU** (after M1). Investigate a `wgpu`-style backend or WebGPU RHI for browser demos; WASM host already exists.
- **Jolt GPU strand hair** (after M8). Wrap Jolt's Cosserat-rod hair (v5.6, work in progress upstream) once stable, bridging its compute interface to `concordia-rhi`.
- **Flesh, muscles and panel-sewn garments** (after M8). Volumetric soft tissue and 2D-pattern garment authoring.
- **Networked destruction** (with multiplayer). Replicate breaks and debris.
- **Asynchronous browser-viewport captures** (after M1). `capture.screenshot` renders and reads back on the engine thread, so in the browser path the editor's other calls wait behind viewport frames (seconds each on a virtual GPU). Queue the readback on the render thread and reply when it lands, as `viewport.present` already does.
