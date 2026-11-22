# ADR-0080: Shared engine and Studio attribution
- Status: Accepted (author's decision, M3 refinement (b) = B1)
- Date: 2026-11-19
- Builds on: ADR-0002 (subscription-first), ADR-0032 (edit world vs play world), ADR-0033 (MCP adapter), ADR-0046 (engine modules), ADR-0049 (Studio right after M1)

## Context
Agent Studio must attribute every engine edit to the Claude Code subagent that made it, confine each role to its scope, and let the human watch it all in the editor. M0/M1 as built get in the way:
- **Two engines.** `concordia mcp` always starts its own engine. The editor starts another one on the same files, so the editor never sees agent edits. (`--connect` was deferred from M0 and never built.)
- **One session for every caller.** The MCP adapter runs every tool call in one engine session. Transactions belong to sessions, so parallel subagents would join each other's transactions and be misattributed.
- **No identity from Claude Code.** Claude Code tells an MCP server nothing about the calling subagent (docs checked against Claude Code 2.1.328). Hooks do get `agent_type`, `agent_id` and `tool_use_id`. A PreToolUse `updatedInput` replaces a tool's whole input and isn't documented as excluded for MCP tools.
- **The command log is per scene** and holds edit-world ops only, so file edits and task changes have nowhere to go.

## Decision (option B1 of the M3 refinement)
1. **One engine per project.** `concordia mcp` and the editor each attach to a live engine found in `.concordia/headless.json`, or host one and publish that file.
2. **One engine session per caller identity** in `concordia mcp`: `agent:<role>#n`, with slots assigned first come and capped by the role's count.
3. **Identity travels in a reserved `_studio` key** that the project-settings PreToolUse hook adds through `updatedInput`, and the adapter strips it before dispatch. Any model-supplied `actor` is ignored. With Studio on, a call without `_studio` is read-only (fail closed).
4. **Non-world events go to a project-level Studio journal,** `.concordia/studio/journal.jsonl`: file edits, task changes and slots. It's append-only, versioned, with an actor on every entry, and writers take an advisory lock. Hooks append to it directly; the engine watches it and publishes `studio` events. The per-scene command log is unchanged and is written only by the engine.
5. **Fallback** if `updatedInput` is not honored for MCP tools: *check mode*. Agents pass `actor`, and the hook denies mismatches.

These are guardrails, not a security boundary. An agent that bypasses hooks gets read-only engine access, and Bash file writes are best effort.

## Alternatives
- **B2: hooks write through the running engine over WebSocket.** One writer, but every audit needs a live engine within the hook's 2 s budget.
- **B3: one MCP server per role** (`--actor agent:<role>`). Attribution is structural, but it multiplies tools in the main session, loses instance numbers and breaks the `mcp__concordia__*` naming.

## Consequences
- The editor's Agent Activity panel shows Claude Code's edits live, and `txn.undo_to` can undo them.
- Parallel roles get `TxnConflict` instead of corrupting each other's transactions.
- The Studio journal adds a second log format, versioned like the scene journal.
- M2 2.4.5 refines the server's path check for `asset.import`: importing writes only the file's sidecar (`<path>.import.ron`) and the cache, so a role may import a file it may write or one whose sidecar it may write. The animator adds clip events to models it doesn't own (`assets/models/**/*.import.ron`, shared with the modeler). Other default roles' results are unchanged.
