# ADR-0078: Pointer rays as input, the lens, physics queries for processors, and click-to-move
- Status: Accepted (M2 phase 2.5b.2a, 2026-11-22; author decisions N2 and N4)
- Date: 2026-11-22
- Builds on: ADR-0063 (determinism contract), ADR-0072 (replay v0), ADR-0076 (Terra LOD), ADR-0077 (navigation)

## Context
Click-to-move turns a click on the screen into a place on the navmesh. The screen, though, is outside the simulation:
- The camera's projection lives in `concordia-render`.
- The window size lives in the app.
- Nothing plays the game in a window yet: `concordia run` draws the edit world, and M2's gate drives play headless through `input.inject`.

Replays (ADR-0072) re-run the simulation from recorded input only, so anything a click depends on must be recorded or derived from hashed state. Processors can't reach physics raycasts: the runtime sits behind `physics.raycast` in the play state.

## Decision
1. **The simulation sees rays, never pixels (N2).**
   - Input gains a `Ray` kind (`ActionKind::Ray`, `ActionValue::Ray { origin, direction }`).
   - Two inputs feed it: an event, `InputEvent::PointerRay { origin, direction }`, and a binding, `Binding::PointerRay`, which fits only `Ray` actions.
   - Directions are stored normalized. Rays that are non-finite or zero are refused, by `input.inject` and in direct action values alike.
   - A ray action is inactive until its first ray and pressed from then on.
   - Replays record ray changes like any action, so `.replay.ron` stays version 0. A replay never depends on the window size, the camera's projection or the renderer.
2. **The lens is one function where cameras live.** `concordia_render::camera_ray(camera, cursor, size)` composes the camera's projection, at the image's aspect, with its view, and unprojects through `concordia_input::pointer_ray`. Rays start on the near plane. `scene_camera(world, requested)` picks a camera (the highest priority, ties to the lowest id), and `RenderWorld::camera` uses the same rule. The lens is served in two places:
   - **`viewport.pointer_ray`** (MCP `viewport_pointer_ray`) gives agents and tests a pixel's ray from a camera entity or a `View`, in the edit or play world. It needs no GPU.
   - **`concordia run`** will play the scene in its window and send cursor moves as rays from the camera it draws with (N4, 2.5b.2b).
3. **`PhysicsQueries`** is a play resource: a handle to the play session's physics runtime, not hashed. Processors raycast through it. A query sees bodies as of the last physics step, so it finds nothing on tick 0. That is deterministic, and it leaves the first step as it was.
4. **`ClickToMove` is gameplay the engine ships.**
   - **Fields:** a `Button` action (`click`), a `Ray` action (`pointer`), `hold` (retarget while held, or only when pressed), physics `layers` and a reach (`max_distance`).
   - **`nav.click`** runs in `PrePhysics`, after input sampling and before `nav.plan`. It takes the nearest physics hit along the ray, snaps it onto the agent's navmesh (`nearest_point`) and writes `NavAgent.target`, which is planned the same tick.
   - **A miss** (no hit, or no navmesh near) changes nothing. Bad settings or action names warn once per agent, and the component is ignored.
5. **Every project starts with an input map.** `concordia new` writes `input/default.input.ron` (`InputMap::starter`): `move` on the left mouse button, and `pointer_ray` on the pointer ray. Those are `ClickToMove`'s defaults. `concordia studio init` adds the map when a project has none, because QA now injects input.

## Alternatives
- **Pixels plus the window size as input** (the processor builds the ray). Replays would have to record the window, and nav would read the renderer's `Camera`.
- **Only callers compute rays** (no lens). Agents and tests would reimplement unprojection, and windowed play would have no shared function.
- **Sync bodies into physics at play start**, so tick-0 queries hit. That changes the first step's Jolt state and every existing play hash.
- **A signal for missed clicks.** There's no consumer until 2.7's UI feedback; 2.7 also routes clicks to the HUD first.

## Consequences
- New: `concordia-input`'s ray kind and `InputMap::starter`; `concordia-render`'s `camera_ray`, `scene_camera` and `viewport.pointer_ray`; `concordia-physics`'s `PhysicsQueries`; `concordia-nav`'s `ClickToMove` and `nav.click`. `concordia-nav` now depends on `concordia-input`, and `concordia-render` on `concordia-input` too.
- `input.inject`'s schema gains `PointerRay`, and the editor's generated types change. QA gains `input_inject` and `viewport_pointer_ray`.
- **2.5b.2b (N4):** `concordia run` plays the main scene. Window input is sampled into the play world, cursor moves are also sent as rays through `camera_ray`, and the play world is drawn from its highest-priority camera.
