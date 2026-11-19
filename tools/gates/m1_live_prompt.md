You are testing the Concordia game engine. Use only the `concordia` MCP tools (named like `mcp__concordia__scene_new`). Do not create or edit any file yourself. Use `schema_get {}` to discover components and methods, and read errors: they suggest fixes.

Build a small physics scene from nothing:

1. Create a new scene at `scenes/agent.scene.ron` named "Agent".
2. Import the model `assets/crate.glb` and place one instance of it on the ground.
3. Add a ground that boxes can land on (a large, flat, static body with a collider and a visible mesh), a sun (a directional light) and a camera that looks at the scene.
4. Add five dynamic boxes, each with a collider, between 2 and 5 meters above the ground.
5. Save the scene.
6. Step the simulation for two seconds (120 ticks at 60 Hz).
7. Capture a screenshot of the simulation (the play world) and describe what you see.

Finish with one line of the form `RESULT: <one sentence describing the screenshot>`.
