# ADR-0043: Editor viewport embedding
- Status: Accepted
- Date: 2026-11-18

## Context
ADR-0004 chose a Tauri v2 editor with React panels around a natively rendered Metal viewport, to be validated in M1. Phase 1.8 built a throwaway spike (a React layout whose viewport-slot rect a `ResizeObserver` reports to Rust) and tried the candidate approaches on macOS (M3 Pro, Retina, macOS 26). Approach B (Tauri multi-webview) was skipped because it is still `unstable` (decision D4).

## Measurements
| Check | A: NSView + CAMetalLayer above the webview | C: docked borderless child NSWindow |
|---|---|---|
| Rect tracking, automated (3 window sizes, 2× scale, `spike_viewport_tracks_slot_rect_on_resize`) | exact (0 pt error) | exact (0 pt error) |
| Live window resize (author, by eye) | stays attached | stays attached |
| Panel-width change (author) | clean | clean |
| Window move (author) | attached | attached |
| Second display / Retina change (author) | correct | correct |
| Full screen, Stage Manager (author) | correct | correct |
| Mouse focus between view and panels (author) | normal | normal |
| Keyboard focus | not tested (the spike had no text field) | not tested |
| CPU (author, Activity Monitor) | low; a 1–2 s spike on virtual-desktop switch came from Google Chrome's helper (another app), not the spike | same |
| Frame pacing of the spike's render tick | p50 10.4 ms (a timer, not display-synced) | same |
| Input-to-photon latency | not measured | not measured |

## Decision
- **Approach A** is the editor's viewport: an `NSView` with a `CAMetalLayer`, added to the window's content view above the webview (`addSubview:positioned:NSWindowAbove`) and positioned at the slot reported by the DOM.
  - Placement: convert the CSS rect to the content view's coordinates (flip y unless the view is flipped); set `contentsScale` to the device pixel ratio and `drawableSize` to the size × scale.
  - Mouse input over the slot reaches the native view directly.
- **Approach C** (child window) is the documented fallback, since it passed every check too. Use it only if a future macOS or Tauri change breaks A.
- **Approach D** (offscreen render streamed into the page) is always built. It serves browser-based Playwright tests and remote or agent viewing, through the existing `capture.screenshot` path.
- **Requirements carried into Phase 1.9:**
  - Present on a display link synced to the screen (ProMotion 120 Hz), not a timer.
  - Add a keyboard-focus test with a real text field (focus handoff between panels and the viewport).
  - Measure input-to-photon latency with the real renderer.

## Consequences
The editor needs `macOSPrivateApi` (a transparent webview) and the `objc2-app-kit` / `objc2-quartz-core` bindings in the editor shell only, never in engine crates. The viewport is a second presentation path, so the RHI gains a swapchain or drawable abstraction in Phase 1.9. Windows and Linux shells need their own equivalents later; D covers them until then.

## Alternatives considered
- **B, Tauri multi-webview:** unstable API; revisit when it is stable.
- **D only:** portable, but adds a copy and latency to every frame; kept as the fallback and test path.
- **A different shell (native UI toolkit):** loses the web panel stack ADR-0004 chose for agent-friendliness.
