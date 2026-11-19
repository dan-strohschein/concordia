# ADR-0004: Editor = web UI + native viewport
- Status: Accepted (confirmed; see ADR-0043)
- Date: 2026-11-17

## Decision
Editor panels are written in TypeScript + React (Vite, pnpm). The 3D viewport is rendered natively by the Rust renderer. Both are Engine API clients. The proposed shell is Tauri v2, with the viewport as a native Metal layer composited with the webview; M1 validates this.

## Consequences
Panels are fast to build, AI builds them well, and theming is easy. Embedding a native viewport alongside a webview is the main technical risk; the fallback is a separate native viewport window docked by the shell.
