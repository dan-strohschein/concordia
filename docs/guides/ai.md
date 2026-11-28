# Game AI (`concordia-ai`)

Runtime AI that needs no LLM: brains that run behavior trees over a blackboard, utility scoring, and perception. Everything is deterministic and replayable.

## Brains
- Add `[ai]` to `concordia.toml` (`agents_per_tick`, `sight_per_tick`, default 64 each).
- Give an entity `ai.Brain { tree: "ai/guard.bt.ron" }`. The `.bt.ron` file is an `ai.BehaviorTree`: declared blackboard `keys` and the nodes, depth first. Play start loads it and adds `ai.Blackboard` and `ai.BtState`.
- Nodes: Sequence, Selector, Parallel (`All`/`One`), UtilitySelector, RandomSelector (seeded); Condition (key compared with a value; `abort`: `None`, `SelfOnly`, `Lower`), Cooldown, Loop, TimeLimit, Inverter; tasks MoveTo (`NavAgent.target`, done on `NavArrived`), Wait, PlayAnim (an `AnimGraphPlayer` trigger), FaceTarget, SetBlackboard, CastSkill (`ai.Cast`), RunGraph (`ai.Run`), Plan (below).
- Every choice raises `ai.Decision`: assert on it with `event("ai.Decision")`, and inspect a brain with `ai.inspect` and `ai.trace`.

## Behavior graphs
- A brain can name a Behavior `.graph.ron` instead of a `.bt.ron` (`ai.Brain { tree: "ai/guard.graph.ron" }`); play start compiles it to the same tree.
- Nodes are the `behavior.*` set in `graph_list_nodes` (one per tree node above, plus `behavior.fail`). Composites order children by `child0`–`child3`; decorators have one `child`; parameters are constant inputs (`op`: eq, ne, lt, le, gt, ge, set, unset; `abort`: none, self, lower).
- The graph's variables are the blackboard keys (bool, i32/i64, f32, Vec3, EntityId, String). A wrong value or unknown key is a diagnostic naming the node and pin.
- Utility selectors and their curves are written in `.bt.ron` for now.

## Perception
- `ai.Perceiver` (sight range and cone, hearing range, memory length, eye height, factions noticed) and `ai.Faction`.
- Sight checks line of sight through physics. Raise `ai.Noise` on a noise's source and `ai.Damaged` on a victim. `ai.Memory` decays.
- The strongest hostile fills `target` and `target_position` when the tree declares them.

## LLM plans (opt-in)
- A `Plan { goal, deadline_ticks, options, key }` task asks an `LlmProvider` to pick one of `options`; the answer must match a schema generated from the reflected `ai.PlannerReply`. It lands in `key`, or the task fails (invalid, failed or late) and the tree's fallback runs.
- Requests never block the simulation: the provider works on its own threads and is polled.
- Every applied answer is recorded in the replay (`external`, replay format 2), so replays need no provider and give the same world.
- **The default build has no provider, no model runtime and no network client.** A shipped game brings a provider *the game developer* chose and bundled (`AiModule::with_provider`). The developer's Claude subscription is never available at runtime: agents build the game; they don't run inside it.
