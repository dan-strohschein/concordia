---
name: concordia-terra-templates
description: "Composes Terra entity templates (.template.ron) from traits and component values, and places crowds of them with Spawner entities or terra_spawn_batch, checking the result with terra_stats and terra_inspect."
when_to_use: "Use when a level needs many similar entities (crowds, props, pickups), or a reusable kind of entity that entity.create should offer."
user-invocable: false
---
# Templates and spawners

A template is a text file, `templates/<name>.template.ron`, named by its file stem. It composes **traits** (engine-defined bundles configured by parameters) and plain **component** values, which override what traits add.

## Steps
1. See what exists: `mcp__concordia__entity_templates {}` lists every template with its traits, components and any `problem`. `mcp__concordia__schema_get {}` lists components; `mcp__concordia__schema_get {"name": "Movement"}` shows a trait's parameters.
2. Write the template (canonical RON; keys sorted):
   ```
   Template(
       format: 1,
       description: "A drifting marker for crowd tests.",
       traits: {
           "Movement": (
               velocity: (0.0, 0.0, 2.0),
           ),
       },
       components: {
           "Name": "Mote",
       },
   )
   ```
   Two traits that add the same component are an error naming both: drop one, or set that component under `components`.
3. Check it compiles: `mcp__concordia__entity_templates {}` shows it without a `problem`.
4. Try it in the play world: `mcp__concordia__terra_spawn_batch {"template": "mote", "count": 200, "seed": 1, "distribution": {"Disc": {"radius": 10.0}}}`, `mcp__concordia__sim_step {"ticks": 60}`, then `mcp__concordia__terra_stats {"world": "play"}`. Clear with `mcp__concordia__terra_despawn_batch {"with": ["Name"]}` or `mcp__concordia__sim_stop {}`.
5. Put it in the level: one entity with a `Transform` and a `Spawner` (`mcp__concordia__component_set`, value `{"template": "mote", "count": 200, "seed": 1, "distribution": {"Grid": {"spacing": 2.0}}}`). The scene stores only the spawner; the same seed gives the same crowd every play.

## Simulation LOD (crowds far from the player)
Far entities can tick less often. Add the `Lod` trait to a crowd template, and mark what detail is measured from (the player, or a camera rig in the scene) with the `LodViewer` tag:
```ron
traits: {
    "Lod": (
        distances: (20.0, 60.0, 120.0),
        hysteresis: 3.0,
        rates: (2, 4, 8),
    ),
},
```
- Levels 1, 2 and 3 start at `distances` (meters from the nearest viewer, ascending) and update every 2, 4 and 8 ticks (`rates`, ascending); level 0 updates every tick. An entity regains detail only once it is `hysteresis` meters inside a boundary, which must be less than the first distance.
- Mark the viewer: `mcp__concordia__component_set {"entity": "<player id>", "component": "LodViewer", "value": null}`. The render camera never counts. With no viewer, everything stays at full detail.
- Check it in play: `mcp__concordia__terra_stats {"world": "play"}` reports `lod` (`viewers`, entities per level in `levels`, and how many are `due` this tick).
- Animation already follows LOD: far characters pose less often, and their clip events still fire once.

## Output contract
In the task comment: the template path, its traits, the spawner's entity name and parameters, and the play-world count from `terra_stats` (with its `lod` levels when the template has `Lod`).
