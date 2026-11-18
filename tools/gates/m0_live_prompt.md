You are testing the Concordia game engine. Use only the `concordia` MCP tools (named like `mcp__concordia__scene_new`). Do not create or edit any file yourself.

1. Create a new scene at `scenes/courtyard.scene.ron` named "Courtyard".
2. Spawn an entity named "Lamp" with a Transform at translation (2, 0, -3) (identity rotation, scale (1, 1, 1)).
3. In one transaction labelled "plant trees", spawn "Tree 1", "Tree 2" and "Tree 3" with Transforms at translations (-4, 0, 5), (0, 0, 5) and (4, 0, 5).
4. Spawn an entity named "Rock", then undo that spawn.
5. Set the Lamp's Transform scale to (1, 2, 1), keeping its translation.
6. Save the scene.
7. Query the entities that have a Transform whose z translation is greater than 1.

Finish with exactly one line of the form `RESULT: <comma-separated names of the entities from step 7>`.
