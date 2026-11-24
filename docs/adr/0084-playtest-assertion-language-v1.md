# ADR-0084: Playtest assertion language v1
- Status: Accepted (M3 phase 3.2a, 2026-11-23; my recommendation, under the author's standing approval for M3)
- Date: 2026-11-23
- Builds on: ADR-0072 (replays), ADR-0063 (determinism: assertions read simulation state only)

## Context
Agents and tests need to state gameplay outcomes precisely and have the engine prove them, instead of eyeballing screenshots or writing one-off Rust per check: "the player reaches the door within 10 seconds", "no wisp ever reaches the square". The statement has to be:
- short enough for a task's acceptance criteria;
- checked before it runs, so a typo is a clear error, not a silent pass;
- evaluated the same way live, over a replay, or in a scripted scenario.

## Decision
1. **A small text grammar** (v1, in the M3 plan):

   ```
   temporal ":" pred
   ```

   - **Temporal operators:** `within`, `always [for]`, `never [for]`, `eventually`, `at` and `after …,`.
   - **Predicates:** `and` (binds tighter), `or`, `not`, parentheses, comparisons, and boolean expressions on their own.
   - **Expressions:** numbers with units (`m`, `s`, `ticks`, `deg`), `distance(sel, sel)`, `count(sel)`, `exists(sel)`, `event("Signal")`, and fields `sel.Component.path` / `any(sel).…` / `all(sel).…`.
   - **Selectors:** `tag("…")`, `name("…")`, `id("…")` and `with(Component)`.
2. **Its own crate, `concordia-assert`.** The language is pure: no world, no engine. Rust tests use it directly, and `concordia-replay` evaluates it.
3. **A hand-written recursive-descent parser** with 1-based column spans and the list of accepted tokens. It has no parser-generator dependency, so the messages stay ours. The printer is canonical, and parsing its output gives back the same tree (property-tested).
4. **Type checking against the reflection registry:**
   - Component names and field paths are resolved through each component's JSON Schema, so script-defined components work. Vectors index as `x`, `y`, `z`, `w`.
   - Unknown names get "did you mean" suggestions within edit distance 2.
   - **Units:** plain numbers (fields, counts) compare with anything. Length, Time and Angle compare only with themselves. Booleans stand alone.
   - **Quantifiers:** `tag` and `with` may match many entities, so a field read needs `any` or `all`.
   - **Durations:** seconds become whole ticks at the project rate, or the check fails.
5. **Tools take text only.** The syntax tree is not reflected. `assert_eval` (3.2b) takes the canonical text, so each role learns one form, and the tool's schema stays small (ADR-0073 budgets).
6. **`Tags`**, a built-in list of words, backs `tag(…)`.

## Alternatives
- **A structured syntax tree as the tool input (the original plan).** It doubles what an agent must learn, and the recursive schema is large for every role that has the tool.
- **Rust-only checks.** They need a build for every acceptance criterion and can't be written by roles without Bash.
- **A parser generator.** It's another dependency, and its messages are harder to make precise.
- **Full LTL.** It's more expressive, but it's harder to read in a task, and the bounded operators cover the plan's cases.

## Consequences
- Acceptance criteria can be pasted into tasks and checked by the engine (3.2b's `assert_eval`).
- A misspelled component or field fails before anything runs, and the message names the closest match.
- New expressions touch one crate: syntax tree, printer, parser, checker and builder (AGENTS.md).
- **Note (M3 phase 3.2b): evaluation.**
  - **Where it runs:** `concordia-replay` evaluates assertions (`assert.eval`, MCP `assert_eval`) over a replay, a `.playtest.ron` scenario, or the current play world ("live": the condition on the current tick, changing nothing).
  - **How it decides:** a monitor per assertion decides tick by tick from tick 0, with early exit. `always` and `never` over a whole run pass at its end, and `eventually` and an unreached `at` are undecided there.
  - **Values:**
    - `event("X")` means a signal of type X was raised on that tick (`World::pending_signal_count`).
    - Times compare in seconds (ticks divided by the rate).
    - `any` and `all` apply over every pair of values.
    - A selector read as one that matches 0 or several entities is an `error` result, not a failure.
  - **The CLI:** `concordia assert eval` and `concordia test playtests`.

