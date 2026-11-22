---
name: concordia-animation-graphs
description: "Builds animation graphs (.animgraph.ron: blend trees and state machines with Godot AnimationTree semantics) from imported clips, authors clip events in model sidecars, and checks states, transitions and events in a preview scene with sim_step, play-world steering, anim_inspect and skeleton screenshots. Also covers bone attachments and property tweens."
when_to_use: "Use when a task asks for a character's animation states or transitions, footstep or hit events on clips, an item held on a bone, a tweened property, or a check that a graph plays as gameplay expects."
user-invocable: false
---
# Animation graphs, previews and events

A model with a skin gives its mesh entity `Skinned`; its clips are sub-assets `asset:<id>#clip/<name>` (FBX names them `<armature>|<action>`, e.g. `Character|Walk`). A clip plays on any skeleton with the same bone names. A character plays a graph through `AnimGraphPlayer` on its `Skinned` entity. **Graphs, clips and events load when play starts**: after editing a graph or a sidecar, re-import it and `mcp__concordia__sim_stop {}` before stepping again.

Blender is not part of this yet: authoring new clips in Blender waits for the Blender bridge (M3 phase 3.7). Work with the clips the models already have.

## 1. Find the clips
`mcp__concordia__asset_list {}` lists every imported file with its `sub_assets` (`clip/Idle`, `clip/Walk`, ...). After you change a sidecar, `mcp__concordia__asset_import {"path": "assets/models/guard.fbx"}` re-imports it and returns the same id.

## 2. Write the graph
A graph is `parameters`, named `nodes` and a `root`. `mcp__concordia__schema_get {"name": "AnimGraph"}` lists every field. Nodes:
- `Clip(clip: "asset:<id>#clip/<name>", looping: true)`; `looping: false` holds the last pose.
- `Blend2(a, b, amount, filter)`: `amount` is a float parameter (0..1); a non-empty `filter` lists the bones blended, the rest keep `a`.
- `BlendN(inputs, weights)`: one float parameter per input, normalized by their sum.
- `OneShot(base, shot, request, fade_in, fade_out, curve, filter)`: the trigger `request` plays `shot` once over `base`; the fade-out ends at the shot's end.
- `TimeScale(input, scale)`: a float parameter scales the rate (negative plays backward).
- `Transition(inputs: [(node, reset)], select, xfade, curve)`: an int parameter picks the input.
- `StateMachine(states: {name: node}, start, transitions)`. Each transition has `from`, `to`, `switch` (`Immediate`, `Sync`: keep the position, or `AtEnd`: wait until the current state's clip ends), `advance` (`Disabled`, `Enabled`: travel only, the default, or `Auto`: whenever its bool or trigger `condition` holds, or always without one), `xfade` (seconds), `curve` (`Linear`, `Smooth`), `reset` (default true: the destination starts at 0) and `priority` (lower wins).

Parameters: `Float(default)`, `Int(default)`, `Bool(default)`, `Trigger`. Nodes name each other; each node has one parent, and errors name the node. A character with five states, travel for locomotion, attacks and death, a trigger for hits, and automatic returns when a one-off clip ends:

```ron
(
    parameters: {"hurt": Trigger},
    nodes: {
        "idle": Clip(clip: "asset:0190a2b4-5c6d-7e8f-9a0b-1c2d3e4f5a6b#clip/Character|Idle"),
        "walk": Clip(clip: "asset:0190a2b4-5c6d-7e8f-9a0b-1c2d3e4f5a6b#clip/Character|Walk"),
        "attack": Clip(clip: "asset:0190a2b4-5c6d-7e8f-9a0b-1c2d3e4f5a6b#clip/Character|Attack", looping: false),
        "hit": Clip(clip: "asset:0190a2b4-5c6d-7e8f-9a0b-1c2d3e4f5a6b#clip/Character|Hit", looping: false),
        "death": Clip(clip: "asset:0190a2b4-5c6d-7e8f-9a0b-1c2d3e4f5a6b#clip/Character|Death", looping: false),
        "moves": StateMachine(
            states: {"idle": "idle", "walk": "walk", "attack": "attack", "hit": "hit", "death": "death"},
            start: "idle",
            transitions: [
                (from: "idle", to: "walk", xfade: 0.1),
                (from: "walk", to: "idle", xfade: 0.1),
                (from: "idle", to: "attack"),
                (from: "walk", to: "attack"),
                (from: "attack", to: "idle", switch: AtEnd, advance: Auto, xfade: 0.1),
                (from: "idle", to: "hit", advance: Auto, condition: Some("hurt")),
                (from: "walk", to: "hit", advance: Auto, condition: Some("hurt")),
                (from: "attack", to: "hit", advance: Auto, condition: Some("hurt")),
                (from: "hit", to: "idle", switch: AtEnd, advance: Auto, xfade: 0.1),
                (from: "idle", to: "death"),
                (from: "walk", to: "death"),
                (from: "attack", to: "death"),
            ],
        ),
    },
    root: "moves",
)
```

Save it as `assets/anim/<character>.animgraph.ron` and import it: `mcp__concordia__asset_import {"path": "assets/anim/guard.animgraph.ron"}` returns its id. A malformed graph is refused with the node to fix.

## 3. Preview it
Previews are your own scenes under `assets/anim/previews/`; game scenes belong to the level designer.
1. `mcp__concordia__scene_new {"path": "assets/anim/previews/guard.scene.ron", "name": "Guard preview"}` (it refuses while the open scene has unsaved edits; never discard someone else's work), or `mcp__concordia__scene_open {"path": "assets/anim/previews/guard.scene.ron"}`.
2. `mcp__concordia__prefab_instantiate {"asset": "asset:0190a2b4-5c6d-7e8f-9a0b-1c2d3e4f5a6b"}`, then `mcp__concordia__query_run {"with": ["Skinned"], "select": ["Name", "Parent"]}` for the entity that plays. A model with several materials has one `Skinned` entity per part: play the topmost one (its `Parent` is not another `Skinned` entity); the parts under it follow its pose.
3. `mcp__concordia__component_set {"entity": "0190a2b4-0000-7000-8000-00000000000a", "component": "AnimGraphPlayer", "value": {"graph": "asset:0190a2b4-0000-7000-8000-0000000000f1"}}`, then `mcp__concordia__scene_save {}`.

## 4. Play and steer
`mcp__concordia__sim_step {"ticks": 1}` starts play and runs one fixed tick (60 per second by default). Steer the running game with play-world patches; each is seen by the next tick, isn't journaled and never changes the scene:
- travel along the fewest enabled transitions (a jump when there is no path): `mcp__concordia__component_patch {"entity": "0190a2b4-0000-7000-8000-00000000000a", "component": "AnimGraphPlayer", "patch": {"travel": {"moves": "walk"}}, "world": "play"}`;
- fire a trigger: `mcp__concordia__component_patch {"entity": "0190a2b4-0000-7000-8000-00000000000a", "component": "AnimGraphPlayer", "patch": {"triggers": ["hurt"]}, "world": "play"}`;
- set parameters: `mcp__concordia__component_patch {"entity": "0190a2b4-0000-7000-8000-00000000000a", "component": "AnimGraphPlayer", "patch": {"floats": {"speed": 0.5}, "bools": {"armed": true}}, "world": "play"}`.

`"travel"` and `"triggers"` are one-tick requests, cleared after the tick that reads them. `mcp__concordia__sim_stop {}` ends play; the next `sim_step` starts again from the scene.

## 5. Check it
- `mcp__concordia__anim_inspect {"entity": "0190a2b4-0000-7000-8000-00000000000a"}` (play world only): every bone's `local`, `model` and `world` transform; the graph's nodes (a machine's `current` and `previous` state and `fade`, each clip's `time`, `frame` and events); every parameter's effective value; and `pending_events`, the events raised by the last tick.
- `mcp__concordia__capture_screenshot {"world": "play", "skeleton": true, "view_mode": "normals", "width": 480, "height": 480, "view": {"eye": [3.0, 1.5, 3.0], "target": [0.0, 0.9, 0.0], "projection": {"Perspective": {"fov_y": 0.75, "near": 0.1, "far": 50.0}}}}` draws the posed character with its bones on top. Previews have no lights, so `normals` shows the shape.
- Count ticks: a state entered on tick T covers [0, 1/60) s of its clip on T, so a time t falls on tick T + floor(t × 60). An `AtEnd` transition happens the tick after the clip reaches its end.
- `mcp__concordia__log_tail {"level": "warn"}`: graphs, clips or bones that failed to load (target `concordia::anim`). A character stuck in its rest pose usually has one.

## 6. Clip events
Events are authored in the model's import sidecar (`<model>.import.ron`, next to the model): the `settings` line, in seconds from the clip's start, by clip name. Then re-import the model; the import rewrites the sidecar in canonical form.

```ron
    settings: {"clip_events": {"Character|Walk": [{"time": 0.208333, "name": "step_left"}, {"time": 0.708333, "name": "step_right"}]}},
```

- Frames: time = frame / the clip's frame rate (FBX keeps its rate; `anim_inspect` shows `frame`). At 24 fps and 60 Hz, an odd frame lands half-way between ticks, so its tick is unambiguous.
- A step covers [from, to): an event fires on the tick that starts at it or passes it, once per lap of a looping clip; a clip held at its end also fires an event exactly at its end; time scale 0, speed 0 and jumps fire nothing.
- In graphs, every clip with a share of the pose above 0 fires, and the event's `weight` is that share (a cross-fade's fading-out state fires with the rest of the weight).
- Gameplay receives them as `AnimEvent` signals (`name`, `clip`, `node`, `time`, `weight`) on the character, the tick after they are raised. Agree event names with the gameplay programmer.
- Errors name the clip and event: unknown clips, times outside the clip, empty names.

## 7. Items on bones and tweens
- An item follows a bone through an entity whose `Parent` is the `Skinned` entity and which has `BoneAttachment`, Godot's `BoneAttachment3D`; the item is its child. To point an existing attachment at another bone: `mcp__concordia__component_set {"entity": "0190a2b4-0000-7000-8000-00000000000b", "component": "BoneAttachment", "value": {"bone": "hand.R"}}`. Check it with `anim_inspect`'s bone `world` transform. A missing bone leaves it in place, logged once.
- Tween any number field (Godot's `Tween` transitions and eases), here a lift on an entity: `mcp__concordia__component_set {"entity": "0190a2b4-0000-7000-8000-00000000000c", "component": "Tweens", "value": {"tweens": [{"component": "Transform", "field": "/translation", "to": [0.0, 2.0, 0.0], "duration": 0.5, "transition": "Quad", "ease": "Out"}]}}`. Tweens run at the fixed step, the last one on a field wins, a finished one lands exactly on `to`, and they die with their entity.

## Output contract
In the task comment: the graph and its asset id, each state's clip and each transition, the events with the tick each was raised in the preview, the captures, and who wires the graph into the game's scenes.
