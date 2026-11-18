# ADR-0033: MCP adapter (rmcp pin and protocol versions)
- Status: Accepted
- Date: 2026-11-17

## Context
The MCP server is how a subscription agent harness (Claude Code) drives the engine without an API key (ADR-0002). The official Rust SDK, `rmcp`, ships breaking majors often (2.0 in June 2026, 3.0 in July 2026, 3.11 in November 2026), and MCP itself has two live protocol shapes: 2025-11-25 (stateful, `initialize` handshake) and 2026-07-28 (stateless).

## Decision
- `concordia-mcp` is a thin adapter over `concordia-api`: every Engine API method except `session.open` is one tool (`.` → `_`). Each tool's input and output schemas are the generated params and result schemas, byte for byte; tests prove it. The adapter opens one session per process with the configured default actor.
- Pin `rmcp = "=3.11.0"` with `default-features = false` (no macros, no schemars-derived schemas). Upgrades are a deliberate change: bump the pin, fix `src/tools.rs` and `src/resources.rs` (the only files that use rmcp; `tests/src/mcp_client.rs` is the only other user), and rerun the M0 gate.
- Support both protocol versions (rmcp's default `KNOWN_VERSIONS`); a test performs both handshakes.
- Engine errors are **tool errors** (`isError: true`, text with message, suggestions and `see`, plus the JSON-RPC error object as structured content), never protocol errors, so the model can read and self-correct. Unknown tools are protocol errors with a did-you-mean.
- `human:*` actors are rejected over MCP, both as the server's `--actor` and per call.
- Logs go to stderr only; stdout carries protocol frames only (tested with trace logging).
- Resources: `concordia://schema/index`, `concordia://schema/component/{name}`, `concordia://schema/method/{name}`, `concordia://docs/agent-guide` (the repository guide, embedded), `concordia://docs/errors` (generated from `ApiErrorKind::ALL`), `concordia://scene/current` (canonical RON, unsaved changes included).
- `component` params are free strings (not an enum of registered names) so that tool schemas stay identical to the generated params schemas; errors and `schema_get` provide the names.

## Consequences
An rmcp major is a two-file change caught by the gate. The tool list stays under the 12k-token budget (tested). Remote attach to a running `concordia headless` is deferred (M1).

## Alternatives considered
- rmcp's `#[tool]` macros: derive schemas from `schemars`, which would duplicate and drift from `concordia-reflect`.
- Our own MCP implementation: avoids churn but re-implements a moving protocol; revisit only if rmcp becomes unmaintained.
