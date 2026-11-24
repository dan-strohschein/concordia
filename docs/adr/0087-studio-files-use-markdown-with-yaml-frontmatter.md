# ADR-0087: Studio role, skill and task files use Markdown with YAML frontmatter
- Status: Accepted (M3 phase 3.3, 2026-11-19; written at M3's close, 2026-11-24)
- Date: 2026-11-24
- Builds on: the text-formats rule (RON is the source of truth), ADR-0073 (per-role scopes)

## Context
Concordia's text formats are RON: scenes, materials, cues, input maps, replays and playtests. Agent Studio's files are different in kind. A role is mostly a prompt (instructions in prose); a skill is a document; a task is a description with notes. People and agents edit them as Markdown, and Claude Code's own agents and skills are Markdown with YAML frontmatter, which `concordia studio sync` generates.

## Decision
1. **`studio/agents/<role>.md`, `studio/skills/<name>/SKILL.md` and `studio/tasks/<id>-<slug>.task.md`** are Markdown with YAML frontmatter. The frontmatter holds the structured fields (scopes, paths, status, acceptance), and the body is kept verbatim.
2. **The exception is narrow:** only these Studio files. `studio/team.toml` stays TOML, like `concordia.toml`. Every engine format stays RON.
3. **Parsing is strict:** `serde_norway` (MIT OR Apache-2.0, maintained; `serde_yaml` is not), unknown keys refused with a did-you-mean suggestion. Task files are rewritten canonically by the task tools.

## Consequences
- Studio files read naturally and map one-to-one onto Claude Code's files.
- One more format in the repository, with its own parser dependency, checked by `cargo deny`.

## Alternatives considered
- **RON frontmatter:** consistent with the engine, but unfamiliar to every editor and agent harness, and it would need translating for Claude Code.
- **Separate RON plus Markdown files per role:** doubles the files, and their fields and prose drift apart.
