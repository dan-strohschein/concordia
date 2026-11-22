# ADR-0017: Advanced Godot features milestone (M13)
- Status: Proposed (author's direction, 2026-11-21)
- Date: 2026-11-21
- Builds on: ADR-0006 (licensing and clean room), ADR-0063 (determinism contract), ADR-0074 (animation runtime), ADR-0075 (play-world writes)

## Context
M2 2.4.3 built `.animgraph.ron` blend graphs and state machines with Godot 4's `AnimationTree` as the compatibility target, and left out the Godot features nothing in M2 needed:
- the one-shot's abort and fade-out requests, autorestart and additive mix;
- the transition node's auto-advance, transition to self and break loop at end;
- advance expressions and break loop at end on state machine transitions;
- `Curve` resources for fades.

M5 imports Godot projects' animation trees onto the native graph. Without these features, those trees import with "unmapped" reports.

## Decision
1. **A new milestone, M13 "Advanced Godot features"** (`plans/M13-advanced-godot-features.md`), is added to `docs/design.md`'s milestone list.
2. **It adds exactly the features 2.4.3 left out,** with Godot 4's documented behavior, as `.animgraph.ron` version 2. Version 1 files keep their behavior.
3. **Godot is the oracle:** its MIT documentation and source may be read for semantics, and a headless Godot 4 project may produce committed oracle values. Our code is our own.
4. **Determinism holds:** everything runs at the fixed step, and the autorestart's random delay is derived from hashed state, so it replays and doesn't depend on thread count.
5. **Advance expressions use a small fixed grammar** over graph parameters (ADR-0280), not a scripting language.
6. **ADR block:** M13 draws ADR numbers from 0280–0299.

## Consequences
- `concordia-assets` (the format, validation and importer version 2) and `concordia-anim` (evaluation) change; no new crate. An optional `tools/godot-oracle` project needs Godot only to regenerate fixtures, never in CI.
- M5's AnimationTree import can map these features instead of reporting them. The plan recommends running M13 before that M5 phase.
- Other Godot nodes (blend spaces, `Add2`, `TimeSeek` and others) stay out unless phase 13.0 brings them in.

## Alternatives considered
- **Fold the features into M5.** That keeps one Godot milestone, but M5 is large and these are engine features any project can use, not import mapping.
- **Leave them to the backlog.** Imported trees would keep losing behavior silently or with reports, and the reference port's characters may use them.
