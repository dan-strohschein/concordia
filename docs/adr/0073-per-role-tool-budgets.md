# ADR-0073: Tool budgets are per agent role
- Status: Accepted (M2, 2026-11-20; the author's decision). Amended 2026-11-21: the budget is 50,000 tokens (see the amendment).
- Date: 2026-11-20
- Supersedes in part: ADR-0033 (the 12k-token `tools/list` budget) and ADR-0070 (its rise to 16k)
- Builds on: ADR-0080 (Studio scopes enforced by the hook and the server)

## Context
ADR-0033 capped the whole MCP `tools/list` at 12k tokens, and ADR-0070 raised the cap to 16k. The test only covered the core tools. A project with modules was already past it: about 20.5k tokens with `[input]`, `[physics]`, `[assets]`, `[scripts]` and `[replay]`, and about 38k for a Studio project (phase 2.8 left this as an open question).

A whole-list budget measures the wrong thing. No agent should ever get the whole list:
- In the Studio, each role is a subagent whose `tools:` line names its tools, so a role loads only those definitions. Like a human studio's lead, the director coordinates: it plans, delegates and reviews evidence, and it doesn't model, shade or code.
- Outside the Studio, a single Claude Code instance uses Concordia through MCP, the Engine API and the CLI. Claude Code loads MCP tool schemas on demand through tool search.

The default roles were also broader than their prompts. They used globs (`entity_*`, `sim_*`, `task_*`, `replay_*`), and the director had eight world-inspection tools.

## Decision
1. **No whole-list budget.** The server's `tools/list` has no token cap. A tool's size still matters, but only through the roles that use it.
2. **Access is configured by the admin in the roster, and only there.** A role's `mcp_tools` in `studio/agents/<role>.md` is the most it can ever call. The hook and the server both enforce it (ADR-0080), and agents can't change `studio/` (a human-only gate).
3. **Default roles name their tools explicitly.** They use no globs. Each tool a role has is one its prompt or preloaded skills tell it to use, and each tool those tell it to use is in its scope (tests in `concordia-studio/tests/roster.rs`). A skill's "For the director" section counts only for the director.
4. **The director coordinates.** It keeps the task tools it uses (create, list, get, update, comment), the team tools (`studio_*`) and `txn_history`. It reviews the evidence that specialists attach to tasks. It no longer has `scene_info`, `entity_get`, `component_get`, `query_run`, `schema_get`, `capture_screenshot`, `render_stats` or `log_tail`, and it no longer preloads `concordia-task-workflow`, because it never claims a task.
5. **A per-role budget of 22,000 tokens** (4 chars per token over each served tool definition: name, description, input and output schemas and annotations), for each enabled default role. `every_enabled_default_role_stays_under_its_tool_budget` in `concordia-mcp/tests/role_tool_budget.rs` measures it. The test runs `studio sync` against a project with every module and sums the real served definitions on each generated agent's `tools:` line. A failure names the role, its tool count and its size.

### Why 22k
Measured after this change:

| Role | Tools | Tokens |
|---|---|---|
| level-designer | 35 | ≈20.4k |
| qa-playtester | 22 | ≈17.6k |
| tech-artist | 21 | ≈14.0k |
| director | 13 | ≈12.5k |
| gameplay-programmer | 16 | ≈12.4k |
| writer | 11 | ≈9.6k |

The definitions average about 600 tokens, so the 2.5k first suggested would fit only about four tools. The task and team tools are the floor. Every specialist needs `task_get`, `task_claim`, `task_comment`, `task_update`, `studio_ask`, `studio_answer` and `studio_questions`, and they come to about 7.4k tokens, mostly the task view's output schema (about 740 tokens per task tool).

22k is the largest role plus about 8%, which is room for two or three average tools. Adding more needs a deliberate change: either a role split, or a raised budget written down here. It is about 58% of the 38k full list. Slimming the Studio tools' output schemas is the follow-up that would let the budget drop.

## Amendment (2026-11-21): 50k per role
The author raised the per-role budget from 22,000 to 50,000 tokens. 22k had left the level designer under 1k of room (≈21.1k after 2.3b), so almost any new tool for it would have needed a role split. 50k is deliberately generous: shrinking the tool definitions (mostly the Studio tools' output schemas) is the follow-up, and the budget comes down after that. Decisions 1–4 are unchanged: no agent gets the whole list, and each role still names its tools explicitly.

## Alternatives
- **Keeping a whole-list budget.** Every new module would have to fight for room in a list that no agent loads in full.
- **A budget for each role in the roster.** This would be more precise, but an admin could raise their own limit. One constant in a test keeps the defaults honest. Custom rosters are the admin's choice.
- **Globs that match fewer tools.** A glob grows silently when a module adds a tool, so the role's size and abilities change with no roster edit.
- **Measuring input schemas only.** Claude Code may not show output schemas to the model, but the whole definition is what is served and cached. Measuring all of it is conservative and matches the earlier measure.

## Consequences
- `tool_list_token_budget`, `replay_tools_add_under_1700_tokens_to_a_full_project` and the 16k assertion in `binary_serves_script_reload_when_scripts_are_configured` are retired.
- The default roles shrink: the director goes from 25 tools (≈20.8k) to 13 (≈12.5k), and the level designer from 51 (≈28.3k) to 35 (≈20.4k). The level designer no longer gets `replay_*`, because nothing in its prompt or skills records replays (this reverses part of M2 2.8's D8).
- The tech artist gains `scene_open`, `scene_save` and `sim_stop`, and the gameplay programmer gains `scene_open` and `sim_stop`. Their prompts now open, save and stop what they change, which they couldn't fully do before.
- The level designer keeps `scene_info` and `entity_spawn`, and its prompt now uses them. M3S gate calls that used tools a role no longer has now use ones it has (`task_get`, `schema_get`, `component_patch`). The director's attempt to approve is now refused by its scope, because it has no `task_approve`.
- M2 2.4.5 enables the animator with 22 tools (≈16.7k tokens): its preview-scene, steering, `anim_inspect` and capture tools, and the task and team tools. Measured that day: level designer 35 (≈21.5k), tech artist 23 (≈20.0k), QA 22 (≈18.4k), director 13 (≈12.5k), gameplay programmer 16 (≈12.4k), writer 11 (≈9.6k). `capture_screenshot` gained `skeleton`, a few tokens for every role that has it.
- M2 2.4.5 fixes the tech artist's `scene_save`: it had the tool but not the path, so the server refused to save the scenes its prompt told it to save. Scenes are now shared (`scenes/**` in `[paths] shared`, the author's call: the level designer edits them but doesn't own them alone, and they aren't locked down yet); the tech artist may write scene files (`scenes/**/*.scene.ron`), not other files under `scenes/`.
- M2 2.7a.2 enables the sound designer (24 tools, ≈15.1k tokens) and the composer (20, ≈13.1k), each with `audio_*` tools (the composer without `audio_inspect`), the wiring tools and `scene_save`. No other role's tools changed; measured that day: level designer 37 (≈23.1k), QA 24 (≈21.5k), tech artist 23 (≈20.1k), animator 22 (≈16.8k), gameplay programmer 17 (≈12.9k), director 13 (≈12.5k), writer 11 (≈9.6k). The budget test's manifest gains `[audio]`. With nine enabled roles and Claude Code's eight agent colors, the color test now requires distinct exact colors and every named color used before any repeats (the composer is `#14b8a6`).
