# ADR-0083: Studio panel and Claude Code launcher
- Status: Accepted (author's decisions, M3 phase 3.8, 2026-11-19)
- Date: 2026-11-19
- Builds on: ADR-0002 (subscription-first), ADR-0047 (thin panels), ADR-0080 (Studio journal), ADR-0081 (team questions), ADR-0082 (shared engine)

## Context
Phase 3.8 lets the human see and steer the Studio team from the editor: a task board, a command log with actor attribution, approvals, Studio status, and an optional launcher for the user's own `claude`. Three questions had no answer in the plan:
- **Doctor results.** `concordia studio doctor` existed only in the CLI, but the panel must show its checks.
- **The interactive terminal.** On macOS, how does the launcher open `claude --agent director` in a terminal?
- **Role colors.** The panel colors roles to match the generated agents' `color`, but Claude Code has only 8 agent colors. The roster has 12 roles, and teams can be any size.

## Decision
1. **`studio.doctor` is a Studio method** with editor audience, so it is not an agent tool. The checks live in `concordia_studio::doctor` (`DoctorEnv`, `machine_checks`, `sync_check`, `claude_path`, `auth_method`), shared by the method, `concordia studio doctor` and the launcher. They run in the engine host's environment, which is the one the Studio hooks use.
2. **The launcher opens a `.command` file.** It writes `.concordia/launcher/director.command` (`cd <project>` and `exec claude --agent director`, with no environment) and runs `open -a <app>` on it. Terminal is the default app, and the panel remembers the user's choice. No Automation permission is needed.
3. **A role's `color` is the single source,** and it may be one of Claude Code's 8 names or any `#rrggbb`. The editor shows the exact color. Sync writes the nearest Claude Code name (CIELAB distance) into the agent file. `studio.roster` returns both (`color` as hex, `agent_color` as the name), so the editor keeps no palette of its own.
4. **The launcher never handles credentials.** Children inherit the environment unchanged. Detection looks only at `PATH`, and at whether `ANTHROPIC_API_KEY` is set (never its value). The log (`.concordia/launcher/launcher.log`) replaces every environment value of six or more characters with `<redacted:NAME>`. A headless run is `claude -p <prompt> --agent director --output-format stream-json --verbose`, plus `--max-budget-usd` from `[budgets] headless_max_budget_usd` when set, and never `--bare`.
5. **The panel stays thin** (ADR-0047).
   - Dropping a card calls `task.update`, `task.approve` (done) or `task.reject` (rejected, with a reason) as `human:local`.
   - The board shows only what the engine returns, so a refused move snaps back with the engine's reason.
   - The command log merges `txn.history` and `studio.activity`, kept live by `edit`, `scene` and `studio` events.
6. **Budget limits only.** `studio.status` reports the team's limits, the role instances held and the last sync. Per-task usage is still counted in `concordia mcp`'s memory and is not shown yet.

## Alternatives
- **The editor shell runs the `concordia` CLI for doctor.** That would be desktop-only, need `concordia` on the editor's PATH, and add a second path to the same checks.
- **`osascript` driving Terminal.** It writes no file, but macOS asks once for Automation permission, and AppleScript quoting of paths is fragile.
- **An embedded terminal** (a PTY plus a terminal view in the editor). The author wants it eventually: a full, multi-color terminal. It's deferred because it adds new dependencies on both sides of the shell.
- **Sync assigns colors,** or **the panel uses its own palette.** The first lets a role's color change when another role is enabled; the second doesn't match Claude Code.

## Consequences
- Role files accept `color: "#rrggbb"`. Generated agent files still carry only Claude Code's names, so teams of any size keep valid agent files.
- The Studio panel works in the browser editor too, except the launcher, which needs the desktop shell.
- Follow-ups: the embedded terminal; budget usage in `studio.status`, with its counters moved out of `concordia mcp`'s memory.
