---
name: concordia-sample-lamplight
description: "A guided tour of Lamplight, the engine's sample game: where its rules, fog, scene, content, menus and gate live, and how a change to it is proven. Read it to learn how a whole Concordia game fits together."
when_to_use: "Use when planning or making a change to the Lamplight sample, or when you want a worked example of a complete game (two script modules, a scene built through the Engine API, UI, audio and a deterministic gate) before building your own."
user-invocable: false
---
# A tour of Lamplight

Lamplight is the sample game: a lamplighter lights street lamps through a foggy night so wisps burn before they reach the town square. Its README is `examples/m2-lamplight/README.md`; the manifest `examples/m2-lamplight/concordia.toml` turns on every module it uses.

## Where things are
- **The rules** (`lamplight` module, WASM): `examples/m2-lamplight/scripts/lamplight-rules/`. The pure rules are in `examples/m2-lamplight/scripts/lamplight-rules/src/rules.rs` (oil, burning, spirit, dawn), the processors that apply them in `examples/m2-lamplight/scripts/lamplight-rules/src/world.rs`, and the menus and HUD in `examples/m2-lamplight/scripts/lamplight-rules/src/screens.rs`.
- **The fog** (`fog` module, a native library, so it hot-reloads mid-night): `examples/m2-lamplight/scripts/lamplight-fog/`. Waves are in `examples/m2-lamplight/scripts/lamplight-fog/src/waves.rs`, the townsfolk in `examples/m2-lamplight/scripts/lamplight-fog/src/people.rs` and `examples/m2-lamplight/scripts/lamplight-fog/src/townsfolk.rs`.
- **Shared types:** `examples/m2-lamplight/scripts/lamplight-types/`. These are the engine types both modules mirror, and the night's broadcast `NightTick`. A processor sees only its own query's columns, so facts about other entities travel as signals, delivered one tick later.
- **The town:** `examples/m2-lamplight/scenes/town.scene.ron` is generated: `examples/m2-lamplight/tools/build_town.py` builds it through the Engine API (lamps, houses, roads and lanes, wagons, townsfolk and the screens). Change the builder and rebuild; don't hand-edit layout in the scene.
- **Templates** spawned at run time: `examples/m2-lamplight/templates/` (the wisp, burn and spirit numbers, a record row).
- **Content:** `examples/m2-lamplight/materials/`, `examples/m2-lamplight/audio/` (cues and `examples/m2-lamplight/audio/lamplight.buses.ron`), `examples/m2-lamplight/anim/`, `examples/m2-lamplight/ui/lamplight.theme.ron` and `examples/m2-lamplight/input/lamplight.input.ron`. Each binary asset is recorded in `examples/m2-lamplight/assets/LICENSE.md`, with its source, licence and SHA-256.

## Running it
- `just lamplight-build` builds both modules into `build/`; `just play-sample` builds them and opens the game in a window.
- Headless, open `scenes/town.scene.ron`, then click through the title screen with `mcp__concordia__input_inject` and step with `mcp__concordia__sim_step`. Read the night with `mcp__concordia__query_run {"with": ["lamplight.Night"], "world": "play"}`.

## How a change is proven
- **Rule timings** are derived by hand in the headers of `tests/integration/lamplight_night.rs` (walking, lighting, burning, the square, dawn). The town's life is in `tests/integration/lamplight_town.rs`, and the menus in `tests/integration/lamplight_ui.rs`. Change a rule there first, then in `rules.rs`.
- **The gate**, `tests/gates/m2_lamplight.rs` (`just gate-tests M2`), plays over MCP only:
  - a scripted night that is won, replayed exactly at 1 and 8 threads;
  - an idle night that is lost;
  - a mid-night `script_reload`;
  - goldens of the title, mid-night and dawn screens;
  - a no-clipping audio digest;
  - one tool per role.
- If your change alters what the gate sees, say so in the task. Renders and digests are re-blessed only after a person looks at them.

## Rules of thumb
- Keep rules pure and in `rules.rs`; processors only move data in and out.
- New per-entity state is a reflected component in the module, mirrored in `lamplight-types` if the other module reads it.
- Every number a test checks is derived in that test's header, not copied from a run.
