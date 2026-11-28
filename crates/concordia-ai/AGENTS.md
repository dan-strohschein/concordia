# concordia-ai: agent guide

## Purpose
Runtime AI that needs no LLM (M4 4.12): a typed blackboard, event-driven behavior trees in a flat array, utility scoring; then (4.12b–e) the engine module, perception, an opt-in LLM seam, and Behavior/Utility graphs on this runtime.

## Public entry points
- `Blackboard` (`ai.Blackboard`: declared keys, values, version), `BbType`, `BbValue`, `BlackboardError`.
- `BehaviorTree` (`ai.BehaviorTree`: keys + nodes depth first, each with its subtree's `end`), `Spec` (nested, flattened by `BehaviorTree::build`), `NodeKind`, `TaskKind`, `Abort`, `CmpOp`, `ParallelPolicy`, `TreeError`.
- `Ticker { tree, state, bb, host, rng, dt, decisions }.tick()`, `BtState` (`ai.BtState`), `NodeMem`, `Status`, `Decision`, `TaskHost` (what the engine implements).
- Utility: `Curve`, `Input`, `Consideration`, `UtilityOption`, `Selection`, `score`, `choose`.
- Engine (4.12b): `AiModule` (`[ai] agents_per_tick`), components `ai.Brain { tree }`, `ai.Blackboard`, `ai.BtState`; signals `ai.Decision`, `ai.Cast`, `ai.Run`; `add_ai_processors`/`insert_ai_resources` (`ai.think`, PostPhysics), `Trees`, `AiStats`; methods `ai.inspect`, `ai.trace`.

## Invariants
- Composites keep memory: a running child resumes next tick. Conditions re-check per `abort`: `None` on entry only, `SelfOnly` while their child runs, `Lower` while lower-priority siblings under a selector run (they abort them).
- Time counts from the tick a node starts (`Wait`, `TimeLimit`, `Cooldown` use the tree clock `BtState::time`).
- Randomness only from the caller's `SimRng` (`RandomSelector`, `Selection::TopN`); everything else is a pure function of the inputs.
- Curves clamp to [0, 1]; a consideration at 0 vetoes its option; scores use Mark's IAUS compensation.

- `ai.think` changes the world only through commands (`NavAgent.target`, `AnimGraphPlayer` triggers, `Transform` facing) and signals, from a parallel loop with per-piece buffers: results never depend on the thread count. Each agent's randomness is `processor_seed(sim seed, tick, "ai.think")` forked by its id.
- Time slicing: `agents_per_tick` agents re-think per tick in rotation, plus every agent whose `MoveTo` heard `NavArrived`/`NavPathFailed` this tick; a skipped agent's tree clock catches up (`BtState::last_tick`).

- Perception (4.12c): `ai.sense` runs before `ai.think`. Sight: `sight_per_tick` perceivers look a tick in rotation (cone around +Z, range, line of sight by `PhysicsQueries::raycast` from eye height; no physics, no occlusion); every perceiver hears `ai.Noise` (linear falloff to the noise's radius, capped by `hearing_range`) and feels `ai.Damaged`. Hostility: `notices` factions, or every faction but its own. Memory decays linearly over `memory_seconds`; the strongest entry fills `target` / `target_position` where the tree declares them.

- LLM (4.12d): `LlmProvider` (submit/poll, never blocking), `MockLlmProvider`, `LlmService` (the world's provider and pending plans), `action_schema` (from the reflected `ai.PlannerReply`), `validate_reply`. A `Plan` task's answer is logged to `concordia_core::external::ExternalInputs` (failures too, as `!error: …`), in source order per tick; a replay plays the log back and never asks. No provider: `Plan` fails at once. `tests/deps.rs` keeps HTTP clients and model runtimes out of the default build.

## How to test
`cargo nextest run -p concordia-ai`

## References
Colledanchise & Ögren, *Behavior Trees in Robotics and AI* (CRC 2018) for composite semantics; Champandard's AiGameDev articles on event-driven BTs (2007–2012) and UE's public behavior-tree documentation for abort modes (no code); Mark & Dill, "Improving AI Decision Modeling Through Utility Theory" (GDC 2010); Mark, "Building a Better Centaur: AI at Massive Scale" (GDC 2015) for the compensation factor; Graham, "An Introduction to Utility Theory" (Game AI Pro, 2013) for curves.

## Status
M4 4.12a–d: the pure runtime, the engine module, perception and the LLM seam. Graphs on the runtime are 4.12e.

## Engineering rules
These apply to every change, by humans and agents alike.
1. **TDD on every phase.** Write the tests first, see them fail, implement, then refactor.
2. **SOLID.** Depend on abstractions at crate boundaries; extend through traits rather than by editing callers.
3. **No duplicate functionality.** Before writing any implementation, search for existing functionality (grep, `cargo doc`, crate `AGENTS.md` files). Extend or refactor what exists; never write a parallel version.
4. **Single responsibility at every level:** function, type, module and crate.
5. **Small functions** that do one thing.
6. **Validate parameters.** Every function checks its inputs:
   - Rust: encode invariants in types (newtypes, enums, `NonZero*`); validate at boundaries and return `Result` with a descriptive error; use `debug_assert!` for internal invariants. Public APIs never panic on bad input.
   - TypeScript: strict types, plus runtime validation at API and IPC boundaries.
7. **Short comments.** One-line doc comments on public items; no essays, and no comments that restate the code.
8. **Idiomatic code.**
   - Rust: follow the Rust API Guidelines and keep it clippy-clean; `thiserror` in libraries, `anyhow` only in binaries and tests.
   - Editor UI: idiomatic React and TypeScript, with function components, hooks, strict mode, ESLint and Prettier.
