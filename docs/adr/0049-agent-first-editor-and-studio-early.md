# ADR-0049: Agent-first end of M1, and Agent Studio right after M1
- Status: Accepted
- Date: 2026-11-19
- Amends: ADR-0048 (editor v1 joins M1)
- Builds on: ADR-0002 (subscription-first), ADR-0047 (thin panels)

## Context
ADR-0048 added editor phases to M1 so the author could build scenes by hand. On reflection, the author restated the engine's premise: game developers work through **their AI of choice, or a team of AIs**, so the editor's UI and UX matter far less than the agent surface. The editor's main job is letting a human watch what agents do and override them.

A check found the agent surface was incomplete. The repo's MCP launcher opened the M0 sandbox project, so Claude Code saw only the 25 core tools. It had no `capture_screenshot`, imports, physics or rendering, although those methods existed.

## Decision
1. **The rest of M1's editor work is agent-first** (`plans/M1-see-it.md`, phases 1.12–1.16, renumbered from 1.12–1.15):
   - **1.12 Agent surface:**
     - MCP coverage of every agent method;
     - an `agent`/`editor` audience on methods, so editor plumbing isn't offered to agents;
     - description quality tests;
     - agent-sized screenshots;
     - agent-guide workflows.
   - **1.13 Scene tree, 1.14 Component discovery, 1.16 Viewport views:** the Engine API methods are the deliverable; the UI is minimal (polish, drag-and-drop, custom widgets, command palette, native menu bar, snapping and freelook are dropped or moved to the backlog).
   - **1.15 Agent Activity:** a live, actor-filtered transaction list with "undo to here". It replaces the Undo History and menu polish.
   - **The M1 gate adds:**
     - an MCP coverage test;
     - a **live agent gate** (`just gate-live M1`): Claude Code on the subscription builds, saves, simulates and captures a scene from nothing through MCP only, with outcomes asserted.
2. **The repo's MCP launcher defaults to `examples/m1-physics-stack`** (done in this change).
3. **Agent Studio moves up.** M3's Studio phases 3.3–3.6 (config and task board, `studio sync`, attribution and enforcement, roster and skills) and 3.8 (the editor Studio panel) run **right after the M1 gate, before M2**.
   - 3.8 extends the 1.15 Agent Activity panel rather than building a second command log.
   - These phases get a **Studio checkpoint gate** that needs no replay: `studio sync` output matches snapshots, and a live run in which the Director assigns a scene-building task to a role subagent, the role's edits are attributed and confined to its scope, and a reviewing role verifies the result with `capture.screenshot` and queries.
   - Replay (3.1–3.2), content generation (3.7) and the full M3 gate stay after M2.

## Consequences
- **M1 finishes sooner,** and finishes on the capability that matters: an agent can build a playable scene alone.
- **The editor stays thin and modest,** in line with ADR-0047. A UI polish pass can be scheduled later if it's wanted.
- **The Studio phases need a refinement pass at their start, not after M2.** The open items are the `concordia new` project layout and how hooks reach the command log, and both are answerable from M0/M1.
- **M2 starts after the Studio checkpoint.**
