# The fixture character

An original test character (M2 phase 2.4.5): our own content, made by a script, never downloaded. It stands in for real characters in the done criteria of Phase 2.4 and in Agent Studio's animation skill.

| File | What it holds |
|---|---|
| `character.fbx` | A 1.85 m mannequin of boxes, one per bone (plus a nose on the head), each weighted fully to its bone; 17 bones (`pelvis`, `spine`, `chest`, `neck`, `head`, `upper_arm`/`forearm`/`hand` `.L`/`.R`, `thigh`/`shin`/`foot` `.L`/`.R`); four materials (Cloth, Shirt, Skin, Boot), so four mesh parts; five actions at 24 fps: `Character|Idle` (24 frames, looping), `Walk` (24, looping), `Attack` (15), `Hit` (9) and `Death` (27). It faces +Z once imported. |
| `character.fbx.import.ron` | Its sidecar: a fixed asset id (the graph names its clips by it) and the clip events, all on odd frames so each lands half-way between 60 Hz ticks: Idle `breathe` (frame 11), Walk `step_left` (5) and `step_right` (17), Attack `swing` (7), Hit `flinch` (1), Death `fall` (13) and `land` (27, the end). |
| `character.animgraph.ron` | A five-state machine (`moves`): travel between idle and walk (0.1 s), to attack and to death; `Auto` on the trigger `hurt` into hit; `Auto` + `AtEnd` back to idle from attack and hit (0.1 s). Canonical text. |

## Regenerate
```
blender --background --factory-startup --python-exit-code 1 --python tools/fixtures/make_fbx.py -- tests/fixtures/character character
```
Made with Blender 5.2.1 (references: the Blender Python API documentation). The bytes differ between runs (Blender stamps the creation time), so commit a regenerated file only with a reason, then rerun `fixture_character_plays_its_states_through_the_graph` (its golden changes if the poses do). The sidecar and graph are hand-kept; changing a clip's length or an event's frame changes the expected ticks in that test.
