# concordia-input

Input for Concordia: named actions bound to keys, mouse, scroll, the pointer (as a position, or as a world ray for clicks) and the viewport size in `.input.ron` files, sampled once per fixed tick so simulations and replays are deterministic, plus the `input.inject` Engine API method. Typed text (insertions and editing commands) is input too, so text fields replay exactly. Consumers such as the in-game UI can mark the pointer consumed for the tick, so world readers (click-to-move) skip what the UI took.

See `AGENTS.md` for invariants, tests and references.
