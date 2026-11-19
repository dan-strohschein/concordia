# ADR-0082: Shared engine: discovery, attach and lifetime
- Status: Accepted (author's decisions, M3 phase 3.5b, 2026-11-19)
- Date: 2026-11-19
- Builds on: ADR-0080 decision 1 (one engine per project), ADR-0043 (editor host), ADR-0047 (native viewport)

## Context
ADR-0080 decided that `concordia mcp` and the editor share one engine per project. This ADR records how they find it, what happens when its host exits, and what an attached editor gives up.

## Decision
1. **Discovery.** `.concordia/headless.json` holds `{url, token, pid, version, host}` (`host` = `editor | mcp | headless`), owner-only. A process attaches when the pid is alive and the WebSocket handshake accepts the token. Otherwise the file is stale, and the process hosts an engine and rewrites the file.
2. **One decision at a time.** Attach-or-host runs under an exclusive lock on `.concordia/engine.lock`, so two processes starting together end with one host.
3. **Versions must match.** A live engine of another Concordia version is not attached. The start fails and names the host and both versions.
4. **One code path.** `concordia_app::SharedEngine::attach_or_host` serves `concordia mcp`, the editor and `concordia headless --ws`. Clients use the `EngineLink` trait, implemented by the in-process `EngineHandle` and by `RemoteEngine`, which opens one WebSocket connection per engine session, so Studio's per-agent sessions stay separate on the host. The method registry is built locally from the same project's modules.
5. **Who attaches.**
   - `concordia mcp` and the editor attach or host.
   - `concordia headless --ws` refuses to start while another engine is live.
   - The CLI's `studio` and `task` commands attach when one is live; otherwise they use a short-lived, unpublished in-process engine.
   - `concordia headless` on stdio, `concordia run` and `concordia fmt` keep private engines.
6. **The editor attaches with the page's viewport** (author's choice). The native viewport needs the engine in the editor's process, so an attached editor sets `__CONCORDIA_NATIVE__ = false` and renders through the page. Starting the editor first, or launching Claude Code from it (3.8), keeps the native viewport.
7. **Host exit.**
   - A host removes its connection file when it stops (drop, Ctrl-C or SIGTERM), but only if the file still names it.
   - Attached clients notice at once: pending and later calls fail with the new error `EngineExited` (−32017), which names the host's pid.
   - `concordia mcp` then attaches or hosts again on the **next** call (author's choice). Agents re-open their scene; unsaved edits of the old host are gone.
   - The editor shows a banner telling the user to restart it.
8. **`scene.text`** (a read-only core method) serves the `concordia://scene/current` resource to attached clients, which have no in-process engine.

## Alternatives
- **Refuse to start a second editor or MCP server while one is live.** It keeps the native viewport guaranteed, but blocks the common "Claude Code first" order.
- **Handover** (the host saves and exits, and the editor re-hosts). Deferred; it may come with the 3.8 launcher.
- **Stay failed after a host exit** until Claude Code restarts the server. It's simpler, but stalls agent runs.

## Consequences
- The core has 34 methods and 31 MCP tools (`scene.text`).
- The engine's lifetime follows its host: Claude Code's session when `concordia mcp` hosts, the editor window when the editor does.
- Dropping a `WsServer` now closes its open connections.
