# Lamplight

M2's sample game (decision D2; `plans/M2-port-begins.md`, phase 2.10): an original night-watch
game in a small harbour town. You are the lamplighter. Fog wisps drift in from the harbour; light
the street lamps before they reach the square, refill your oil from the wagons on the ring road,
and keep the town's spirit up until the dawn bells.

**Status:** content (2.10b). The scene, templates, scripts and UI arrive in 2.10c, the gate in 2.10d.

## Layout
- `concordia.toml`: the project (input, physics, assets, animation, audio and UI).
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
- `tools/`: the generators for our own files (see below).

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
