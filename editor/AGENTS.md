# Editor (`editor/`)

## Purpose
The Concordia editor (ADR-0004, ADR-0043): React panels in a Tauri v2 shell around a native Metal viewport. Layout and UX follow Godot 4.6 (`docs/design/editor-ux.md`).
- **`editor/ui`:** the panels. The same code runs in a browser (streamed viewport) and in the shell.
- **`editor/src-tauri`:** the native shell (its own Cargo workspace).
  - `EngineHost` joins the project's shared engine (ADR-0082). With no live engine, it hosts one in-process, serves the token-protected WebSocket API and writes `.concordia/headless.json`, so `concordia mcp` attaches to the editor. With a live one (e.g. Claude Code started first), it attaches; the native viewport is then off (`__CONCORDIA_NATIVE__ = false`) and the page renders the viewport. Either way it opens the main scene if the engine has none open.
  - If the engine's host exits, `EngineClient` rejects calls and the `EngineLost` banner asks the user to restart the editor.
- **Team Questions** (`editor/ui/src/questions/`, ADR-0081): a dock shown only in Studio projects (when `studio.questions` exists), with threads, filters and pinned escalations.
  - A badge on the dock title and in the title bar counts escalations.
  - A banner stays up until every escalation is answered, and a new escalation reveals the dock.
  - Answers go to `studio.answer` as `human:local`. There is no editor-only logic; the panel follows `studio` events.
- **Studio panel** (`editor/ui/src/studio/`, ADR-0083): a bottom-panel tab beside Output, shown only in Studio projects (`task.list` exists). Its tabs:
  - **Board:** one column per task status. Dropping a card calls `task.update`, `task.approve` (done) or `task.reject` (rejected, with a reason) as `human:local`; the board shows only what the engine returns, so a refused move snaps back with the reason.
  - **Approvals:** tasks in review, with Approve and Reject (a reason is required).
  - **Status:** instances, budget limits, the last sync, `studio.doctor` checks and **Sync now** (`studio.sync`).
  - **Claude Code:** the launcher; desktop shell only.
- **Agent Activity** merges `txn.history` with the Studio journal (`studio.activity`) in Studio projects. It filters by actor, role, task and kind, and colors actors by role (`studio.roster`'s `color`).
- **Claude Code launcher** (`src-tauri/src/claude_launcher.rs`, commands in `launcher_commands.rs`): it finds the user's `claude` and shows its login method (display only).
  - "Open Director" writes `.concordia/launcher/director.command` and runs `open -a <app>` on it.
  - "Run headless" runs `claude -p … --agent director --output-format stream-json --verbose` and streams summarized lines to the page as `launcher://event`, grouped by `parent_tool_use_id`.
  - Children inherit the environment unchanged. The launcher never reads, stores or logs credentials, and its log redacts every environment value.
- **The shell window:**
  - It opens a transparent window and injects the engine URL and `__CONCORDIA_NATIVE__`.
  - The Metal viewport is an `NSView` + `CAMetalLayer` **below** the webview at the page's slot (ADR-0043 as amended), attached with `concordia_render::attach_surface`. The UI calls `viewport.present`, every animation frame during play.

## Invariants
- **No editor-only logic.** The editor is just another Engine API client. Every read is an API call. Every edit is `component.set` / `entity.*` / `txn.*`, attributed to `human:local`. Anything the editor can do, an agent can do through the same API, and vice versa (the e2e test `agent_edit_appears_live_in_inspector` proves the editor sees agent edits).
- **One gesture is one committed edit:** Enter, blur, toggle or pick. Typing alone never calls the engine, and Escape reverts.
- **Live updates come from `events.subscribe`** (`edit`, `scene`, `sim`, and `studio` in Studio projects). Panels refresh what an event names; they never poll for edits (only `log.tail`).
- **Types in `src/api/generated/` come from the engine** (`just editor-schemas`). Never edit them; CI fails when they are stale.
- **The inspector is schema-driven** (`src/inspector/widgets.ts`); unknown shapes fall back to a JSON editor and never crash. Struct-variant enums (`oneOf` of one-key objects, e.g. `MaterialNode`, `AlphaMode`, `Projection`) get a variant picker plus the variant's fields; maps (`additionalProperties` without `properties`, e.g. `Material.nodes`) get rename, remove and add; an empty option gets an Add button with `defaultValue` (required fields only, so the engine fills its own defaults).
- **Materials (M2 2.3c):** clicking a `.material.ron` in the FileSystem dock shows it in the Inspector (`MaterialInspector`), read with `material.get` and saved with `material.set` as `human:local`. An edit the engine refuses stays as a draft with the engine's reason and a Revert button until the material is valid again (draft until valid; the author's choice), so multi-step changes such as renaming a node and its references work. An `asset` event for the shown path reloads it, and any `asset` event refreshes the FileSystem dock; the viewport redraws on every event.

## Rules (ADR-0047)
- **The viewport belongs to the engine:** input, camera, picking, gizmo drags and everything drawn in it (selection, gizmo, grid: `viewport.overlays`). Never draw over the viewport from the page.
- **Panels are thin:** they show engine state and send Engine API calls. Behaviour that more than one panel or client needs goes into the engine behind a method, not into TypeScript.

## Native viewport coordinates
The webview spans the window's content view including the area under the title bar, while the page starts below it. Measure the slot inside the webview, offset by `webview height − window.innerHeight` (`native::content_rect`). Verify placement by capturing the window (`screencapture -l <window>`; `CONCORDIA_EDITOR_PRINT_WINDOW=1` prints the number), not by reading the frame back through the same formula.

## How to test
- `just editor-test`: lint (tsc, ESLint, Prettier), Vitest unit tests, then Playwright in Chromium against `concordia headless --ws` on a temporary copy of `examples/m1-physics-stack`. The e2e tests need a GPU, since the streamed viewport uses `capture.screenshot`.
- `just editor-shell-test`: cargo-deny with `editor/src-tauri/deny.toml` (the root policy plus five named MPL-2.0 exceptions Tauri needs), the host tests (token rejection, main scene, connection file, attach and stale-file hosting), the launcher tests (fake `claude` and `open` shims; never the real `claude`) and the native smoke test (10 presented frames and a non-blank capture; macOS with a GUI session).
- `just editor`: runs the native editor on `examples/m1-physics-stack`.
- `just editor-bench <label>`: performance bench (release; window on top for about a minute per scene size). Reports go to `target/bench/`; `tools/bench/summary.py` prints them. The driver lives in `ui/src/bench/`, and the shell's side is in `src-tauri/src/bench.rs` and the `bench_*` commands.
- Unit tests use `tests/unit/support/engine.ts` (a fake engine behind a `Transport`) and real schema fixtures from `tests/fixtures/schemas` (regenerated by `just editor-schemas`).

## Common tasks
- **Add a widget:**
  1. Give the field a hint in Rust (`#[reflect(widget = "name")]`, or `TypeInfo::with_widget` for a type) so the schema carries `x-concordia-widget`.
  2. Map the hint in `widgetFor` (`src/inspector/widgets.ts`) and add a test over a real schema fixture.
  3. Render it in `ValueEditor` (`src/inspector/fields.tsx`); commit once per gesture.
  4. Run `just editor-schemas`.
- **Add a panel:** place it as Godot does (see `docs/design/editor-ux.md`). If Godot has no equivalent, add it to *Needs visual design* and keep it basic.
- **Add a dependency:** pin an exact version at least two weeks old (pnpm enforces a minimum release age); install with `--ignore-scripts`.

## Don'ts
- Don't add editing behaviour the Engine API lacks; add the method to the engine first.
- Don't let the launcher read, store or log a credential or an environment value, and never pass `--bare` (it skips the subscription login and the project's hooks).
- Don't run the real `claude` in tests; a live run uses the author's subscription and needs their go-ahead.
- Don't poll for scene changes; subscribe to events.
