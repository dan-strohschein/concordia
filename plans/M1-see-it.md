# M1: See it
Status: Planned · Depends on: M0 (headless core) · Detail level: Ready to execute · Estimated effort: 16–22 weeks (author + agents), see [Effort estimate](#effort-estimate)

## Goal & context
M0 gave us a headless engine: ECS, reflection, command log, RON scenes, the Engine API (JSON-RPC 2.0) with its MCP adapter, and the `concordia` CLI. M1 makes the engine **visible and physical**. It adds a window, input, a thin future-proof RHI with a Metal backend, Slang shaders, a render graph, clustered forward+ PBR with shadows and IBL, an asset pipeline with glTF import, Jolt physics, and a minimal editor that is just another Engine API client.

M1 also closes the visual half of the agent loop (design principle 3). An agent can step a simulation headless, call `capture.screenshot`, and get back an image it can look at. Golden-image tests make "what it looks like" a regression-tested property from now on.

The first real game is the **reference port** (an isometric ARPG in Godot 4.6 Forward+), ported in M2. M1 therefore prioritizes:
- an **orthographic/isometric camera** as a first-class projection, with stable (non-shimmering) shadows while the camera pans;
- **many dynamic point/spot lights** (spell effects), so clustered light culling is in M1 rather than deferred;
- a directional sun with **cascaded shadow maps** tuned for a top-down view;
- MSAA rather than temporal AA (deterministic, golden-friendly, matches Godot Forward+'s MSAA option);
- glTF import that **preserves skins and animations** in the imported asset, even though skinned rendering arrives in M2 (`concordia-anim`), so M2 doesn't need to re-import;
- physics raycasts and a character controller (click-to-move needs pointer → world picking).

## Scope / Non-goals
**In scope**
- `concordia-platform`: winit windowing, DPI, raw window handles, file watching (debounced).
- `concordia-input`: actions, bindings (`.input.ron`), fixed-tick sampling, pointer ray, and the `input.inject` API.
- `concordia-rhi`: traits and types designed for bindless resources, explicit barriers, transient aliasing, timeline sync, and capability flags for M4 (mesh shaders, 64-bit atomics, visibility buffer).
- `concordia-rhi-metal`: full backend over `objc2-metal`. `concordia-rhi-vulkan` over `ash`: **stretch** (Phase 1.10).
- `concordia-shaders`: Slang → MSL (and SPIR-V for the stretch), reflection, Rust/Slang layout checks, content-addressed shader cache, hot reload.
- `concordia-render`: render graph with automatic barriers and transient aliasing; clustered forward+ PBR (glTF metallic-roughness); directional light with CSM; point and spot lights (unshadowed); IBL (split-sum); tonemapping (AgX, Khronos PBR Neutral, ACES fitted); MSAA 4×; perspective and orthographic cameras; an offscreen/headless path; `capture.screenshot`.
- `concordia-assets`: asset database, `<file>.import.ron` sidecars, importer registry, content-addressed DDC in `.concordia/ddc`, hot reload, `asset.import`.
- `concordia-import-gltf`: glTF 2.0 (`.gltf`/`.glb`), meshes, materials, textures, node hierarchy → scene/prefab, skins and animations stored but not yet played.
- `concordia-physics` + `jolt-sys`: Jolt via a C API; rigid bodies, shapes, layers, character controller (CharacterVirtual), raycasts and shape casts, ECS sync, deterministic fixed stepping, state save/restore.
- Minimal editor (Tauri v2): outliner, schema-driven inspector generated from `concordia-reflect`, native viewport. A spike validates ADR-0004's embedding approach, and a fallback is defined.
- Test infrastructure: RHI conformance tests, golden images with NVIDIA FLIP, headless GPU CI, physics determinism, glTF conformance on license-vetted Khronos sample models, and editor UI tests (vitest + Playwright).

**Non-goals (deferred)**
- Skinned mesh rendering and animation playback (M2, `concordia-anim`). M1 only imports and stores them.
- Point/spot light shadows (shadow atlas): M2 candidate, needed for the reference port spell lights. M1 shadows only the directional light.
- TAA, SSAO, SSR, bloom, volumetrics, decals, particles (M2+ as needed by the port). Bloom is the first post effect to add in M2.
- Virtualized geometry, visibility buffer, virtual shadow maps, GI (M4). M1 only makes sure the RHI can express them (capabilities, 64-bit atomics, mesh shader pipeline type stubs).
- Texture compression (BC/ASTC, KTX2/Basis). M1 imports RGBA8/RGBA16F with generated mips. Revisit in M2 when the reference port's texture volume is known.
- Windows/D3D12, Linux windowing polish, gamepads (gilrs is a stretch inside Phase 1.1; otherwise M2).
- Docking layout, gizmos beyond a translate gizmo, asset browser, material editor in the editor (M2/M3).
- USD import (M5 interchange work).

## Dependencies (what must already exist, from which milestone)
| From | What M1 uses |
|---|---|
| M-1 | Workspace, `rust-toolchain.toml`, `justfile` (`just test`, `just lint`, `just gate`), CI workflow skeleton, Xcode Metal toolchain, Node 24 + pnpm, `cargo-deny`, the `tests/` cross-crate test package (see *Proposed convention additions* for the name `concordia-tests`) |
| M0 `concordia-core` | glam math, `EntityId` (UUIDv7), fixed-step time, tracing, job system |
| M0 `concordia-reflect` | `#[derive(Reflect)]`, type registry, JSON Schema export (drives MCP schemas and the inspector) |
| M0 `concordia-ecs` | archetype ECS, schedules, fixed-step deterministic loop (`FixedUpdate` at 60 Hz by default), resources, events |
| M0 `concordia-command` | command log, transactions, undo/redo, `Actor` |
| M0 `concordia-scene` | `.scene.ron` / `.prefab.ron` read/write, canonical formatting |
| M0 `concordia-api` | JSON-RPC server (stdio + WebSocket), method registry, sessions, `sim.play/pause/step`, `component.get/set`, `schema.get`, `log.tail`, and a change-notification mechanism (if M0 shipped none, Phase 1.9 adds `events.subscribe`; see Proposed convention additions) |
| M0 `concordia-mcp` | MCP server binary mapping methods 1:1 to tools with reflect-generated input schemas |
| M0 `concordia-cli` | `concordia new/run/headless/...` (M1 makes `concordia run` open a window) |

## Crates, modules & directories (created / touched)
```
crates/
  concordia-platform/        NEW  src/{lib,window,event_loop,watch,dpi}.rs
  concordia-input/           NEW  src/{lib,action,binding,map,state,sample,inject,pointer}.rs
  concordia-rhi/             NEW  src/{lib,device,caps,resource,bindless,barrier,cmd,pipeline,sync,surface,format,conformance/}.rs
                               conformance/ is a pub test suite every backend runs (feature "conformance")
  concordia-rhi-metal/       NEW  src/{lib,device,heap,bindless,barrier,cmd,pipeline,surface,capture}.rs
  concordia-rhi-vulkan/      NEW (stretch)  src/{lib,device,alloc,descriptors,barrier,cmd,pipeline,swapchain}.rs
  concordia-shaders/         NEW  src/{lib,compiler,slang_ffi,reflect,layout_check,cache,watch}.rs
  concordia-render/          NEW  src/{lib,graph/{mod,builder,compile,alias,barriers,execute},camera,mesh,material,
                               texture,light,cluster,shadow/csm,ibl,tonemap,capture,headless,plugin}.rs
                               shaders/*.slang  (engine built-in shaders)
  concordia-assets/          NEW  src/{lib,id,sidecar,db,importer,ddc,handle,hot_reload,builtin}.rs
  concordia-import-gltf/     NEW  src/{lib,mesh,material,texture,scene,skin,anim,ids}.rs
  concordia-physics/         NEW  src/{lib,world,body,shape,layers,character,query,sync,hash,state}.rs
  jolt-sys/               NEW  build.rs (cmake), wrapper.h, src/lib.rs (bindgen output), shim/concordia_jolt_ext.{h,cpp}
  concordia-testkit/         NEW (dev-dependency only) golden-image compare (FLIP), bless, GPU test guards, test scenes
  concordia-api              TOUCH  capture.screenshot, input.inject, asset.import, asset.list, physics.raycast handlers
  concordia-mcp              TOUCH  MCP image content for capture_screenshot
  concordia-cli              TOUCH  `concordia run` (windowed), `concordia import`, `concordia headless --gpu`
  concordia-ecs / concordia-scene  TOUCH  new reflected components registered; no format changes expected
third_party/
  JoltPhysics/            NEW  vendored Jolt (MIT), pinned tag, LICENSE kept
  joltc/                  NEW  vendored Amer Koleci's joltc C API (MIT), pinned commit, LICENSE kept
editor/
  src-tauri/              NEW  Tauri v2 app (Rust) linking engine crates; viewport host
  ui/                     NEW  React + Vite + TS; panels: Outliner, Inspector, Viewport, Log
  ui/tests/unit/          NEW  vitest
  ui/tests/e2e/           NEW  Playwright
tests/
  gates/m1_physics_golden.rs, gates/m1_mcp_screenshot.rs   NEW
  golden/{rhi,render,gltf,gates}/*.png                      NEW
  fixtures/gltf-sample-assets/manifest.ron                  NEW (pinned commit, SHA-256s, licenses; models fetched, not vendored)
  fixtures/hdri/                                            NEW small CC0 HDRI (Poly Haven, ≤ 512×256 .hdr)
examples/
  m1-physics-stack/       NEW  concordia.toml, scenes/stack.scene.ron, materials/*.material.ron, input/default.input.ron
tools/
  slang/slang.toml        NEW  pinned Slang version + per-platform SHA-256
  slang/fetch.sh          NEW  downloads the pinned release into target/toolchain/slang/<ver>/
  ci/gpu-probe.rs or `concordia gpu-probe`  NEW  prints device caps (used in CI logs)
.github/workflows/ci.yml  TOUCH  macOS GPU job, Linux lavapipe job (stretch), editor UI job
justfile                  TOUCH  slang-fetch, bless, gate M1, editor-dev, editor-test, gltf-assets
```

## Phases
Phase order respects dependencies. 1.1 and 1.7 (physics) have no rendering dependency and can run in parallel with 1.2–1.6.

```
1.1 platform+input ─────────────────────────────┐
1.2 RHI+Metal ─► 1.3 shaders ─► 1.5 graph+capture ─► 1.6 lighting ─┬─► gate
1.4 assets+glTF (CPU side parallel to 1.2/1.3) ──┘                  │
1.7 physics (parallel) ─────────────────────────────────────────────┤
1.8 editor viewport spike (after 1.2) ─► 1.9 minimal editor ─────────┘
1.10 Vulkan backend (stretch, after 1.6)
```

---

### Phase 1.1: Platform & input
- **Objective:** Open a window, receive OS events, watch files, and turn raw input into deterministic per-tick action state that agents can drive with `input.inject`.
- **Deliverables**
  - `concordia-platform`: `Platform` wrapping a winit `EventLoop` via `ApplicationHandler`; `WindowDesc`, `WindowId`, `WindowEvent` (engine-owned enum, not winit's); `raw_window_handle` export for the RHI; DPI scale; `FileWatcher` (debounced) emitting `FileChanged { path, kind }` events into the ECS.
  - `concordia-input`: `InputMap` loaded from `*.input.ron`; `Action` kinds `Button`, `Axis1d`, `Axis2d`; bindings for keys, mouse buttons, mouse delta, scroll, pointer position, modifiers, and simple chords; `ActionState` resource (pressed, just_pressed, just_released, value) updated once per fixed tick; `PointerState` (window-space position and a world `PointerRay` computed from the active camera, including orthographic).
  - `input.inject` Engine API method (MCP `input_inject`).
  - `concordia run` opens a window showing a cleared color (the renderer comes later); `concordia headless` stays windowless.
- **Tests first**
  - `concordia-platform`
    - `window_desc_round_trips_through_reflect`: `WindowDesc` serializes to RON and back unchanged.
    - `watcher_reports_modified_file_once_after_debounce`: writing a temp file three times within 50 ms produces exactly one `Modified` event within 500 ms.
    - `watcher_reports_create_and_remove`: create then delete produce `Created` and `Removed` in order.
    - `watcher_ignores_paths_matching_ignore_globs`: changes under `.concordia/` and `target/` produce no events.
    - `headless_platform_creates_no_window`: `Platform::headless()` has no windows and no winit dependency at runtime (event loop never created).
  - `concordia-input`
    - `input_map_parses_default_bindings`: `examples/m1-physics-stack/input/default.input.ron` parses; insta snapshot of the canonical RON.
    - `button_action_reports_just_pressed_for_exactly_one_tick`.
    - `axis2d_from_wasd_is_normalized_on_diagonals`.
    - `events_between_ticks_are_applied_at_next_tick_boundary`: OS events timestamped between ticks N and N+1 affect tick N+1 only (determinism).
    - `press_and_release_within_one_tick_still_registers_just_pressed`: no lost taps.
    - `injected_click_produces_same_action_state_as_os_click`: an `input.inject` `mouse_button` event yields an `ActionState` identical to the OS path.
    - `inject_at_future_tick_is_applied_at_that_tick`: `{"tick": 10}` has no effect at tick 9 and takes effect at tick 10.
    - `inject_is_attributed_in_command_log`: the injection appears in `log.tail` with the caller's `actor` and is marked non-undoable.
    - `ortho_pointer_ray_is_parallel_to_camera_forward`: for an orthographic camera every pointer ray has the camera's forward direction; the origin moves with the cursor.
    - `perspective_pointer_ray_passes_through_cursor_pixel_center`.
  - `concordia-api` integration (`crates/concordia-api/tests/input_inject.rs`)
    - `input_inject_schema_is_generated_from_reflect`: `schema.get` for `InputInjectParams` matches an insta snapshot; no hand-written schema exists.
    - `input_inject_rejects_unknown_action_with_invalid_params`: JSON-RPC error `-32602` with the list of known actions in `data`.
- **Implementation notes**
  - Crates (verify versions at execution time): `winit` 0.30.x (`ApplicationHandler` API), `raw-window-handle` 0.6, `notify` 8.x + `notify-debouncer-full` 0.5.x, `globset` 0.4. Gamepads via `gilrs` 0.11 only if time allows (stretch).
  - Threading: on macOS, winit must own the main thread. The engine's simulation runs on a worker thread fed through a channel; the render thread submits on its own thread. `concordia-platform` documents this in `AGENTS.md` as an invariant.
  - Input sampling: OS events are pushed into an `InputQueue` with a monotonic timestamp; the fixed-step loop drains events with `t <= tick_time` at the start of each `FixedUpdate`. Injected events carry an explicit tick (or "next"), so headless runs are deterministic. M3's replay records the post-mapping `InputFrame` per tick; design the `InputFrame` type now (`Vec<(ActionId, ActionValue)>` sorted by `ActionId`) and keep it reflectable.
  - `input.inject` params (reflected Rust type, schema generated):
    ```rust
    #[derive(Reflect)] pub struct InputInjectParams { pub actor: Actor, pub events: Vec<InjectedEvent> }
    #[derive(Reflect)] pub struct InjectedEvent { pub tick: TickSpec /* Next | At(u64) */, pub kind: InjectedKind }
    #[derive(Reflect)] pub enum InjectedKind {
        Action { action: String, value: ActionValue },      // bypasses bindings
        Key { key: KeyCode, pressed: bool },
        MouseButton { button: MouseButton, pressed: bool },
        PointerMove { x: f32, y: f32 },                     // logical pixels in the viewport
        Scroll { dx: f32, dy: f32 },
    }
    ```
  - Command-log policy: input is not a world mutation, but every injection is logged as an attribution-only, non-undoable entry so reviewers see what an agent "pressed". (Flagged in Open questions for M0 alignment.)
- **Done criteria:** all tests above pass on macOS and Linux CI (input tests are headless); `concordia run examples/m1-physics-stack` opens a window that closes cleanly; `AGENTS.md` and `README.md` for both crates exist.

---

### Phase 1.2: RHI traits and Metal backend
- **Objective:** A thin, explicit RHI whose model already fits bindless rendering, explicit barriers, transient aliasing and M4's mesh shaders, visibility buffer and 64-bit atomics, implemented first on Metal and verified by a backend-agnostic conformance suite.
- **Deliverables**
  - `concordia-rhi`: traits and plain-data descriptors (no backend types leak).
  - `concordia-rhi-metal`: device, queues, buffers, textures, samplers, heaps, bindless table, graphics and compute pipelines, command encoding, barriers, timeline sync, `CAMetalLayer` surface, headless (no surface) device, readback.
  - `concordia_rhi::conformance` test suite (behind feature `conformance`), run by every backend's `tests/conformance.rs`.
  - `concordia gpu-probe` CLI subcommand printing `DeviceCaps` as JSON (used in CI logs).
  - CI: a macOS GPU job (see *CI decision* below) running the conformance suite on day one of the phase.
- **Tests first** (`crates/concordia-rhi/src/conformance/*.rs`, invoked from `crates/concordia-rhi-metal/tests/conformance.rs`; GPU tests use `concordia_testkit::gpu_test!`, which fails rather than skips when `CONCORDIA_REQUIRE_GPU=1`)
  - `device_reports_caps_and_limits`: `DeviceCaps` is populated; `max_bindless_textures >= 16_384`; serializes to JSON (snapshot per backend, redacting device name).
  - `buffer_upload_and_readback_round_trips`: 1 MiB of random bytes uploaded through a staging buffer read back identically.
  - `compute_dispatch_writes_expected_values`: a compute shader writes `out[i] = i * 3 + 1` over 65 536 elements; readback matches exactly.
  - `compute_reads_bindless_buffer_by_index`: shader receives only a `BindlessIndex` in push constants and sums the referenced buffer; result exact.
  - `triangle_renders_golden`: offscreen 256×256 RGBA8 sRGB, one vertex-pulled triangle (no vertex buffers bound; bindless storage buffer); compared to `tests/golden/rhi/triangle.png` with FLIP mean ≤ 0.001.
  - `clear_color_readback_is_exact`: clear to (0.25, 0.5, 0.75, 1.0) on RGBA8Unorm reads back `(64,128,191,255)`.
  - `depth_test_orders_overlapping_triangles`: reverse-Z (clear 0, `GreaterEqual`); nearer triangle wins regardless of draw order.
  - `msaa4x_resolve_produces_antialiased_edge`: edge pixels have intermediate values; golden `tests/golden/rhi/msaa_edge.png`.
  - `barrier_between_compute_write_and_fragment_read_is_respected`: compute writes a texture, a fragment pass samples it in the same command buffer; result correct across 100 iterations (catches missing barriers on untracked resources).
  - `transient_textures_aliased_in_same_heap_do_not_corrupt`: two non-overlapping-lifetime textures placed at the same heap offset each produce their own correct output.
  - `timeline_fence_signals_monotonically`: CPU waits on values 1..=10 submitted in order; `completed_value()` never decreases.
  - `readback_of_frame_n_does_not_block_frame_n_plus_1_submission`: frames in flight = 2.
  - `pipeline_creation_error_is_reported_not_panicked`: invalid MSL returns `RhiError::PipelineCompile { log }`.
  - `device_lost_is_reported` (Metal: simulated via a test hook) returns `RhiError::DeviceLost`.
  - Unit tests (no GPU): `format_table_block_sizes_are_correct`, `texture_desc_validation_rejects_zero_extent`, `barrier_access_merge_is_associative` (proptest).
- **Implementation notes**
  - **Crates (verify at execution time):** `objc2` 0.6.x, `objc2-foundation` 0.3.x, `objc2-metal` 0.3.x, `objc2-quartz-core` 0.3.x (for `CAMetalLayer`), `block2` 0.6.x, `raw-window-handle` 0.6. These are MIT/Apache-2.0/Zlib. `metal-rs` is deprecated in favor of `objc2-metal`; do not use it.
  - **Core types** (`concordia-rhi`):
    ```rust
    pub trait Device: Send + Sync {
        fn caps(&self) -> &DeviceCaps;
        fn create_buffer(&self, d: &BufferDesc) -> Result<Buffer>;
        fn create_texture(&self, d: &TextureDesc) -> Result<Texture>;
        fn create_heap(&self, d: &HeapDesc) -> Result<Heap>;             // for transient aliasing
        fn create_placed_texture(&self, heap: &Heap, offset: u64, d: &TextureDesc) -> Result<Texture>;
        fn create_graphics_pipeline(&self, d: &GraphicsPipelineDesc) -> Result<Pipeline>;
        fn create_compute_pipeline(&self, d: &ComputePipelineDesc) -> Result<Pipeline>;
        fn create_mesh_pipeline(&self, d: &MeshPipelineDesc) -> Result<Pipeline>;  // returns Unsupported unless caps.mesh_shaders
        fn bindless(&self) -> &BindlessTable;
        fn queue(&self, kind: QueueKind) -> Queue;                         // Graphics, Compute, Transfer
        fn create_timeline(&self, initial: u64) -> Result<Timeline>;
        fn create_surface(&self, w: RawWindowHandle, d: RawDisplayHandle) -> Result<Surface>;
    }
    pub struct BindlessIndex(pub u32);         // stable for the resource's lifetime
    pub struct DeviceCaps {
        pub backend: Backend, pub device_name: String, pub unified_memory: bool,
        pub max_bindless_textures: u32, pub max_bindless_buffers: u32,
        pub mesh_shaders: bool, pub atomic_u64_buffer: bool, pub atomic_u64_texture_min_max: bool,
        pub ray_tracing: bool, pub async_compute: bool, pub msaa_sample_counts: SampleCountFlags,
        pub texture_compression: TextureCompressionFlags, pub subgroup_size: (u32, u32),
    }
    ```
    Handles are typed, generation-checked indices (`Buffer`, `Texture`, `Pipeline`) owned by the device; destruction is deferred until the GPU timeline passes the last use.
  - **Bindless model:** one global table of sampled textures, storage textures, storage buffers and samplers. Every resource created with `usage.contains(BINDLESS)` gets a `BindlessIndex`. Shaders receive indices via push constants (≤ 128 bytes) or a per-draw `DrawData` storage buffer. Metal: a tier-2 argument buffer plus `MTLResidencySet` (macOS 15+) for residency; resources are created with `MTLHazardTrackingModeUntracked` so the RHI's barriers are authoritative. Vulkan (1.10): descriptor indexing (`VK_EXT_descriptor_indexing`, core in 1.2) with update-after-bind; descriptor buffers are a later optimization.
  - **Barrier model:** sync2-style. `Access { stages: PipelineStages, access: AccessFlags, layout: TextureLayout }`; `cmd.barrier(&[BufferBarrier], &[TextureBarrier])` with `before`/`after` `Access` and subresource ranges. The render graph (1.5) produces these; hand-written code may too. Metal maps to `memoryBarrierWithScope:afterStages:beforeStages:`, encoder boundaries and `MTLFence` for cross-encoder/cross-queue dependencies; layouts are no-ops on Metal. Aliasing barriers are explicit (`TextureBarrier::aliasing(prev, next)`).
  - **Sync:** `Timeline` maps to `MTLSharedEvent` / Vulkan timeline semaphores. A submission is `Submit { cmd_buffers, wait: &[(Timeline, u64)], signal: &[(Timeline, u64)] }`.
  - **M4 future-proofing:** mesh/task (object) pipeline descriptors exist now (`MeshPipelineDesc`), backed by Metal 3 object/mesh shaders when `caps.mesh_shaders`; `atomic_u64_*` caps are queried (Metal: MSL 3.1 64-bit atomics on Apple8+/M2+ family, verify which GPU families; Vulkan: `VK_KHR_shader_atomic_int64` + `VK_EXT_shader_image_atomic_int64`). `R64Uint` storage textures are in the format enum behind the cap. Indirect draws/dispatches (`draw_indexed_indirect_count`, `dispatch_indirect`) are in the trait now because GPU-driven culling needs them in M4. None of these are *used* in M1 except indirect dispatch in clustering.
  - **Metal 4:** macOS 26 ships Metal 4 (new command buffer/allocator model, argument tables, residency sets everywhere). The RHI model above (explicit barriers, residency, bindless tables) is deliberately close to it. M1 implements the Metal 3 API surface for coverage of macOS 14+; evaluating a Metal 4 code path is an open question.
  - **Headless:** `MetalDevice::new_headless()` uses `MTLCreateSystemDefaultDevice` with no surface; capture renders into an offscreen texture and copies to a shared buffer.
  - **CI decision (Metal):** GitHub-hosted macOS arm64 runners run in VMs exposing an Apple *paravirtual* Metal device. wgpu runs its GPU tests on the `macos-26` arm64 image, but reports conflict (some older images return `nil` from `MTLCreateSystemDefaultDevice`, and the paravirtual device has limited feature support). Decision: the macOS GPU CI job runs on `macos-26` (arm64) with `CONCORDIA_REQUIRE_GPU=1`, and runs `concordia gpu-probe` first. **The first deliverable of this phase is that probe job.** If the probe shows no usable device or missing features required by the gate (MSAA 4×, depth32float, argument buffers tier 2), fall back to a self-hosted Apple Silicon runner labelled `self-hosted-macos-gpu` for golden and gate jobs, and record the decision in ADR-0042.
  - **References:** Apple *Metal Feature Set Tables* and *Metal Shading Language Specification* 3.x; Apple WWDC "Go bindless with Metal 3" (2022) and "Discover Metal 4" (2025); Vulkan spec, synchronization2 chapter; Hans-Kristian Arntzen's blog posts on Vulkan synchronization (themaister.net); Microsoft "Enhanced Barriers" D3D12 spec (for the barrier vocabulary).
- **Done criteria:** conformance suite green on local Apple Silicon and on the CI macOS GPU job (or the self-hosted fallback); `concordia gpu-probe` output is attached to CI logs; no `unsafe` outside `concordia-rhi-metal` FFI modules, each `unsafe` block carrying a `// SAFETY:` comment.

---

### Phase 1.3: Shaders (Slang)
- **Objective:** Author all shaders in Slang; compile to MSL (SPIR-V for the stretch) with reflection, a content-addressed cache and hot reload; guarantee Rust/Slang struct layouts agree.
- **Deliverables**
  - `tools/slang/slang.toml` pinning a Slang release, plus `tools/slang/fetch.sh` and `just slang-fetch` that download the release tarball for the host (`slang-<ver>-macos-aarch64.tar.gz`, `slang-<ver>-linux-x86_64.tar.gz`), verify SHA-256 and unpack to `target/toolchain/slang/<ver>/`. `concordia-shaders` locates it via `SLANG_DIR` (set by `just`) or that default path.
  - `concordia-shaders`: `ShaderCompiler`, `ShaderModule` (target code + `ShaderReflection`), `ShaderCache` (DDC-backed), `ShaderWatcher` (hot reload).
  - Built-in shader library: `crates/concordia-render/shaders/` with `common/bindless.slang`, `common/color.slang`, and the conformance shaders moved from 1.2 into Slang.
  - A `#[derive(ShaderStruct)]` (in `concordia-shaders`) producing a layout descriptor for `#[repr(C)]` Rust structs, checked against Slang reflection.
- **Tests first**
  - `slang_toolchain_is_pinned_version`: the loaded library reports the version in `slang.toml`.
  - `compiles_compute_entry_to_msl`: `conformance/fill.slang` compiles to MSL containing a `kernel` function; reflection reports thread group size `[64,1,1]`.
  - `compiles_vertex_fragment_pair_to_msl`.
  - `compiles_to_spirv_and_validates` (runs `spirv-val` from the Slang/Vulkan SDK if present; required on the Linux job only).
  - `reflection_lists_push_constant_size_and_fields`: snapshot (insta) of `ShaderReflection` for `pbr_forward.slang`.
  - `rust_and_slang_struct_layouts_agree`: for every `#[derive(ShaderStruct)]` type registered (`CameraData`, `DrawData`, `LightData`, `ClusterParams`, `CascadeData`, `MaterialData`), field offsets and total size equal Slang's reflected layout. Adding a field on one side only fails this test.
  - `compile_error_reports_file_line_and_message`: a syntax error yields `ShaderError::Compile { diagnostics: Vec<Diagnostic { file, line, col, message }> }`.
  - `cache_key_changes_when_included_file_changes`: editing `common/color.slang` changes the key of every shader that includes it, and nothing else.
  - `cache_hit_skips_compiler`: second compile of the same source returns from `.concordia/ddc` (compiler call counter unchanged).
  - `hot_reload_swaps_pipeline_at_frame_boundary` (GPU): editing a fragment shader's output color in a temp copy changes the next captured frame; frame N in flight is unaffected.
  - `hot_reload_keeps_last_good_pipeline_on_error` (GPU): a broken edit leaves output unchanged and emits a diagnostic visible through `log.tail`.
- **Implementation notes**
  - **Obtaining Slang:** prebuilt release archives from `github.com/shader-slang/slang/releases` (Apache-2.0 WITH LLVM-exception, which is permissive and compatible). Pin a 2026.x release current at execution time and record its SHA-256 per platform. Building from source (CMake) is the fallback for unsupported hosts. The LunarG Vulkan SDK also bundles Slang, but we do not depend on it for macOS.
  - **Binding:** use the `shader-slang` crate (0.1.x, Rust bindings to Slang's COM-style API; verify maturity at execution time). It needs `SLANG_DIR` (or `SLANG_INCLUDE_DIR`/`SLANG_LIB_DIR`). If it lacks reflection coverage we need, fall back to invoking `slangc` as a subprocess with `-reflection-json` and parsing that JSON. Keep the dependency behind `ShaderCompiler` so either path works.
  - **Targets:** `metal` (MSL source) for Metal, `spirv` for Vulkan. On Metal, MSL is compiled at load time with `newLibraryWithSource`, and the resulting pipeline is cached by Metal's own cache. Precompiling to `.metallib` (via `xcrun metal`) for shipped games is a later optimization.
  - **Bindless in Slang:** use Slang's `DescriptorHandle<T>` / bindless resource support where it maps cleanly to Metal argument buffers and Vulkan descriptor indexing (verify current Slang support for the Metal target); otherwise a small `bindless.slang` module that indexes global arrays (`Texture2D g_textures[]`, `ByteAddressBuffer g_buffers[]`).
  - **Cache key:** `blake3(slang_version, target, entry, defines (sorted), compiler options, transitive include contents (sorted by path))`. Artifacts live in `.concordia/ddc/shaders/<k0k1>/<key>` (via `concordia-assets::Ddc`, built in 1.4; until then a local `DiskCache` with the same layout, swapped in 1.4).
  - **Hot reload:** `FileWatcher` events for `*.slang` → recompile on a worker → publish `ShaderReloaded { module }` → the render thread recreates dependent pipelines between frames.
  - **References:** Slang user guide (shader-slang.org), Slang reflection API docs, MSL spec.
- **Done criteria:** all conformance shaders are Slang; `just slang-fetch && cargo nextest run -p concordia-shaders` passes on a clean checkout; CI caches `target/toolchain/slang`.

---

### Phase 1.4: Assets, DDC and glTF import
- **Objective:** A text-first asset pipeline: source files plus `<file>.import.ron` sidecars, importers producing derived data in a content-addressed `.concordia/ddc`, stable asset ids, hot reload, and a conformant glTF 2.0 importer.
- **Deliverables**
  - `concordia-assets`: `AssetId` (UUIDv7, stored in the sidecar), `SubAssetId` (UUIDv5 of parent id + sub-path), `AssetRef<T>` (reflected; serializes as `"asset:<uuid>"` or `"asset:<uuid>#<sub-path>"`), `AssetDb`, `Importer` trait and registry, `Ddc`, `Handle<T>` with load states, `HotReload`.
  - Built-in assets with fixed ids: `builtin:mesh/cube`, `builtin:mesh/plane`, `builtin:mesh/sphere`, `builtin:material/default`, `builtin:texture/white|black|normal_flat`.
  - Image importer (`png`, `jpg`, `hdr`): RGBA8 sRGB/linear or RGBA16F, mip generation, normal-map flag.
  - `concordia-import-gltf`: meshes (positions, normals, tangents (MikkTSpace-generated if missing), UV0/UV1, colors, joints/weights), materials (metallic-roughness core + `KHR_materials_emissive_strength`, `KHR_texture_transform`, `KHR_materials_unlit`), textures and samplers, node hierarchy → a `.prefab.ron`-equivalent prefab asset, cameras, `KHR_lights_punctual`, skins and animation clips stored for M2.
  - `.material.ron` format (documented, reflected).
  - Engine API: `asset.import` (exists in naming conventions) and `asset.list` (new; see Proposed convention additions). CLI: `concordia import <path>`.
  - `just gltf-assets`: fetches the pinned glTF-Sample-Assets commit into `target/gltf-sample-assets/` and verifies SHA-256s from `tests/fixtures/gltf-sample-assets/manifest.ron`.
- **Tests first**
  - `concordia-assets`
    - `first_import_writes_sidecar_with_new_asset_id`: importing `crate.png` creates `crate.png.import.ron` with a UUIDv7 id and default settings; insta snapshot of canonical RON (id redacted).
    - `reimport_preserves_asset_id`.
    - `moving_file_with_sidecar_preserves_asset_id_and_refs`: scene references still resolve after moving both files.
    - `ddc_key_changes_with_source_settings_importer_version_and_platform`: each of the four inputs changes the key; nothing else does (proptest over settings).
    - `ddc_round_trip_is_byte_identical` and `ddc_write_is_atomic` (crash simulated between temp write and rename leaves no partial entry).
    - `ddc_entries_are_sharded_by_key_prefix`: `.concordia/ddc/<2 hex>/<64 hex>`.
    - `missing_ddc_entry_triggers_reimport_not_error`: deleting `.concordia/ddc` entirely is always safe.
    - `asset_ref_serializes_as_string_and_schema_is_string_with_pattern`: JSON Schema has `"pattern": "^asset:"` so MCP/inspector can validate.
    - `hot_reload_bumps_handle_generation_on_source_change`: editing a PNG on disk updates the handle's generation within 1 s and emits `AssetReloaded`.
    - `asset_import_via_api_is_attributed`: `asset.import` with `actor: agent:tech-artist#1` creates the sidecar as a command (undo removes the sidecar it created).
  - `concordia-import-gltf` (fixtures under `target/gltf-sample-assets`, see license policy below; small hand-made fixtures in `crates/concordia-import-gltf/tests/fixtures/`)
    - `imports_box_glb_with_24_vertices_and_36_indices` (`Box`).
    - `imports_binary_and_embedded_and_separate_variants_identically`: `.glb`, `.gltf` + `.bin`, data-URI variants produce identical mesh data hashes.
    - `generates_tangents_when_missing_matching_mikktspace` (`NormalTangentTest`; compare to reference tangents within 1e-4).
    - `material_factors_and_texture_slots_map_to_material_asset`: `MetalRoughSpheresNoTextures` produces 49 materials with expected metallic/roughness grid values.
    - `alpha_modes_map_to_opaque_mask_blend` (`AlphaBlendModeTest`).
    - `node_hierarchy_becomes_prefab_with_stable_sub_asset_ids`: re-importing yields the same `SubAssetId`s; reordering nodes in the source (hand-edited fixture) keeps ids matched by name.
    - `skins_and_animations_are_preserved_for_m2` (`SimpleSkin`, `Fox`): joint count, inverse bind matrices and clip names/durations stored.
    - `khr_lights_punctual_become_light_components`.
    - `unsupported_required_extension_fails_with_clear_error`: `extensionsRequired: ["KHR_draco_mesh_compression"]` returns `ImportError::UnsupportedExtension("KHR_draco_mesh_compression")`.
    - `import_is_deterministic`: importing twice produces byte-identical DDC blobs.
    - Golden renders of conformance models are in Phase 1.6 (they need the lighting).
- **Implementation notes**
  - **Crates (verify versions):** `gltf` 1.4.x (MIT/Apache), `image` 0.25.x (PNG/JPEG/Radiance HDR), `blake3` 1.x, `uuid` 1.x (`v5`, `v7`), `mikktspace` 0.3 (Zlib; or `bevy_mikktspace` if the original is unmaintained, verify), `fast_image_resize` or a box filter for mips (own code is fine), `tempfile` for atomic writes.
  - **Sidecar format** (`crate.png.import.ron`):
    ```ron
    (
        id: "0193f1c2-...-7abc",
        importer: "image",
        importer_version: 1,
        settings: (color_space: Srgb, generate_mips: true, normal_map: false),
        sub_assets: {},   // glTF: { "mesh/Body": "uuid", "material/Steel": "uuid", ... }
    )
    ```
  - **Sub-asset ids:** generated as UUIDv5(parent_id, "<kind>/<name-or-index>") on first import and **written to the sidecar**, so later imports match by name first, then index. This keeps scene references stable when an artist re-exports with reordered nodes.
  - **DDC:** `Ddc::get(key) -> Option<Bytes>`, `Ddc::put(key, bytes)`; blobs are versioned `postcard`-encoded (MIT/Apache, verify version) derived types (`MeshData`, `TextureData`, `MaterialData`, `PrefabData`, `SkinData`, `ClipData`). The DDC is binary and never committed (`.concordia/` is git-ignored), honoring "binary only in the build cache".
  - **Mesh format:** de-interleaved streams (position f32×3; normal+tangent packed oct-encoded; UVs f32×2; optional joints u16×4 and weights unorm16×4), u32 indices, per-mesh AABB and bounding sphere, meshlet data field reserved (empty) for M4. Vertex pulling from bindless buffers, so the format is free of fixed vertex layouts.
  - **glTF sample-asset license policy:** models are **fetched, not vendored**, from a pinned commit of `KhronosGroup/glTF-Sample-Assets`. Only models whose stated license is **CC0-1.0 or CC-BY-4.0** and that carry no trademark/"non-Khronos mark" flag are allowed; the manifest records each model's license and attribution, and CI fails if a listed model's metadata license changes. Initial allowlist (licenses as listed in the repo's `Models/Models.md`; re-verify per model at execution time): `Box` (CC-BY-4.0), `MetalRoughSpheres` (CC-BY-4.0), `MetalRoughSpheresNoTextures` (CC0), `NormalTangentTest` (CC0), `NormalTangentMirrorTest` (CC-BY-4.0), `AlphaBlendModeTest` (CC-BY-4.0), `OrientationTest` (CC-BY-4.0), `SimpleSkin` (CC0), `Fox` (CC0 + CC-BY-4.0), `BoomBox` (CC0), `FlightHelmet` (CC0). **Excluded:** `Duck` (SCEA Shared Source License), `DamagedHelmet` (includes a CC-BY-NC 4.0 component), `EnvironmentTest` (Adobe Stock license), and anything flagged with a non-Khronos mark (e.g. Cesium logo models `BoxTextured`, `CesiumMan`) unless the mark file is excluded. Golden PNGs rendered from CC-BY models are derivative images; `tests/golden/gltf/ATTRIBUTION.md` credits them.
  - **References:** Khronos glTF 2.0 specification and extension specs; glTF-Sample-Assets README; Morten Mikkelsen's MikkTSpace (mikktspace.com).
- **Done criteria:** all tests pass; `concordia import` on every allowlisted model succeeds; deleting `.concordia/ddc` and re-running reproduces identical blobs.

---

### Phase 1.5: Render graph, forward PBR baseline and headless capture
- **Objective:** A render graph with automatic barriers and transient aliasing; a first forward PBR pass (single directional light, no shadows yet); cameras including orthographic; headless offscreen rendering; `capture.screenshot` over the Engine API and MCP; the golden-image harness.
- **Deliverables**
  - `concordia-render::graph`: `RenderGraph`, `GraphBuilder`, `PassBuilder`, virtual resources (`GraphTexture`, `GraphBuffer`), imported resources (swapchain image, persistent history), compile step (culling, ordering, barrier generation, lifetime analysis, aliasing), execute step (records RHI commands, possibly multi-threaded per pass).
  - Render world extraction: an `Extract` stage copies render-relevant components from the ECS into a `RenderWorld` snapshot each frame, so the simulation and render threads don't share mutable state.
  - Components (all `#[derive(Reflect)]`, appear in `.scene.ron` and the inspector): `Camera { projection: Projection, clear: ClearColor, msaa: Msaa, exposure: Exposure, tonemap: Tonemap, priority: i32 }`, `Projection::{Perspective { fov_y, near, far }, Orthographic { height, near, far }}` (reverse-Z for both), `MeshRenderer { mesh: AssetRef<Mesh>, material: AssetRef<Material>, cast_shadows: bool }`, `DirectionalLight { color, illuminance_lux, shadows: Option<CascadeSettings> }` (shadows wired in 1.6).
  - `CaptureService` and the `capture.screenshot` method: renders the requested camera offscreen at the requested size (independent of window size), reads back, encodes PNG.
  - `concordia-mcp`: `capture_screenshot` returns **MCP image content** (see below).
  - `concordia-testkit`: `assert_golden(suite, name, &RgbaImage, Tolerance)`, FLIP compare, failure artifacts, `CONCORDIA_BLESS=1` / `just bless`.
- **Tests first**
  - Render graph (CPU-only, mock RHI recording commands into a list)
    - `unused_pass_is_culled`: a pass whose outputs nobody reads (and not marked `side_effect`) is not executed.
    - `passes_are_ordered_by_dependencies_not_insertion`.
    - `write_then_read_inserts_one_barrier_with_correct_access`: color-attachment write → fragment shader sampled read yields exactly one `TextureBarrier { before: ColorAttachmentWrite, after: FragmentSampledRead }`.
    - `read_after_read_inserts_no_barrier`.
    - `non_overlapping_transients_share_memory`: three transients with lifetimes [0,1], [2,3], [1,2] fit into two heap slots; the plan matches an insta snapshot.
    - `aliasing_inserts_aliasing_barrier_before_reuse`.
    - `imported_swapchain_ends_in_present_layout`.
    - `graph_compile_is_deterministic`: same graph built twice compiles to identical command lists (property test with random DAGs).
    - `graph_dump_to_dot_snapshot`: `RenderGraph::to_dot()` for the forward pipeline (insta), useful for agents to read.
  - Rendering (GPU, goldens in `tests/golden/render/`)
    - `unlit_cube_golden`: builtin cube, unlit material, perspective camera.
    - `pbr_sphere_grid_directional_golden`: 7×7 spheres varying metallic/roughness, one directional light, no IBL; FLIP mean ≤ 0.01.
    - `orthographic_isometric_camera_golden`: cube grid seen with yaw 45°, pitch −35.264° (true isometric), ortho height 20.
    - `ortho_and_perspective_agree_at_center_for_matching_framing`: center pixel of a lit plane matches within 1/255.
    - `reverse_z_has_no_z_fighting_on_distant_coplanar_quads`.
    - `msaa_resolve_matches_golden`.
  - Capture / API
    - `capture_screenshot_is_independent_of_window_size`: a 1280×720 capture with a 300×200 window returns 1280×720.
    - `capture_screenshot_headless_works_without_window` (engine started with `concordia headless --gpu`).
    - `capture_twice_without_sim_change_is_byte_identical`: same device, same scene: identical PNG bytes (determinism of our renderer on one device).
    - `capture_screenshot_reports_sim_tick_and_frame`.
    - `capture_screenshot_with_unknown_camera_returns_invalid_params`.
    - `concordia-mcp`: `mcp_capture_screenshot_returns_image_content`: `tools/call` result has `content[0] = { "type": "image", "mimeType": "image/png", "data": <base64> }` and a small `text` item with width/height/tick; `isError` false.
    - `mcp_capture_screenshot_schema_is_reflect_generated` (insta snapshot of input schema).
  - Golden harness (`concordia-testkit`)
    - `identical_images_have_zero_flip`.
    - `one_pixel_change_is_below_default_tolerance_but_large_shift_is_not`.
    - `failure_writes_actual_expected_and_flip_heatmap`: to `target/golden-failures/<suite>/<name>.{actual,expected,flip}.png`.
    - `bless_overwrites_only_with_env_set`.
    - `missing_golden_fails_with_bless_hint` (never auto-creates a golden in CI: `CI=true` forbids blessing).
- **Implementation notes**
  - **Render graph:** declarative passes (`builder.add_pass("forward", |p| { let c = p.write_color(hdr, Load::Clear(..)); p.read_texture(shadow, Stage::Fragment); ... ; move |ctx| { ... } })`). Compile: topological sort with stable tie-breaking by insertion index; reference counting for culling; per-resource access history → barriers; lifetime intervals → first-fit placement into per-frame `Heap`s, bucketed by memory requirements (greedy interval coloring is sufficient). Async compute is modeled (`QueueKind` per pass) but M1 runs everything on the graphics queue.
  - **References:** Yuriy O'Donnell, "FrameGraph: Extensible Rendering Architecture in Frostbite" (GDC 2017); Hans-Kristian Arntzen, "Render graphs and Vulkan — a deep dive" (themaister.net, 2017); Graham Wihlidal, "Halcyon: Rapid Innovation using Modern Graphics" (SEED, 2019).
  - **PBR baseline:** Cook-Torrance GGX/Trowbridge-Reitz D, Smith height-correlated visibility, Schlick Fresnel, Lambert diffuse, as in the glTF 2.0 spec Appendix B and Google's Filament material documentation (Apache-2.0); Brian Karis, "Real Shading in Unreal Engine 4" (SIGGRAPH 2013 course notes, a public talk, so allowed; no Unreal code is consulted). HDR target `RGBA16Float`, linear; exposure in EV100 (physical camera, Lagarde & de Rousiers, "Moving Frostbite to PBR", SIGGRAPH 2014).
  - **Capture API** (`capture.screenshot`):
    ```rust
    #[derive(Reflect)] pub struct CaptureScreenshotParams {
        pub camera: Option<EntityRef>,      // default: highest-priority active camera
        pub width: u32, pub height: u32,    // default 1280×720, max 8192
        pub target: CaptureTarget,          // Offscreen (default) | Viewport (editor/window as displayed)
        pub format: ImageFormat,            // Png (M1 only)
    }
    #[derive(Reflect)] pub struct CaptureScreenshotResult {
        #[reflect(mcp_content = "image")] pub image: EncodedImage, // { mime_type, data_base64 }
        pub width: u32, pub height: u32, pub sim_tick: u64, pub frame: u64,
    }
    ```
    `capture.screenshot` is read-only (no actor required, but one may be passed for logging). `concordia-mcp` maps any result field annotated `mcp_content = "image"` to an MCP `ImageContent` item and the remaining fields to a `TextContent` JSON item. This is the general mechanism (a reflect attribute), not a special case in the adapter; listed under Proposed convention additions.
  - **Tolerance:** `Tolerance { mean_flip: f32, max_p99_flip: f32 }`, default `{ 0.01, 0.25 }`. FLIP compare runs on the sRGB 8-bit output (LDR-FLIP). Crate: `nv-flip` 0.1.2 (MIT/Apache/Zlib Rust bindings to NVIDIA's FLIP, which is BSD-3-Clause; verify at execution time). Pixels-per-degree: library default (67).
  - **Golden variants:** one golden per test by default. If a backend legitimately differs beyond tolerance (e.g. lavapipe in 1.10), a variant `tests/golden/<suite>/<name>@<backend-class>.png` is allowed, chosen by `DeviceCaps::golden_class()` (`metal-apple`, `vulkan-lavapipe`). See Proposed convention additions.
- **Done criteria:** golden tests green on CI's macOS GPU job; `capture_screenshot` callable from Claude Code through `.mcp.json` against `concordia-mcp` (manual check recorded in the PR description); `RenderGraph::to_dot()` documented in `concordia-render/AGENTS.md`.

---

### Phase 1.6: Lighting: clustered lights, cascaded shadows, IBL, tonemapping
- **Objective:** The look the reference port needs: hundreds of dynamic point/spot lights, a shadowed sun stable under an isometric camera, image-based ambient lighting, and Godot-comparable tonemapping.
- **Deliverables**
  - Components: `PointLight { color, intensity_lumens, range }`, `SpotLight { color, intensity_lumens, range, inner_angle, outer_angle }`, `Environment { skybox: Option<AssetRef<Texture>>, ibl: AssetRef<EnvironmentMap>, intensity: f32 }`, `CascadeSettings { cascades: u8 (1..=4), max_distance, split_lambda, resolution, depth_bias, normal_bias, pcf: PcfKernel, stabilize: bool }`, `RenderSettings` resource (cluster grid dims, max lights).
  - Passes: depth prepass → light clustering (compute) → CSM shadow passes → forward opaque (clustered) → skybox → forward transparent (sorted back-to-front, same clustered lighting) → MSAA resolve → tonemap → output.
  - IBL preprocessing as an importer: `.hdr` equirect → `EnvironmentMap` asset (cubemap, GGX-prefiltered specular mips, SH9 irradiance) computed on the GPU at import and stored in the DDC; a BRDF LUT generated once and cached.
  - Tonemappers: `AgX` (default; Godot 4.4+ offers AgX, which keeps the port visually close), `KhronosPbrNeutral`, `AcesFitted`, `Linear`. Exposure: manual EV100.
  - glTF conformance goldens (`tests/golden/gltf/`).
- **Tests first**
  - Clustering (CPU reference vs GPU)
    - `cluster_assignment_matches_cpu_reference`: 1 024 random point lights; GPU light lists per cluster equal (as sets) to a CPU reference implementation for both perspective and orthographic cameras.
    - `ortho_camera_uses_linear_depth_slices`: slice boundaries for ortho are linear in view depth; perspective uses exponential slices (snapshot of boundaries).
    - `light_outside_frustum_is_in_no_cluster`.
    - `cluster_overflow_is_reported_not_silent`: more than `max_lights_per_cluster` lights in a cluster sets an overflow counter readable via `RenderStats` (and a log warning).
    - `many_lights_golden`: 512 colored point lights over a plane in the isometric view; golden `tests/golden/render/many_lights_iso.png`.
    - `spot_light_cone_falloff_golden`.
  - Shadows
    - `csm_split_distances_follow_practical_split_scheme`: for λ = 0.75, near 0.1, far 100, 4 cascades, splits match the formula within 1e-5.
    - `csm_stabilized_cascades_snap_to_texel_grid`: moving the camera by sub-texel amounts leaves each cascade's light-space origin on the texel grid (no shimmer).
    - `ortho_camera_csm_bounds_tightly_fit_view_volume`: cascade bounds for the iso camera contain the view frustum slice and are at most 1.5× its bounding-sphere area.
    - `box_on_plane_casts_shadow_golden`: `tests/golden/render/shadow_box_plane_iso.png`.
    - `shadow_shimmer_is_below_threshold_while_panning` (GPU): capture 30 frames while the iso camera pans by 0.37 texels per frame; per-pixel temporal variance inside shadowed regions stays below a threshold.
    - `no_shadow_acne_on_flat_plane_at_grazing_sun`: lit plane at 10° sun elevation has < 0.1% pixels darker than the unshadowed reference outside the true shadow.
  - IBL and tonemapping
    - `irradiance_sh9_of_uniform_environment_is_constant`: a constant-color HDR yields SH9 with only the DC term non-zero (± 1e-4).
    - `prefiltered_mip0_equals_source_for_roughness_zero`.
    - `brdf_lut_matches_reference_values` at 5 sample points within 1e-3 (reference values from Karis 2013 split-sum integration computed by a CPU test helper).
    - `tonemap_agx_maps_mid_grey_to_expected_srgb`: 0.18 linear → reference 8-bit value (from a CPU reference implementation in the test).
    - `khronos_pbr_neutral_preserves_base_colors_below_threshold`: per the Khronos PBR Neutral spec, colors below the compression start map to themselves.
    - `pbr_spheres_with_ibl_golden`.
  - glTF conformance goldens (FLIP mean ≤ 0.02 each; models from the allowlist; fixed camera framing the model's bounds)
    - `gltf_metal_rough_spheres_golden`, `gltf_metal_rough_spheres_no_textures_golden`, `gltf_normal_tangent_test_golden`, `gltf_normal_tangent_mirror_test_golden`, `gltf_alpha_blend_mode_test_golden`, `gltf_orientation_test_golden`, `gltf_boombox_golden`, `gltf_flight_helmet_golden`.
    - These goldens are reviewed by eye against the Khronos sample viewer screenshots (the reviewer records "looks equivalent" in the PR). We compare against our own blessed output, never against copied third-party screenshots.
- **Implementation notes**
  - **Clustered forward+:** froxel grid (default 16×9×24) in view space; depth slicing exponential for perspective and linear for orthographic (an iso ARPG camera has a shallow, near-constant depth range). Light culling in a compute pass: one thread group per cluster tests light bounding spheres/cones against cluster AABBs and writes compact index lists into a global buffer via an atomic counter; the fragment shader indexes its cluster by `(pixel / tile, slice(view_z))`. Budget: 4 096 lights per frame, 128 per cluster (configurable). References: Olsson, Billeter, Assarsson, "Clustered Deferred and Forward Shading" (HPG 2012); Emil Persson, "Practical Clustered Shading" (SIGGRAPH 2015 course); Tiago Sousa & Jean Geffroy, "The devil is in the details: idTech 666" (SIGGRAPH 2016).
  - **CSM:** practical split scheme (Zhang et al., "Parallel-Split Shadow Maps", 2006) blended by λ; per-cascade bounding sphere, texel snapping for stability (Michal Valient, "Stable Rendering of Cascaded Shadow Maps", ShaderX6); depth atlas `Depth32Float` 2 048² per cascade default, 3 cascades default for iso; reverse-Z; slope-scaled + normal-offset bias (Holbert, "Saying goodbye to shadow acne", GDC 2011 poster); PCF 3×3 or 5×5 with hardware comparison samplers (Castaño, "Shadow Mapping Summary" 2013 blog for optimized PCF). Cascade blending across seams.
  - **IBL:** split-sum approximation (Karis 2013); GGX importance-sampled prefiltering with mip-filtered samples (GPU Gems 3 ch. 20, Colbert & Křivánek); SH9 irradiance (Ramamoorthi & Hanrahan, "An Efficient Representation for Irradiance Environment Maps", SIGGRAPH 2001). Multi-scattering energy compensation (Fdez-Agüera, "A Multiple-Scattering Microfacet Model for Real-Time IBL", JCGT 2019) is a nice-to-have.
  - **Tonemapping:** AgX after Troy Sobotka's published AgX description and Blender's public configuration (check license of any LUT before use; prefer an analytic approximation written from the description); Khronos PBR Neutral per its public specification; ACES fitted after Stephen Hill's/Narkowicz's public curve fits.
  - **HDRI fixture:** one small CC0 HDRI from Poly Haven, downsampled to ≤ 512×256, committed in `tests/fixtures/hdri/` with its source URL and license in `tests/fixtures/hdri/LICENSE.md`.
- **Done criteria:** all goldens green; a perf smoke benchmark (`cargo bench -p concordia-render --bench many_lights`, not a gate) shows 1 024 point lights at 1080p ortho under 4 ms GPU time on the dev machine (record the number in `concordia-render/AGENTS.md`).

---

### Phase 1.7: Physics (Jolt)
- **Objective:** Deterministic rigid-body physics, character controller and scene queries through Jolt, synchronized with the ECS and driven by the fixed-step loop.
- **Deliverables**
  - `third_party/JoltPhysics` (pinned release tag, MIT) and `third_party/joltc` (pinned commit of Amer Koleci's `joltc`, MIT), each with LICENSE.
  - `jolt-sys`: CMake build of Jolt + joltc + our small shim `shim/concordia_jolt_ext.{h,cpp}`, `bindgen`-generated bindings committed to `src/bindings.rs` (regenerated by `just jolt-bindgen`, checked in CI for drift).
  - `concordia-physics`: `PhysicsWorld` resource, components `RigidBody { kind: Static|Dynamic|Kinematic, mass_override, friction, restitution, linear_damping, angular_damping, gravity_scale, ccd }`, `Collider { shape: ColliderShape, layer: PhysicsLayer, is_sensor, offset }`, `ColliderShape::{Box, Sphere, Capsule, Cylinder, ConvexHull(AssetRef<Mesh>), TriangleMesh(AssetRef<Mesh>), HeightField(..)}`, `CharacterController { height, radius, max_slope_deg, step_height, ... }`, `Velocity { linear, angular }`; events `CollisionStarted/Ended`, `SensorEntered/Exited`.
  - Systems in `FixedUpdate`: `sync_ecs_to_jolt` (create/destroy/update bodies, ordered by `EntityId`), `step_physics`, `sync_jolt_to_ecs` (write `Transform`, `Velocity`), `move_characters`. Render-side transform interpolation between fixed steps (`PreviousTransform`).
  - Queries: `raycast`, `raycast_all`, `shape_cast`, `overlap` with layer filters. Engine API `physics.raycast` (new method; useful for agents and click-to-move debugging).
  - `PhysicsWorld::state_hash()` and `save_state()/restore_state()` (Jolt `StateRecorder`), prepared for M3 replay and rollback.
- **Tests first** (no GPU needed; run on macOS and Linux)
  - `jolt-sys`
    - `jolt_version_matches_pinned_tag`.
    - `bindings_are_up_to_date`: regenerating bindings produces no diff (CI only).
    - `create_and_destroy_physics_system_does_not_leak` (counts via our shim's allocation hooks).
  - `concordia-physics`
    - `sphere_falls_under_gravity_matches_analytic_within_tolerance`: after 1 s (60 steps), y ≈ y0 − ½gt² within 1%.
    - `box_rests_on_static_plane`: after 3 s, linear speed < 1e-3 and it's above the plane.
    - `box_stack_of_10_is_stable_for_10_seconds`: top box displacement < 1 cm.
    - `restitution_one_sphere_bounces_to_near_original_height`.
    - `sensor_reports_enter_and_exit_events_once`.
    - `layers_filter_collisions`: bodies on non-colliding layers pass through.
    - `raycast_hits_nearest_body_with_correct_normal_and_entity`.
    - `raycast_respects_layer_filter`.
    - `ortho_pointer_ray_picks_ground_point` (integration with `concordia-input`): a click at the screen center of the iso camera hits the ground at the camera's look-at point.
    - `character_controller_climbs_step_below_step_height_and_not_above`.
    - `character_controller_slides_along_wall`.
    - `character_controller_does_not_walk_up_slope_steeper_than_max`.
    - `despawning_entity_removes_body`; `changing_collider_via_component_set_rebuilds_shape` (through `component.set` with an actor; undo restores the old shape).
    - Determinism (per conventions: same run twice and across thread counts, compare world-state hashes):
      - `same_simulation_twice_produces_identical_state_hash`: 100 random bodies, 600 steps.
      - `state_hash_identical_across_thread_counts_1_2_8`: Jolt job system with 1, 2 and 8 workers.
      - `spawn_order_in_scene_file_does_not_change_outcome`: bodies are created sorted by `EntityId`, so shuffling entity order in the `.scene.ron` gives the same hash.
      - `save_restore_then_step_equals_uninterrupted_run`: save at step 300, step to 600, restore, step to 600 again; hashes equal.
      - `box_stack_hash_matches_committed_snapshot`: the gate scene's 180-step hash equals an insta snapshot; this test runs on **both** macOS arm64 and Linux x86_64 CI, checking Jolt's cross-platform determinism mode. (If it differs across platforms, see Risks: snapshots become per-platform and an ADR records it.)
- **Implementation notes**
  - **C API decision.** Options researched:
    1. **Amer Koleci's `joltc`** (MIT; C API from the JoltPhysicsSharp project; `JPH_` prefix; CMake; broad coverage including CharacterVirtual and narrow-phase queries; also used by Odin and Dart bindings and by the `oxijolt-sys` crate, which tracks Jolt 5.6).
    2. **SecondHalfGames `JoltC`** (MIT/Apache-2.0; C wrapper of Jolt 5.6.0, self-described as work in progress) with the `joltc-sys`/`rolt` Rust crates (published crates lag at Jolt 5.0).
    3. **Our own thin C shim** covering only what we use.
    **Decision:** vendor (1) at a pinned commit together with the matching Jolt release, generate our own bindings in `jolt-sys`, and add a small own shim (`concordia_jolt_ext.cpp`) for anything missing or engine-specific (state save/restore to a byte buffer, determinism configuration, allocation hooks, contact listener batching into a ring buffer rather than per-contact callbacks into Rust). We do not depend on `rolt`/`oxijolt` because the safe API should be ours and match the ECS. If `joltc` lags a needed Jolt version, fall back to (3) by growing the shim; the `concordia-physics` API does not change.
  - **Build:** `jolt-sys/build.rs` uses the `cmake` crate (0.1.x) with `CROSS_PLATFORM_DETERMINISTIC=ON`, `DOUBLE_PRECISION=OFF` (single precision is enough for ARPG zones; revisit for world streaming), `OBJECT_LAYER_BITS=16`, `USE_ASSERTS` in debug, interprocedural optimization off for faster builds, and `-ffp-contract=off` (Jolt's documented requirement for cross-platform determinism; verify flags against the pinned Jolt's `Docs/` at execution time). `bindgen` 0.71+ (verify) runs in `just jolt-bindgen`, not on every build, so contributors need no libclang.
  - **Determinism rules** (documented in `concordia-physics/AGENTS.md`): fixed Δt only; bodies added in `EntityId` order; no `HashMap` iteration in sync code (use `BTreeMap` or sorted `Vec`); contact events sorted by `(EntityId, EntityId)` before being exposed to the ECS; Jolt's job system replaced by a shim job system backed by `concordia-core`'s job system (Jolt's design is deterministic regardless of thread count, which the tests verify).
  - **ECS/commands:** authoring changes (adding a `RigidBody`, editing a `Collider`) go through `concordia-command` with an actor. Simulation output (transforms written by `sync_jolt_to_ecs` while the sim is playing) is *simulation state*, not command-log entries; `sim.step`/`sim.play` themselves are logged commands. This matches M0's sim model (confirm, see Open questions).
  - **Hash:** `state_hash = blake3` over entities sorted by `EntityId` of `(id, position.to_bits(), rotation.to_bits(), linvel.to_bits(), angvel.to_bits())`.
  - **References:** Jolt Physics documentation and architecture notes (jrouwe.github.io/JoltPhysics), Jorrit Rouwé's GDC 2022 talk "Architecting Jolt Physics for Horizon Forbidden West"; Erin Catto's GDC physics talks for character/contact background.
- **Done criteria:** tests green on macOS arm64 and Linux x86_64; `cargo deny` passes with Jolt and joltc licenses recorded; `examples/m1-physics-stack` simulates in `concordia run` and in `concordia headless`.

---

### Phase 1.8: Editor viewport spike (validates ADR-0004)
- **Objective:** Within a fixed timebox (5 working days), prove or disprove that a native Metal viewport can be composited with a Tauri v2 webview on macOS with acceptable behavior, and pick the approach for Phase 1.9.
- **Deliverables**
  - `editor/spikes/viewport/` (throwaway, deleted after the decision, which is recorded in ADR-0043): a Tauri v2 app with a React panel layout and a rectangular "viewport slot" whose bounds are reported from the DOM (`ResizeObserver`) to Rust.
  - Candidate approaches, tried in order:
    - **A. Native subview under/over the webview.** Get the `NSWindow` from Tauri (`WebviewWindow::ns_window()`), insert an `NSView` backed by a `CAMetalLayer` into the content view hierarchy, positioned to the slot rect, with the webview above it and transparent in the slot (`transparent: true` + `macOSPrivateApi`), or the Metal view above the webview (input routing then goes to Rust directly).
    - **B. Tauri multi-webview.** Use Tauri v2's multiple-webviews-per-window (behind the `unstable` feature) to lay out panel webviews around a native region holding the Metal view.
    - **C. Docked child window (ADR-0004's stated fallback).** A borderless `NSWindow` attached with `addChildWindow:ordered:`, tracking the slot rect on move/resize.
    - **D. Universal fallback: streamed viewport.** The engine renders offscreen and streams frames into an `<img>`/`<canvas>` (PNG/JPEG or raw RGBA over the WebSocket API, or a Tauri custom protocol). Always works, including in a plain browser (which Phase 1.9's Playwright tests rely on), but adds latency.
  - A decision table with measurements, committed in ADR-0043.
- **Tests first** (spike-level, kept as manual measurement scripts plus one automated check)
  - `spike_viewport_tracks_slot_rect_on_resize` (automated, macOS): a Rust test launches the spike app with `--spike-autotest`, resizes the window through three sizes, reads back the native view frame, and asserts it equals the DOM slot rect (in points, at 1× and 2× scale) within 1 pt.
  - Manual measurement checklist (recorded numbers, not pass/fail): input-to-photon latency (high-speed phone camera or `CAMetalLayer` presented-time callbacks), frame pacing at 60/120 Hz (ProMotion), mouse capture and focus handoff between DOM and viewport, keyboard shortcuts while the viewport has focus, Retina scale changes when dragging between displays, full-screen and Stage Manager behavior, CPU/GPU cost of the webview while the viewport animates.
- **Implementation notes**
  - Accept A or B if: rect tracking is exact, no visible tearing or one-frame lag of the viewport behind panel resizes worse than one frame, and input focus can be routed predictably. Otherwise use C; D is always built anyway (it's needed for browser-based tests and for remote/agent viewing).
  - Crates: `tauri` 2.x, `tauri-build` 2.x, `objc2-app-kit` 0.3.x for `NSView`/`NSWindow` (verify versions). Tauri v2 is MIT/Apache-2.0.
  - References: Tauri v2 docs (window/webview APIs, `macOSPrivateApi`), Apple AppKit docs (`NSView`, `CAMetalLayer`, `addChildWindow:ordered:`).
- **Done criteria:** ADR-0043 records the measurements, the chosen approach and the fallback; ADR-0004's status line is updated from "shell choice confirmed in M1" to "confirmed (see ADR-0043)" as part of that change; the spike directory is removed.

---

### Phase 1.9: Minimal editor
- **Objective:** A Tauri v2 editor that is purely an Engine API client: outliner, schema-driven inspector generated from `concordia-reflect`, native viewport (approach from 1.8), log panel, undo/redo, play/pause/step.
- **Deliverables**
  - `editor/src-tauri`: hosts an in-process engine (`EngineHost`) and starts the Engine API WebSocket server on `127.0.0.1:<random port>` with a per-launch bearer token; injects port and token into the webview. Hosts the native viewport (or the streamed fallback). Contains **no editing logic**; all edits are Engine API calls from the UI with `actor: "human:local"`.
  - `editor/ui` (React, TypeScript strict, Vite, pnpm):
    - `api/` a typed JSON-RPC client (`EngineClient`) with request ids, timeouts, notification subscriptions; types generated from the engine's JSON Schemas (`json-schema-to-typescript` at build time, verify version) via `just editor-schemas` (output committed, drift-checked in CI).
    - `panels/Outliner` (tree from `query.run`, rename, reparent by drag, spawn/despawn, multi-select).
    - `panels/Inspector` driven by `schema.get`: widget registry mapping JSON Schema shapes and `x-concordia-*` hints (see Proposed convention additions) to editors: number (with range/step), bool, string, enum (oneOf), `Vec3`/`Quat` (euler display), color (linear, with sRGB picker), `AssetRef` (id + picker of `asset.list`), `EntityRef`, arrays, nested structs, `Option<T>`. Unknown shapes fall back to a raw JSON editor, never to a crash.
    - `panels/Viewport`: native slot or streamed image; camera controls (orbit/pan/zoom, ortho toggle, iso preset); click to select (via `physics.raycast` or a render pick ID pass; M1 uses raycast against colliders plus mesh AABBs); a translate gizmo (the only gizmo in M1).
    - `panels/Log` (`log.tail`), toolbar (play/pause/step, undo/redo, save).
  - `just editor-dev` (Vite + Tauri dev), `just editor-test` (vitest + Playwright), `just editor-build`.
- **Tests first**
  - vitest (`editor/ui/tests/unit/`)
    - `concordia_client_correlates_responses_by_id_out_of_order`.
    - `concordia_client_rejects_on_timeout_and_jsonrpc_error`.
    - `schema_to_widget_maps_vec3_number_enum_color_assetref` (table-driven over fixture schemas taken from real `schema.get` output, snapshot).
    - `unknown_schema_shape_falls_back_to_json_editor`.
    - `inspector_emits_component_set_with_actor_human_local_on_commit` (mock client; one call per committed edit, not per keystroke).
    - `outliner_builds_tree_from_parent_links_and_sorts_stably`.
  - Playwright (`editor/ui/tests/e2e/`), **browser mode**: Vite preview in Chromium connected to `concordia headless --gpu --api ws://127.0.0.1:0 --project examples/m1-physics-stack` started by the test fixture (streamed viewport, approach D). This is the same UI code as in Tauri; only the transport host differs.
    - `outliner_lists_scene_entities`: names match `query.run` results.
    - `selecting_entity_shows_its_components_in_inspector`.
    - `editing_transform_x_updates_engine_and_is_logged_with_human_actor`: after editing, `component.get` over a separate API connection returns the new value, and `log.tail` shows `human:local`.
    - `undo_shortcut_calls_txn_undo_and_restores_value`.
    - `agent_edit_appears_live_in_inspector`: a second API client acting as `agent:test#1` changes a component; the open inspector updates within 500 ms (proves the editor is just another client).
    - `step_button_advances_sim_and_viewport_image_changes`: viewport image hash changes after stepping the physics scene 30 ticks.
    - `viewport_click_selects_box_under_cursor`.
  - Rust (`editor/src-tauri/tests/`)
    - `api_server_rejects_connections_without_token`.
    - `editor_shell_smoke_renders_native_viewport` (macOS GPU job only): launches the built app with `--smoke-test`, waits for 10 presented frames, calls `capture.screenshot { target: Viewport }`, asserts a non-uniform image, exits 0.
- **Implementation notes**
  - Why browser mode for e2e: Playwright cannot drive Tauri's WKWebView on macOS, and `tauri-driver` (WebDriver) does not support macOS. Driving the same UI in Chromium against a real headless engine tests everything except the native shell, which the smoke test covers.
  - Libraries (verify versions at execution time): React 19, Vite 7, TypeScript 5.x, vitest 3.x, `@playwright/test` 1.5x, `@tauri-apps/api` 2.x, Zustand for UI state. Keep dependencies few; panels are simple.
  - Inspector edits are batched into one transaction per user gesture (`txn.begin` / `component.set`… / `txn.commit`), so a drag of the gizmo is one undo step.
- **Done criteria:** `just editor-test` green in CI (Linux job for vitest + Playwright against a headless engine using lavapipe if 1.10 landed, otherwise the macOS GPU job); the smoke test green on macOS; `editor/AGENTS.md` documents "no editor-only logic" and how to add a widget.

---

### Phase 1.10 (stretch): Vulkan backend and Linux software-GPU CI
- **Objective:** A second RHI backend proving the abstraction, and GPU tests on Linux CI without a GPU.
- **Deliverables:** `concordia-rhi-vulkan` over `ash` (0.38.x, verify) with `gpu-allocator` (0.27.x, verify) for memory (supports placed/aliased allocations); Vulkan 1.3 baseline (dynamic rendering, synchronization2, timeline semaphores, descriptor indexing); swapchain via `ash-window`/`raw-window-handle`; SPIR-V from 1.3's Slang path. CI job `linux-lavapipe` on `ubuntu-24.04` (or newer) installing Mesa's `mesa-vulkan-drivers` (lavapipe, a CPU Vulkan driver) and running conformance + render goldens with `CONCORDIA_BACKEND=vulkan`. On macOS the Vulkan backend can run through MoltenVK for local cross-checks only (not CI-required).
- **Tests first:** the same `concordia_rhi::conformance` suite instantiated in `crates/concordia-rhi-vulkan/tests/conformance.rs`; `validation_layers_report_zero_errors` (runs conformance with `VK_LAYER_KHRONOS_validation` enabled and a debug-utils messenger that fails on any error); render goldens rerun with the `vulkan-lavapipe` golden class (variants only where FLIP exceeds tolerance, each variant justified in the PR).
- **Implementation notes:** lavapipe is chosen over SwiftShader because Mesa packages are a one-line install on Ubuntu runners and lavapipe tracks recent Vulkan versions; SwiftShader remains a fallback. wgpu's CI uses a Mesa software install on Ubuntu for its Vulkan tests, which is evidence this works on hosted runners. References: Vulkan 1.3 spec; Khronos synchronization examples wiki.
- **Done criteria:** conformance + render goldens green on lavapipe in CI. If not done in M1, this phase moves to M2 unchanged.

---

## Milestone gate (the executable acceptance test: exact command and what it asserts)

**Command:**
```
just gate M1
```
which runs (recipe added to the `justfile`):
```
CONCORDIA_REQUIRE_GPU=1 cargo nextest run -p concordia-tests --no-fail-fast \
    --test m1_physics_golden --test m1_mcp_screenshot
```
It must pass on CI's macOS GPU job (`macos-26` arm64, or the `self-hosted-macos-gpu` fallback from Phase 1.2), and `m1_physics_golden`'s physics-hash assertion must also pass on Linux x86_64 (run with `CONCORDIA_REQUIRE_GPU=0`, which skips only the image comparisons there unless 1.10 landed).

**Gate scene:** `examples/m1-physics-stack/scenes/stack.scene.ron`
- static ground: a 30×0.5×30 m box collider with a `MeshRenderer` (default grey material);
- a 4-3-2-1 pyramid of ten 1 m dynamic boxes plus three boxes dropped from 4–6 m with slight rotations, so they topple and collide; three materials (`red_painted.material.ron` rough dielectric, `brushed_steel.material.ron` metallic, `emissive_glow.material.ron` with emission);
- a static glTF prop (`BoomBox`, CC0, from the fetched sample assets) to put an imported asset in the shot;
- `DirectionalLight` with 3 stabilized cascades, `Environment` with the fixture HDRI;
- 16 colored `PointLight`s in a ring (exercises clustered lighting);
- camera `gate_cam`: orthographic isometric (yaw 45°, pitch −35.264°, height 18), MSAA 4×, AgX tonemap, fixed EV100.

**`tests/gates/m1_physics_golden.rs`**
- `m1_gate_physics_scene_golden`
  1. Start an in-process `EngineHost` in headless GPU mode with project `examples/m1-physics-stack`.
  2. Through the Engine API (in-process transport, same request types as JSON-RPC): `scene.open`, then `sim.step { ticks: 180, actor: "agent:gate" }` (3 s at 60 Hz).
  3. Assert physics really ran: the three dropped boxes have y below their start height; every dynamic body's linear speed < 0.05 m/s (settled); `PhysicsWorld::state_hash()` equals the insta snapshot `m1_gate_state_hash`.
  4. `capture.screenshot { camera: "gate_cam", width: 1280, height: 720 }`.
  5. Assert FLIP vs `tests/golden/gates/m1_physics_stack.png`: mean ≤ 0.015, p99 ≤ 0.25.
  6. Assert the image is not degenerate (guards against blessing a broken frame): luminance in a fixed region of the known shadow cast by the pyramid is at least 30% lower than an adjacent sunlit ground region; at least one pixel near each of three point lights has the light's hue dominant.
  7. Re-run steps 1–4 in a fresh `EngineHost`; assert the PNG bytes are identical to the first run (render + physics determinism on one device).
- `m1_gate_state_hash_cross_platform` (runs on Linux and macOS, no GPU): steps 1–3 only, without the renderer.

**`tests/gates/m1_mcp_screenshot.rs`**
- `m1_gate_agent_captures_screenshot_over_mcp`
  1. Spawn the real `concordia-mcp` binary over stdio exactly as `.mcp.json` registers it (`concordia-mcp --project examples/m1-physics-stack --gpu`), with the environment **cleared** except `PATH`, `HOME`, `SLANG_DIR` and `CONCORDIA_*` (proves no LLM API key is needed or read).
  2. MCP `initialize` / `notifications/initialized`; `tools/list` contains `capture_screenshot`, `scene_open`, `sim_step`, `input_inject`; `capture_screenshot`'s `inputSchema` equals the reflect-generated schema (insta snapshot).
  3. `tools/call scene_open`, then `tools/call sim_step { ticks: 180, actor: "agent:gate" }`.
  4. `tools/call capture_screenshot { camera: "gate_cam", width: 1280, height: 720 }`.
  5. Assert the result has `isError: false` and a content item `{ "type": "image", "mimeType": "image/png", "data": <base64> }`; base64-decode → valid PNG 1280×720; FLIP vs the same golden `m1_physics_stack.png` within the same tolerance.
  6. `tools/call log_tail` shows the `sim.step` entry attributed to `agent:gate`.

**Agent demonstration (sign-off, recorded, not CI):** in this repo's Claude Code session (subscription, no API key) with `concordia-mcp` registered in `.mcp.json`, ask the agent to open the stack scene, step 180 ticks and call `capture_screenshot`; the agent describes the image it received (boxes, shadows, colored lights). Paste the transcript excerpt into the PR that flips M1's status to "Gate passed".

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| Hosted macOS runners expose no usable Metal device or an incomplete paravirtual one | Probe job first in Phase 1.2; fallback to a self-hosted Apple Silicon runner; lavapipe (1.10) gives Linux GPU coverage as a second net. ADR-0042 records the choice. |
| Golden images differ between paravirtual and physical Apple GPUs | FLIP tolerances; golden class variants (`@<class>`) allowed only with PR justification; avoid non-deterministic features (no TAA, fixed sample patterns, no atomics-ordered blending in goldens). |
| `shader-slang` crate immature or lags Slang releases | `ShaderCompiler` trait hides it; `slangc` subprocess + `-reflection-json` fallback. |
| Slang MSL output gaps for bindless/argument buffers | Small `bindless.slang` abstraction with explicit global arrays; conformance test `compute_reads_bindless_buffer_by_index` catches it in Phase 1.2/1.3. |
| `joltc` lags Jolt or misses an API | Own shim grows to cover it; pinned versions; bindings committed. |
| Jolt cross-platform determinism differs between arm64 and x86_64 | `CROSS_PLATFORM_DETERMINISTIC`, `-ffp-contract=off`; if still different, per-platform hash snapshots and an ADR (same-platform determinism is what M3 replay requires first). |
| Tauri webview + native viewport compositing fails (ADR-0004 risk) | Timeboxed spike with 4 approaches; streamed viewport (D) always exists. |
| Render graph over-engineering delays visible results | Phase 1.5 limits the graph to features the M1 passes need; async compute and multi-queue are modeled but not executed. |
| Scope: M1 is the largest milestone | Vulkan is explicitly a stretch; physics and editor run in parallel tracks; the gate needs neither Vulkan nor the native viewport (it works with approach D). |
| Third-party asset licenses | Fetched-not-vendored glTF samples with a license allowlist checked in CI; one CC0 HDRI committed with its license. |
| macOS main-thread requirements (winit, AppKit) conflict with engine threading | Documented invariant in `concordia-platform`: main thread = platform loop only; sim and render on their own threads. |

## Open questions
1. **Input injection and the command log:** M1 logs injections as attribution-only, non-undoable entries. Confirm this fits M0's command-log model, or whether M0 already defines a non-command "event" log.
2. **Simulation output vs commands:** physics writes transforms as simulation state outside the command log, while `sim.step` is a logged command. Confirm against M0's design of `sim.*`.
3. **Metal 4:** add a Metal 4 code path (MTL4 command allocators, argument tables) in M2/M4, or require macOS 26 and target only Metal 4? M1 targets the Metal 3 API.
4. **Texture compression:** BC vs ASTC on Apple Silicon (both supported on macOS) and KTX2/Basis for shipping; decide in M2 with the reference port's real texture volume.
5. **Point/spot light shadows** for spells: shadow atlas in M2, or wait for M4 virtual shadow maps? the reference port's light usage should decide (count shadowed `OmniLight3D`s in its scenes during M2 planning).
6. **Picking:** raycast against colliders/AABBs (M1) vs a GPU entity-id pass; the latter is needed once skinned characters arrive (M2).
7. **New crate `concordia-testkit`** (dev-only golden/GPU test helpers) and the gate test package name `concordia-tests`; both need adding to `plans/README.md` in the consistency pass.
8. **Golden storage size:** if `tests/golden/` grows large, move to Git LFS (decide before M2).

## ADRs to write
- **ADR-0040: RHI design**: bindless-first, explicit sync2-style barriers, placed resources for aliasing, capability flags for mesh shaders and 64-bit atomics; Metal via `objc2-metal`, Vulkan via `ash`.
- **ADR-0041: Jolt integration**: vendored Jolt + `joltc` + own shim, bindgen committed, determinism flags, single precision.
- **ADR-0042: GPU testing in CI**: macOS GPU runner choice (hosted paravirtual vs self-hosted), lavapipe for Linux, FLIP tolerances, golden classes, blessing rules.
- **ADR-0043: Editor viewport embedding**: the spike's results and decision; updates ADR-0004's status.
- **ADR-0044: Asset ids and DDC**: sidecar ids, sub-asset id stability, DDC key composition, postcard blobs.
- **ADR-0045: Slang toolchain distribution**: pinned prebuilt releases fetched by `just slang-fetch`, `shader-slang` crate vs `slangc` fallback.
(Numbers follow M0's ADR-0009; renumber in the consistency pass if M0 used more.)

## Effort estimate
| Phase | Estimate |
|---|---|
| 1.1 Platform & input | 1.5 weeks |
| 1.2 RHI + Metal + conformance + CI probe | 3–4 weeks |
| 1.3 Shaders (Slang) | 1.5 weeks |
| 1.4 Assets, DDC, glTF | 2–3 weeks |
| 1.5 Render graph, forward PBR, capture, golden harness | 2.5–3 weeks |
| 1.6 Clustered lights, CSM, IBL, tonemapping, glTF goldens | 3 weeks |
| 1.7 Physics (Jolt) | 2.5–3 weeks (parallel track) |
| 1.8 Editor viewport spike | 1 week (timeboxed) |
| 1.9 Minimal editor | 2.5–3 weeks (parallel after 1.5) |
| 1.10 Vulkan + lavapipe (stretch) | 3 weeks |
| Gate + stabilization | 1 week |
| **Total** | ~16–22 weeks on the critical path with physics and editor in parallel; +3 weeks if the Vulkan stretch is done in M1 |

## Proposed convention additions
- **Crates:** `concordia-testkit` (dev-dependency only: golden compare with FLIP, `gpu_test!` macro, bless support). `concordia-rhi-metal` and `concordia-rhi-vulkan` named as separate crates (already implied by the README row). The cross-crate test package under `tests/` is `concordia-tests` (decided in the consistency pass).
- **File extension:** `.input.ron` for input maps (actions + bindings).
- **Engine API methods:** `asset.list` (MCP `asset_list`), `physics.raycast` (MCP `physics_raycast`), and `events.subscribe` (JSON-RPC notifications for change feeds) if M0 does not already define a subscription mechanism.
- **Reflect attributes:** `#[reflect(mcp_content = "image")]` on result fields, which `concordia-mcp` turns into MCP `ImageContent` (others into a JSON `TextContent`); JSON Schema extension keywords `x-concordia-widget` (e.g. `"color"`, `"vec3"`, `"angle"`), `x-concordia-range`, `x-concordia-unit` used by the inspector and by agents.
- **Asset reference syntax:** `"asset:<uuid>"` and `"asset:<uuid>#<sub-path>"` for sub-assets; built-ins as `"builtin:<kind>/<name>"`. Sidecars gain a `sub_assets` map.
- **DDC layout:** `.concordia/ddc/<first 2 hex of key>/<64-hex blake3 key>`; shaders under the same store with a `shader` namespace in the key.
- **Golden variants and artifacts:** optional `tests/golden/<suite>/<name>@<golden-class>.png` (classes: `metal-apple`, `vulkan-lavapipe`); failure artifacts in `target/golden-failures/`; blessing via `CONCORDIA_BLESS=1` (`just bless`), never allowed when `CI=true`.
- **Test env vars:** `CONCORDIA_REQUIRE_GPU=1` (GPU tests fail rather than skip), `CONCORDIA_BACKEND=metal|vulkan`.
- **Toolchain:** Slang pinned in `tools/slang/slang.toml`, installed to `target/toolchain/slang/<ver>/` by `just slang-fetch`; `SLANG_DIR` exported by `just`.
- **Just recipes:** `slang-fetch`, `bless`, `gltf-assets`, `jolt-bindgen`, `editor-dev`, `editor-test`, `editor-build`, `editor-schemas`.
- **CI runner labels:** `macos-26` GPU job; fallback `self-hosted-macos-gpu`; `linux-lavapipe` (stretch).
