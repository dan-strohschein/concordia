---
name: concordia-traffic
description: "Lays out road traffic: lanes and junctions as entities (Lane, Junction), priorities at crossings, vehicle templates with the Traffic trait, problems through query_run, and the lanes debug layer in screenshots."
when_to_use: "Use when a level needs cars, carts or anything that drives along roads: drawing lanes, linking them, making crossings where one road gives way, or checking why a vehicle stopped."
user-invocable: false
---
# Traffic

Roads are ordinary scene entities: each **lane** is an entity with a `Lane` (a centre line driven from its first point to its last, a speed limit in m/s, a width, the lanes it leads to in `next`, and a `priority`), and each **junction** is an entity with a `Junction` listing the short **connector** lanes inside it. Vehicles follow lanes by themselves, keep a safe gap to whatever is ahead, stop at junctions until they may go, and brake for people standing in their lane. Traffic runs whenever `[nav]` is on.

## Steps
1. Draw each lane, one way only (a two-way road is two lanes): `mcp__concordia__entity_spawn {"components": {"Name": "Main east", "Lane": {"points": [[-100.0, 0.0, 0.0], [-6.0, 0.0, 0.0]], "speed_limit": 13.9, "width": 3.5}}}`. Points are in the entity's space (its `Transform` moves and turns them; scale doesn't apply) and must give a length.
2. Link lanes with the ids `entity_spawn` returned: `mcp__concordia__component_set {"entity": "<main east id>", "component": "Lane", "value": {"points": [[-100.0, 0.0, 0.0], [-6.0, 0.0, 0.0]], "speed_limit": 13.9, "width": 3.5, "next": ["018bcfe5-6800-7000-a424-000000000002"], "priority": 0}}`. A lane with no `next` is a dead end: vehicles stop at its end.
3. Make each crossing a junction: draw a connector lane through it for every way across (straight on, each turn), with `next` set to the lane it leaves by, then list the connectors: `mcp__concordia__entity_spawn {"components": {"Name": "Crossing", "Junction": {"lanes": ["018bcfe5-6800-7000-a424-000000000002", "018bcfe5-6800-7000-a424-000000000005"]}}}`. Connectors that cross, or that lead into the same lane, are never driven at once. Give the main road's connectors a higher `priority` (for example 1, side roads 0): among vehicles asking at the same time, higher priority goes first, then whoever waited longest.
4. Add vehicles with a template (see `concordia-terra-templates`) whose traits include `Traffic`, shown below; spawn them near a lane and each is placed on the nearest one. Give the template a mesh, and a `RigidBody` with `kind: Kinematic` plus a `Collider` if physics characters (the player) should bump into the car. Without a `destination` a vehicle wanders: at each lane end it takes one of `next` (a fixed choice per vehicle and lane).
5. Check for problems after play starts: `mcp__concordia__query_run {"with": ["TrafficProblems"], "select": ["TrafficProblems"], "world": "play"}` lists each bad lane or junction with what is wrong (a dangling `next`, a lane with one point or no length, a connector in two junctions). Bad lanes are left out; their vehicles stop.
6. Look at it: `mcp__concordia__capture_screenshot {"view": {"eye": [0.0, 60.0, 40.0], "target": [0.0, 0.0, 0.0], "projection": {"Perspective": {"fov_y": 0.9, "near": 0.1, "far": 300.0}}}, "debug": ["lanes"]}` (the `lanes` debug layer) draws every lane with direction arrows, connectors green (free), yellow (someone is let in) or orange (driven now), each vehicle's footprint, waiting vehicles' lines to their stop line, and problems as red crosses.

A vehicle template's traits:
```ron
traits: {
    "Traffic": (vehicle: (max_speed: 12.0, length: 4.5)),
},
```

## Rules
- Lanes are read when play starts; edit them in the edit world and play again.
- Vehicles brake for anyone standing in their lane (crowd members and characters), but people don't yet step aside for cars.
- A lane's `speed_limit` caps each vehicle's own `max_speed`.
