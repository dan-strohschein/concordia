# ADR-0002: Subscription-first; the engine is a tool provider
- Status: Accepted
- Date: 2026-11-17

## Context
Users should be able to build games with the AI they already pay for (e.g. a Claude Code subscription) without managing LLM API keys.

## Decision
The engine never calls an LLM itself. It exposes the Engine API through an MCP server (`concordia-mcp`) and a CLI, and it compiles Agent Studio configs into Claude Code native files (`.claude/agents`, skills, hooks, `.mcp.json`, `CLAUDE.md`). The user's harness orchestrates. Direct-API or local-model runners are optional plugins.

## Consequences
- Agent Studio's orchestration features are bounded by what the harness supports; we design around subagents, skills, hooks and MCP.
- Role configs are harness-neutral and Claude Code is the first compile target, so other harnesses can be added later.
- Runtime AI inside shipped games can't use the developer's subscription; it defaults to non-LLM techniques.

## Alternatives considered
An embedded agent runner calling LLM APIs directly. Rejected as the default because it requires keys, costs extra and duplicates the harness.
