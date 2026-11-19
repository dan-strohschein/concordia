# ADR-0047: The editor UI stays web-based, with a native viewport and thin panels
- Status: Accepted
- Date: 2026-11-18
- Builds on: ADR-0004 (Tauri editor with React panels), ADR-0043 (viewport embedding)

## Context
The author asked whether the web-based editor UI can keep up with Unreal-class expectations, and whether to replace it with our own UI while that is still cheap. Phases 1.9d to 1.9f built a bench (`just editor-bench`, `concordia run --bench-latency`), set kill criteria with the author, fixed the engine-side bottlenecks, and isolated the latency with the engine alone.

## Evidence (M3 Pro, 120 Hz)
| Measure | Editor (webview over Metal) | Engine window alone |
|---|---|---|
| Input→photon, camera orbit (mean) | ~25–30 ms | ~26 ms windowed; 14.5 ms fullscreen |
| Orbit, gizmo drag and play frame rate | 108–120 fps | 120 fps |
| Agent edit → outliner, 10k entities | p95 ~60 ms | — |

- **The latency wasn't the web layer.** The engine's frame pipeline was the main cost, fixed with late latching, a shallow present queue, a render thread and native input.
- **The web UI adds about 2 ms.**
- **About 11 ms is the macOS window server's compositing,** which every windowed app pays.
- **Known web-specific costs:**
  - Page-drawn viewport overlays update at the page's 60 Hz `requestAnimationFrame`, so they trail the 120 Hz viewport.
  - WebKit adds a few hundred MB of memory.
  - A webview over the viewport rules out direct-to-display scanout (editor only; games run in the engine window).
  - On Linux, Tauri uses WebKitGTK, the weakest of its webviews.

## Decision
Keep the web UI (React panels in Tauri) for the editor, under two rules that keep a later switch to another UI technology bounded to view code:

1. **The viewport belongs to the engine.** Everything in the 3D viewport is native, so it behaves the same under any UI technology:
   - input, camera, picking and gizmo dragging (done in 1.9d–1.9e);
   - the viewport's drawing, including gizmos, selection outlines, the grid and other overlays (next phase, 1.9g).

   The page only lays out the slot and draws panels around it.
2. **Panels are thin.** A panel shows engine state and sends Engine API calls; no editor behaviour lives in TypeScript. Logic that more than one panel or client needs, such as camera rules, drag math or undo grouping, goes into the engine behind a method. This extends the existing rule that the editor is just an Engine API client.

## Consequences
- **Gains:** the web ecosystem for the large panels ahead (node graphs, timelines and curves, the asset browser, text and script editing, profiler views, large property grids, docking), React and TypeScript fluency for agents, and Playwright end-to-end tests.
- **Costs accepted:** WebKit's memory, two languages, and no direct scanout for the editor viewport.
- **Cost of replacing the UI later:** rewriting the panels' views only, roughly proportional to the number of panels, because behaviour and the viewport already live in the engine.
- **Watch items, re-checked at each milestone gate with the bench and the 1.9d kill criteria:**
  - very large panels (node graphs with thousands of nodes; use canvas or WebGL rendering inside the panel);
  - Linux (WebKitGTK) performance once a Linux editor is in scope;
  - memory.
- **The editor's feature set** (files, scene tree, inspector, menus, viewport tools) is scheduled by ADR-0048 as M1 phases 1.11–1.15, all under these rules.
- **The in-game UI system** (a later milestone) is the natural candidate if the editor ever moves off the web: Unreal and Godot build their editors on their own UI. Thin panels keep that path open.
