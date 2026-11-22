---
name: concordia-navigation
description: "Sets up navmeshes (NavMeshVolume over static colliders, NavArea and NavAreaGrid marks), builds them with nav_build, checks reachability with islands and nav_path, and shows the navmesh debug layer in screenshots."
when_to_use: "Use when a level must be walkable: placing floors and obstacles agents path around, blocking or costing areas, or checking that every place a task names can be reached."
user-invocable: false
---
# Navigation

A navmesh is built inside a `NavMeshVolume` box from the **static colliders** in it (`RigidBody` with `kind: Static` and a `Collider`, not a sensor): the same ground characters walk on. Dynamic and kinematic bodies don't carve it. Marks change the walkable ground: `NavArea` boxes block it or make it costly, and a `NavAreaGrid` marks a cell grid.

## Steps
1. Make the volume cover the walkable part of the level: `mcp__concordia__entity_spawn {"components": {"Name": "Nav"}}`, then `mcp__concordia__component_set {"entity": "<id>", "component": "NavMeshVolume", "value": {"half_extents": [40.0, 5.0, 40.0], "agent_radius": 0.3, "agent_height": 1.8, "agent_max_climb": 0.3, "agent_max_slope_deg": 45.0, "cell_size": 0.25, "cell_height": 0.2, "tile_size": 64, "edge_max_error": 1.3}}`. The box is centred on the entity's `Transform`. Match the agent numbers to the characters' `CharacterController`.
2. Build and read the report: `mcp__concordia__nav_build {}` gives each volume's `tiles`, `polygons`, `islands` and bounds. **More islands than the design has separate areas means a gap**: a step too tall (over `agent_max_climb`), a slope too steep, or a doorway narrower than twice `agent_radius`.
3. Block or cost areas: `mcp__concordia__component_set {"entity": "<id>", "component": "NavArea", "value": {"half_extents": [2.0, 2.0, 1.0], "kind": "Blocked"}}`, or `"kind": {"Cost": {"cost": 5.0}}` for ground agents avoid unless it saves five times the distance. Cost must be at least 1.
4. For grid-shaped levels, mark cells: `mcp__concordia__component_set {"entity": "<id>", "component": "NavAreaGrid", "value": {"cell_size": 1.0, "rows": ["..##..", "..##..", "oooooo"]}}`. Cell (0, 0)'s corner is the entity's position; rows run along +z and characters along +x. `.` keeps the ground as it is, `#` blocks it, `o` makes it walkable again (over a `NavArea` block). The grid applies after `NavArea`s.
5. Check a route: `mcp__concordia__nav_path {"from": [0.0, 0.0, 0.0], "to": [20.0, 0.0, 15.0]}` returns `waypoints`, `length` and `partial` (true: the goal can't be reached, and the path ends at the closest reachable point).
6. Look at it: `mcp__concordia__capture_screenshot {"view": {"eye": [0.0, 40.0, 25.0], "target": [0.0, 0.0, 0.0], "projection": {"Perspective": {"fov_y": 0.9, "near": 0.1, "far": 200.0}}}, "debug": ["navmesh"]}` draws every polygon over the scene, colored by island; walls and blocked areas show as gaps.

## Rules
- The navmesh follows edits: `nav_path` and `nav_build` rebuild when the colliders, marks or volume change, and reuse the cached navmesh otherwise.
- The walkable surface sits up to one `cell_height` above the floor: compare positions sideways (x and z), not by height.
- Walls are kept `agent_radius` away; a corner diagonal to a blocked cell is cut by about one cell, not a full radius.
- In the play world (`"world": "play"`), queries use the navmeshes built when play started.
- Characters walk these navmeshes with `NavAgent` (gameplay code; see `concordia-nav-agents`). Add `"nav_paths"` to a screenshot's `debug` to draw each walking agent's remaining path.
