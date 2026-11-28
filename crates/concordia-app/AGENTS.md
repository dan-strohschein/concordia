# concordia-app: agent guide

## Purpose
The composition root: loads a project's `concordia.toml` and builds the engine modules it uses, so the CLI, MCP server and editor never repeat the list (ADR-0046). It also decides whether a process attaches to the project's live engine or hosts one (ADR-0082).

## Public entry points
- `Project::load(root)`, `Project::engine_config()` (modules plus `[sim]` rate and seed and `[journal]` sync: every process that builds an engine from a project starts here, 2.8), `Project::modules()`, `Project::input_map()`
- `Manifest` and its sections (`[project]`, `[scene]`, `[sim]`, `[journal]`, `[window]`, `[input]`, `[physics]`, `[assets]`, `[scripts]` with `[[scripts.module]]` `kind`/`path`/`source`; the scripts module is added last, 2.6d; `[replay]`, no keys yet, adds the replay module before scripts, 2.8; `[anim]`, no keys yet, adds the animation module after assets, 2.4.1; `[nav]` adds the navigation module after physics, 2.5a, and the crowd module (`concordia-terra-crowd`) after it, 2.5c.1; `plans_per_tick`, at least 1, default 32, 2.5b.1; `[audio]` (`voices` 1–256, `buses` a project-relative `.buses.ron`, `backend` `auto` / `offline` / `none`) adds the audio module after animation, 2.7a.2)
- `SharedEngine::attach_or_host[_on]`, `SharedEngine::attach_live`, `HostedEngine`: one engine per project (ADR-0082)
- `model_importers()`: the glTF and FBX importers (2.3d); `asset_importers()`: those plus the audio (`sound`, `cue`) and UI (`font`, `theme`) importers, the one list both `[assets]` and `concordia import` register (2.10b)
- `Player` (`new(draws)`, `event(&WindowEvent, &mut ApiCall)`): windowed play as Engine API calls. It sends `sim.play` and `viewport.present` (the game camera) on open, `input.inject` for input (cursor moves plus their `PointerRay` from `viewport.pointer_ray` over the logical window size), releases held keys and buttons on focus loss, and sends `sim.stop` on close (2.5b.2b, ADR-0078). Tested headless with synthetic events (`tests/player.rs`).
- `AppError`

## Invariants
- Unknown manifest keys are errors; a missing `concordia.toml` means all defaults.
- Paths in the manifest are project-relative and may not escape the project.
- One engine per project: attach-or-host runs under `.concordia/engine.lock`; a stale `headless.json` (dead pid, refused token) is replaced; a live engine of another version is never attached; a host removes its file on drop only if the file still names it.
- A module is included only if the manifest configures it (input needs `[input] map`; physics, assets and render need their sections), so M0 projects keep the M0 tool set.
- `[render] closure_budget` (1..=8) is checked when the project loads and can only lower the platform's closure budget (ADR-0068); `Project::closure_budget` passes it to `RenderModule::with_closure_budget`.
- `[render] backend = "auto" | "metal" | "vulkan"` (M4 4.1v(b), ADR-0088): the render and pathtrace modules' device, through `concordia-rhi-select`; `CONCORDIA_BACKEND` beats it, and `auto` is Metal on macOS, Vulkan elsewhere (`Project::backend_choice`, `render_backend`, `render_backend_with`; a bad env value is `AppError::Backend`).
- `[render] perf_mode = true` (default false) times every frame's passes through `RenderModule::with_perf_mode` (M4 4.1a, ADR-0089); `concordia perf run` sets it in memory.
- `[render] meshlets` (default true; M4 4.3a) and `[render] culling` (default true; M4 4.4) reach the renderer through `RenderModule::with_meshlets` / `with_culling`: the meshlet path, and its GPU culling (`false` draws every candidate meshlet).

## How to test
`cargo nextest run -p concordia-app`

## Common tasks
- **Start an engine in a new process kind:** use `SharedEngine::attach_or_host` with a `HostKind`, never `Engine::new` plus `ws::serve` directly.
- **Add a module:** read its settings from a new manifest section, append it in `Project::modules`, and add a test in `tests/project.rs`.

## Don'ts
- Don't build module lists anywhere else.

## References
- TOML 1.0 specification.

## Status
M1 phase 1.1; M3 phase 3.5b (shared engine); M2 phase 2.6d (`[scripts]`); M2 phase 2.8 (`[replay]`, `engine_config`); M2 phase 2.4.1 (`[anim]`); M2 phase 2.5a (`[nav]`); M2 phase 2.5c.1 (`[nav]` adds the crowd); M2 phase 2.5b.1 (`[nav] plans_per_tick`; `tests/nav_guest.rs`, a WASM guest that sets a `NavAgent` target and hears `AnimEvent` and `NavArrived`); M2 phase 2.7a.2 (`[audio]`; `Playback` and `engine_config_for` / `modules_for`: `concordia run` asks for `Windowed`, so `auto` opens the default device there and mixes offline everywhere else; `audio_settings` reads and validates the bus layout; every project registers the `sound` and `cue` importers; `tests/audio.rs`: clip-event cues on the fixture character, a WASM guest raising `PlayCue`, bus files and the backend choice). M2 phase 2.7b.1 (`[ui]` adds `concordia_ui::UiModule`; every project registers the `font` and `theme` importers; the player passes the window's scale factor to `viewport.present` so the UI draws at display density; `tests/ui.rs`: an imported font and theme reach `ui.inspect`). M2 phase 2.7b.2 (the player sends the window's logical size as `Viewport` input on open, resize and scale change, and asks `ui.hit_test` for the cursor on each pointer move, until a project without `[ui]` answers "no such method"; `tests/ui_click.rs`: a click on a HUD button never moves the hero, one elsewhere does; `tests/ui_replay.rs`: a session with clicks, a drag, the wheel, Tab and a resize replays exactly, at 1 and 8 threads). M2 phase 2.7b.3 (`Clipboard` and `OsClipboard` (arboard, opened lazily, warned about once when missing); `Player::with_clipboard`; a paste injects the OS clipboard's text as typed text, cut to 4096 bytes; after input the player reads `ui.clipboard` and writes each new generation to the OS; `tests/ui_replay.rs` gains typing). M2 phase 2.10b (`asset_importers()`, so `concordia import` imports sounds, cues, fonts and themes too). M3 phase 3.0c (with `[render]`, `[ui]`'s module gets the renderer's `world_ui_lens`, so world-space UI takes input; `tests/ui_world.rs`: a click at a world button's projected centre presses it, the queries see it, equal hashes at 1 and 8 threads; `tests/ui.rs`: a font only a template names loads for the simulation). M3 phase 3.1c (`[replay]`'s module gets an engine factory that builds another project's engine from its own manifest, for `replay.run`'s `ab` mode; `tests/replay_ab.rs`). M3 phase 3.7a (`[gen]` adds `concordia_gen::GenModule`; the manifest field is `generators`, as `gen` is reserved in Rust 2024). M8 phase 8.1c (`[fracture]` adds `concordia_fracture::FractureModule`). M8 phase 8.3 (`[destruction] breaks_per_tick` adds `concordia_destruction::DestructionModule`).

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
