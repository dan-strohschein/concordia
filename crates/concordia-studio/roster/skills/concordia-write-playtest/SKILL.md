---
name: concordia-write-playtest
description: "Writes playtest assertions and .playtest.ron suites that check a scene or mechanic over simulated time, and evaluates them with assert_eval so acceptance criteria are proven by the engine, not by eye."
when_to_use: "Use when a task's acceptance criteria describe behavior over time (falls, reaches, collides, stays within) or a fix needs a regression check."
user-invocable: false
---
# Writing a playtest

An assertion is one line of text the engine decides tick by tick: `<when>: <condition>`, e.g. `within 600 ticks: distance(tag("player"), tag("door")) < 2m`. A playtest (`tests/playtests/<name>.playtest.ron`) names a scene, its input (events by tick, or a replay's), a tick budget and its assertions.

## The language
- **When:** `within 600 ticks` or `within 10s` (passes on the first tick the condition holds, fails at the deadline); `always` or `always for 120 ticks` (fails on the first tick it doesn't); `never` or `never for 2s`; `eventually`; `at 300 ticks`; `after 60 ticks, <when>`. Seconds must be whole ticks at the project's rate.
- **Conditions:** `<`, `<=`, `>`, `>=`, `==`, `!=`; `and` (binds tighter), `or`, `not`, parentheses.
- **Values:** numbers with units `m`, `s`, `ticks`, `deg`; `distance(a, b)` (meters, closest pair); `count(sel)`; `exists(sel)`; `event("PlayerDied")` (raised on this tick); fields such as `name("Crate").Transform.translation.y` (`x`, `y`, `z`, `w` index vectors), `any(tag("enemy")).Health.current`, `all(with(Wisp)).Transform.translation.y`.
- **Selectors:** `tag("player")` (the `Tags` component), `name("Crate")`, `id("<uuid>")`, `with(Component)`. `tag` and `with` may match several entities, so read their fields through `any(…)` or `all(…)`.
- Unknown components and fields are refused before anything runs, naming the closest match; units must agree (a distance is never compared with seconds).

## Steps
1. Turn each acceptance criterion into one assertion with a deadline: "the crate rests on the floor" becomes `within 300 ticks: name("Crate").Transform.translation.y < 1`.
2. Select entities by `Tags` or `Name`, never by id (ids differ between scenes).
3. Try a condition on the open scene now: `mcp__concordia__assert_eval {"assertions": ["always: exists(tag(\"door\"))"]}` (live: the condition on the current tick only).
4. Write the suite under `tests/playtests/` (shared by the level designer and QA):

```ron
Playtest(
    scene: "scenes/arena.scene.ron",
    max_ticks: 600,
    inputs: [(tick: Some(0), event: Key(key: W, pressed: true))],
    assertions: [
        "within 600 ticks: distance(tag(\"player\"), tag(\"door\")) < 2m",
        "never: event(\"PlayerDied\")",
    ],
)
```

5. Run it with its scene open (`mcp__concordia__scene_open`): `mcp__concordia__assert_eval {"source": "scenario", "path": "tests/playtests/crate-rests.playtest.ron"}`. Or check assertions over a recorded replay's inputs: `mcp__concordia__assert_eval {"assertions": ["within 300 ticks: name(\"Crate\").Transform.translation.y < 1"], "source": "replay", "path": "tests/replays/crate-drop.replay.ron"}`.
   A replay that is evidence for an assertion runs its whole window: for `within 600 ticks: …`, step at least 600 ticks before stopping, even when the fix passes at tick 88. A shorter replay ends `undecided` on the old build, so it can't show the bug and proves nothing.
6. Each result has a `status` (`passed`, `failed`, `undecided` when the run ended first, `error` when a value couldn't be read), `decided_at` (the tick) and a `witness`: the comparison's left side by its text, and for a `within` that ran out, the `closest` value and `closest_at`.
7. A failing assertion is information: fix the scene or the code, never the threshold, unless the task's criterion itself was wrong (then say so in a comment).

## Output contract
In the task comment: the playtest path (or the replay and the assertion text) and each assertion's status, tick and witness.
