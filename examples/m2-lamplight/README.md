# Lamplight

M2's sample game (decision D2; `plans/M2-port-begins.md`, phase 2.10): an original night-watch
game in a small harbour town. You are the lamplighter. Fog wisps drift in from the harbour; light
the street lamps before they reach the square, refill your oil from the wagons on the ring road,
and keep the town's spirit up until the dawn bells.

**Status:** the night plays (2.10c.1) and the town lives (2.10c.2): the lamplighter, lamps,
wisps, townsfolk and oil wagons. The menus and HUD arrive in 2.10c.3, the gate in 2.10d.

## Running it
`just lamplight-build` builds both gameplay modules into `build/` (git-ignored): the rules as
WASM and the fog as a native library. The night starts when `lamplight.Night`'s `phase` is
`playing` (the title screen sets it in 2.10c.3); until then nothing moves.

## How it plays (2.10c.1)
- Walk with WASD or the arrows; press E (or F) beside a dark lamp to light it. The reach's `lit`
  event lights the nearest unlit lamp within 2 m, for 20 of the can's 100 oil; a lamp burns 60 s.
- Every 20 s (the first after 5 s) the fog sends six wisps from the harbour toward a ring
  4–6 m around the fountain. A lit lamp's light scorches each wisp inside it for 30 health a second
  (a wisp has 40); burn numbers rise as it burns, and it pops at none.
- A wisp that reaches the square costs one of Town Spirit's 10; the night is lost at 0 and won at
  dawn (180 s).

## The town's life (2.10c.2)
- 40 townsfolk stroll between their front doors and the market stalls (a crowd). A wisp within
  5 m startles them, and they run home and shelter for 8 s. Far from the lamplighter they update
  less often: every 2, 4 or 8 ticks past 25, 45 and 70 m.
- Four oil wagons drive the ring road one way, with a spur to the docks. It splits at x = −5 and
  merges at x = 5, two junctions. Wagons brake for anyone in the road. Stand in a wagon's way and
  it slows beside you and fills your can (then waits 5 s before filling again).

## Layout
- `concordia.toml`: the project (input, physics, navigation, assets, animation, audio, UI,
  rendering, replay and the two script modules).
- `scenes/town.scene.ron`: the town, built by `tools/build_town.py` through the Engine API.
- `templates/`: the wisp and the burn and spirit numbers.
- The scene also holds the lanes and junctions, the wagons (a cart under each) and the townsfolk
  (a walker with the body under it, as the lamplighter).
- `scripts/`: `lamplight-rules` (module `lamplight`, WASM), `lamplight-fog` (module `fog`,
  native) and `lamplight-types` (the engine types both mirror, and the night's broadcast).
- `assets/`: every binary file, each recorded in `assets/LICENSE.md` with its source, licence and SHA-256.
  - `town/`: CC0 town models by Kenney (houses, roofs, roads, the market, lamps, carts, dressing), sharing `town/Textures/colormap.png`.
  - `sounds/`: CC0 foley by Kenney (footsteps, bell, glass, wood, creaks, latch, cloth) and our synthesized sounds (`sizzle`, `pop`, `whoosh`, `rumble`, `click`, `chime`).
  - `music/night.ogg`: the night theme (a 4 s intro, then a 32 s loop from frame 192000; streamed).
  - `characters/`: the lamplighter (Idle, Walk, Light, Startle) and the townsfolk (Idle, Walk, Startle), with clip events in their sidecars: `step_left` and `step_right` on Walk, `reach` and `lit` on Light, `flinch` on Startle.
  - `props/`: the wisp and the lantern pole.
  - `textures/`: the cobble gap mask and the awning stripes.
- `materials/`: wet cobbles (a clear film over stone and puddles mixed by the cobble mask), lamp glass (haze and thin film), awning cloth (fuzz, striped), the wisp (emissive with coverage) and wagon paint (a clear coat).
- `anim/`: the two animation graphs (walk blended by `walk_amount`; the Light and Startle one-shots fired by `light_now` and `startle_now`).
- `audio/`: `lamplight.buses.ron` (Master → Music, SFX, Ambience, UI) and the cues in `audio/cues/`.
- `ui/lamplight.theme.ron`: panels, labels, buttons, text fields, the oil and spirit bars, and the burn and spirit numbers.
- `input/lamplight.input.ron`: `walk` (WASD or the arrows), `light` (E or F), `pause` (P or Escape) and the in-game UI's `ui_*` actions.
- `tools/`: the generators for our own files (see below) and the town builder.

## Regenerating our own files
Run from this folder; each script rewrites its files in `assets/`, and
`cargo nextest run -p concordia-tests --test lamplight_content` checks the result against the
SHA-256 pins in `assets/LICENSE.md` (update the pins only after reviewing the change).

```sh
python3 -I tools/make_textures.py assets
python3 -I tools/make_sounds.py assets      # needs oggenc (vorbis-tools)
blender --background --factory-startup --python-exit-code 1 --python tools/make_characters.py -- assets
blender --background --factory-startup --python-exit-code 1 --python tools/make_props.py -- assets
```

After regenerating a model or sound, re-import it so its sidecar stays current:
`concordia import --project . <file>`.

The town is rebuilt from the repository root with
`python3 -I examples/m2-lamplight/tools/build_town.py target/debug/concordia` (after
`just lamplight-build`); it writes a new scene with fresh entity ids.
