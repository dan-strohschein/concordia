---
name: concordia-nav-agents
description: "Makes characters walk with NavAgent from gameplay code (Rust or a script): set the target, react to NavArrived and NavPathFailed, read NavAgentState, and check routes with nav_path."
when_to_use: "Use when gameplay must move a character somewhere on its own: chasing, patrolling, fleeing, walking to a clicked point, or when an agent walks the wrong way or never arrives."
user-invocable: false
---
# Navigation agents

A `NavAgent` walks its entity's `CharacterController` to `target` along the navmesh. Gameplay writes `NavAgent` (usually just `target`); the engine plans paths (`nav.plan`), steers (`nav.follow`, in `PrePhysics`) and **owns the entity's `CharacterMotion`** while it has a `NavAgent`. Remove the `NavAgent` to drive the character yourself. Navmeshes are the level designer's (`concordia-navigation`).

## Steps
1. Give the character a `NavAgent` sized like its controller (`radius`, `height`), with a `max_speed`. A mismatch, or a radius wider than the navmesh's `agent_radius`, is warned about once in `log_tail`; the controller still collides.
2. Set `target` from gameplay, in any phase: nav reads it at the next `PrePhysics`. `None` stops the agent. In a script, mirror the engine component field for field (the wire layout follows the fields, in this order) and declare `.writes::<NavAgent>()`:
   ```rust
   #[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
   #[reflect(name = "NavAgent", default)]
   pub struct NavAgent {
       pub target: Option<Vec3>,
       pub max_speed: f32,
       pub radius: f32,
       pub height: f32,
       pub path_desired_distance: f32,
       pub target_desired_distance: f32,
       pub path_max_distance: f32,
       pub volume: Option<EntityId>,
   }
   ```
3. React to the signals, delivered the tick after they're raised (declare `.receives::<NavArrived>()` and mirror it the same way):
   - `NavArrived { target, position, partial }`: it reached the target, or with `partial` the closest reachable point (an unreachable target isn't a failure).
   - `NavPathFailed { target, reason, detail }`: no path for this target (`NoNavmesh`, `StartOffNavmesh`, `TargetOffNavmesh`, `InvalidAgent`), raised once per target and retried only when the target moves.
4. Check it in the engine: `mcp__concordia__sim_step`, then `mcp__concordia__query_run {"with": ["NavAgent", "NavAgentState"], "select": ["NavAgentState"], "world": "play"}` shows each agent's `status` (`Idle`, `Waiting`, `Moving`, `Arrived`, `Failed`), `waypoints` and `next` corner. A route that looks wrong: `mcp__concordia__nav_path {"from": [0.0, 0.0, 0.0], "to": [12.0, 0.0, -3.0], "world": "play"}`.

## Rules
- It plans again when the target moves farther than `target_desired_distance` from what it planned for, or when the agent is pushed more than `path_max_distance` off its path; moving the target a little each tick (a chase) replans only when it has moved that far.
- At most `[nav] plans_per_tick` (default 32) agents plan a tick, longest-waiting first; the rest are `Waiting` and keep walking their old path.
- Arrival and corners are measured sideways, within half the agent's height vertically (the navmesh surface sits a little above the floor). The agent cuts each corner by at most `path_desired_distance`; keep it at or under the navmesh's `agent_radius` where corners are tight.
- Agents follow every tick whatever their simulation LOD, and signals reach receivers whatever theirs.

## Crowds
A `NavAgent` with a `CrowdAgent` too is a crowd agent: the crowd steers it with its neighbours (avoidance, optional separation, and collision pushes) instead of `nav.follow`. Gameplay drives it exactly like any agent: set `NavAgent.target`, hear the same `NavArrived` and `NavPathFailed`, and `ClickToMove` works on it. Crowd agents usually come from a template's `Crowd` trait (the level designer's `concordia-terra-templates`).
- **Two bodies:** with no `CharacterController` it is a **surface** agent (the crowd moves its `Transform` along the navmesh; cheap, fine for thousands). With a `CharacterController` the crowd writes its `CharacterMotion` and physics moves it. Every `CharacterController` (the player too) is solid to surface agents: they step aside for it and never push it.
- **Avoidance** is on by default (`CrowdAgent.avoidance`, DetourCrowd's sampled velocity obstacles): each tick the agent picks, among velocities up to its speed, the one that best keeps its desired velocity while not walking into its nearest neighbours (characters included) or the nearby navmesh walls within about 2.5 s. Set `"avoidance": null` for agents that should only be pushed (cheaper; collision still keeps them apart). The `CrowdAvoidance` weights trade keeping course (`weight_desired`, `weight_current`) against passing on a consistent side (`weight_side`) and seeing impacts early (`weight_toi`, `horizon`); more `divs`, `rings` or `depth` sample more finely at more cost.
- **Far agents** (with a `Lod` profile, far from every `LodViewer`) choose their corners, desired velocity and avoidance only on their due ticks and hold them in between, so they may run a little past a corner before turning. They still collide, stay on the navmesh, arrive and raise signals every tick.
- **Its state:** `CrowdState` (hashed: `velocity`, `desired_velocity`, `new_velocity` (avoidance's choice), `corners`, `neighbours`, `walls`, …); its polygon corridor is `NavAgentState.corridor`. Read them with `mcp__concordia__query_run {"with": ["CrowdAgent", "CrowdState"], "select": ["CrowdState", "NavAgentState"], "world": "play"}`.
- **See it:** the `crowd` debug layer draws each crowd agent's ring, velocity, desired velocity, corners, nearby walls and neighbour links (ask whoever has `capture_screenshot`).
- To change a crowd agent's tuning from a script, mirror both types field for field:
  ```rust
  #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
  #[reflect(name = "CrowdAgent", default)]
  pub struct CrowdAgent {
      pub max_acceleration: f32,
      pub collision_query_range: f32,
      pub path_optimization_range: f32,
      pub anticipate_turns: bool,
      pub optimize_visibility: bool,
      pub separation: bool,
      pub separation_weight: f32,
      pub avoidance: Option<CrowdAvoidance>,
  }

  #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
  #[reflect(name = "CrowdAvoidance", default)]
  pub struct CrowdAvoidance {
      pub velocity_bias: f32,
      pub weight_desired: f32,
      pub weight_current: f32,
      pub weight_side: f32,
      pub weight_toi: f32,
      pub horizon: f32,
      pub divs: u8,
      pub rings: u8,
      pub depth: u8,
  }
  ```
