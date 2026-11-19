# concordia-studio

Agent Studio for Concordia: a configurable team of specialist agents (Director, Level Designer, QA, ...) that runs inside your own Claude Code, with no API key.

- `studio/team.toml`: the team, limits and approval gates.
- `studio/agents/<role>.md`: each role's scope (YAML frontmatter) and instructions (Markdown).
- `studio/tasks/T-0001-<slug>.task.md`: the task board.
- `.concordia/studio/journal.jsonl`: who did what (task changes, reviews, agents' file edits).

Add it to a project with `concordia studio init` (new projects have it already), then use `concordia task ...` or the `task_*` / `studio_*` MCP tools.
