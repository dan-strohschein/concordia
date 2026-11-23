# ADR-0060: In-game UI: retained ECS widgets, taffy plus anchors, parley and swash, one batched pass
- Status: Accepted (M2 phase 2.7b, 2026-11-22; author decisions U1–U12, P1, P2). 2.7b.1 lands the display widgets, layout, text, themes, the draw-list seam, the `ui` pass and `ui.inspect`. 2.7b.2 (2026-11-23) adds interaction, focus, scrolling, sounds and `ui.hit_test`. 2.7b.3 (2026-11-23) adds text boxes, spin boxes, lists, typed text, the clipboard and world-space UI.
- Date: 2026-11-22
- Builds on: ADR-0044 (asset ids and the DDC), ADR-0047 (overlays), ADR-0063 (determinism contract), ADR-0069 (signals), ADR-0072 (replay v0), ADR-0073 (per-role tool budgets), ADR-0074 (tweens), ADR-0075 (play-world writes), ADR-0076 (`#[reflect(presentation)]`), ADR-0078 (pointer rays and click-to-move)

## Context
Games need HUDs, floating bars, damage numbers and menus. Agents must be able to build them, and so must the editor. That means text files, components and commands, not a tool-style immediate-mode UI. Gameplay reads what the player does in the UI, so a replay must reproduce every click and keystroke exactly, at any thread count. The widget set is generic, like Godot's core controls; nothing is game-specific. Unreal's UMG and Slate are off limits as sources (clean room).

## Decision
1. **Widgets are ECS components on ordinary entities** (`UiRoot`, `UiNode`, `UiStyle`, `UiThemed`, `UiText`, `UiImage`, `UiNinePatch`, `UiProgress`, `UiSeparator`; later `UiState`, `UiButton`, `UiToggle`, `UiSlider`, `UiDropdown`, `UiTabs`, `UiTooltip`, `UiScroll`, `UiFocus`, `UiSounds`, `UiTextBox`, `UiSpinBox` and `UiList`).
   - They're saved in `.scene.ron` and `.prefab.ron`.
   - The tree is the scene hierarchy (`Parent`).
   - Every edit is a command with an actor. The editor has no UI logic of its own.
   - Checkbox, switch and toggle button are one `UiToggle` with a `look`, and a shared `group` makes radios (U9).
2. **Layout:**
   - taffy 0.15 (MIT) lays out containers (CSS Flexbox, CSS Grid with explicit tracks, and a `Stack` mode);
   - an anchor pass then places `Anchored` children with Godot's `Control` anchor, offset and grow semantics (U7);
   - roots map a base canvas to the viewport with Godot's `canvas_items` stretch and aspect semantics.
   - UI space is logical pixels, origin at the top left, y down.
3. **Text:**
   - **parley** 0.13 (Apache-2.0/MIT) shapes (harfrust), breaks lines (ICU4X, UAX #14), handles bidi and aligns styled runs (U5).
   - **swash** 0.2 rasterizes glyphs into an `R8Unorm` shelf-packed atlas kept outside the world.
   - Fonts load from bytes only (fontique without system fonts), so every machine shapes identically.
   - cosmic-text, the earlier plan, was rejected (P1): its current release pulls `ttf-parser`, which RUSTSEC-2026-0192 flags as unmaintained.
4. **Fonts are files, not crates.**
   - OFL-1.1 is accepted for bundled font files only (U8).
   - The engine ships Noto Sans v2.015 (Regular, Bold, Italic, BoldItalic) and Noto Sans Mono v2.014 (Regular), with `OFL.txt`. `licenses/fonts.md` records their versions and SHA-256 hashes.
   - The crate allow lists don't change for fonts.
5. **Themes:** `.theme.ron` maps class names to a base `UiStyle` plus per-state styles. A field resolves in this order: inline, then the state's style, then the base, then the default. State changes are instant. `Tweens` animate numeric style fields.
6. **The determinism boundary:**
   - **Hashed:** every UI component, `UiState`, every widget value and pending UI signals.
   - **Never hashed:** layout rects, shaped text, glyph atlases and draw lists. These are derived by one pure function `layout(world, assets, viewport)`, which the simulation (hit tests) and the drawing path both call.
   - **Input:** the viewport and typed text become input events, sampled per tick and recorded by replay (an ADR-0072 update).
   - **Gameplay hears the UI** through signals on the widget's entity and through the components' values.
   - **The pointer goes to the UI first:** `ui.interact` runs in `PrePhysics` before world readers and marks the actions it used in `ConsumedActions`. World readers such as `nav.click` read `ActionStates::unconsumed`.
7. **The seam with the renderer.**
   - The draw-list types (`UiDrawList`, `UiQuad`, `UiPaint`, `UiAtlas`) live in `concordia-api`, like `DebugGeometry`.
   - A module hook `ui_draw(engine, world, frame)` produces the draw list on the engine thread, and `RenderRequest.ui` carries it.
   - `concordia-render` and `concordia-ui` don't depend on each other.
8. **One batched `ui` pass after tonemap.**
   - `ui.slang` pulls quads through bindless.
   - The shader draws rounded rects, borders, linear and radial gradients (2–8 stops), textures with UV rects, atlas glyphs and progress fill masks.
   - **Clipping is per quad in the fragment shader.** The RHI has no scissor command, and this needs no RHI change and no new `unsafe`.
   - Tiles and nine-patches are exact CPU quads.
   - UI draws in display space after tonemapping, so it isn't exposed or tonemapped. Editor overlays stay on top.
9. **World-space UI** follows its entity (`world_matrix_with`) projected through the view. It is drawn on top, far to near, and hidden when behind the eye or off screen (U11). Depth occlusion is later work.
10. **Tools:**
    - `ui.inspect` (rects, style, state and text lines, so agents check layout with numbers);
    - `ui.hit_test` (2.7b.2);
    - `capture.screenshot` gains `ui` and `ui_scale`;
    - a `ui_bounds` debug layer (D7).
    - The ui-ux role is enabled with a `concordia-ui` skill.
11. **Clipboard:** the OS clipboard goes through `arboard` in the windowed player only, and pastes are recorded as typed input (U12). BSL-1.0, the licence of its Windows dependencies, joins the allow lists (P2).

## Consequences
- UI is text-first and diffable, and agents, scripts and the editor drive it through the same API.
- Layout and text metrics are deterministic and testable on Linux without a GPU; goldens run on macOS.
- `concordia-api` gains a small set of draw types and one module hook.
- The renderer gains an `R8Unorm` texture mapping. Atlas pages upload whole until the RHI gets region copies.
- **Later work:** IME, gamepad devices, occluded world UI, live camera views, markup, SDF text, shadows, blur, theme transition times, list virtualization, drag and drop, and the Godot `Control` import (M5).

## 2.7b.2 notes (interaction)
- **Input is actions only.** `ui.interact` reads `ui_pointer`, `ui_click`, `ui_scroll`, `ui_viewport` and ten key actions from `ActionStates`, so replays record UI sessions with no new track (ADR-0072's rule that raw positions reach the simulation only through actions). The viewport is an action bound to the new `Binding::Viewport`, fed by `InputEvent::Viewport`; the replay format stays version 0, against the refinement's plan of a viewport track and a version bump.
- **Layout inside the simulation** uses the same `compute_layout` as drawing, with its own fonts and an asset snapshot taken when play starts (themes, their fonts, run and style fonts, measured images that the play world's UI names). An asset first named later measures as missing in the simulation, the same in every run.
- **Ordering:** `ui.interact` is an exclusive system; Terra's exclusive systems gained `after_if_present` / `before_if_present`, so it runs after `input.sample` and before `nav.click` when those exist, and projects without `[nav]` still plan.
- **Consumption:** the pointer is consumed over interactive widgets, scroll containers, tooltips, pictures and boxes with a background or border (Godot's `mouse_filter`, as `UiNode.pointer: Auto | Stop | Ignore`), and while a press is held. `ConsumedActions` (in `concordia-input`) hides every pointer-bound action from `ActionStates::unconsumed`; `nav.click` uses it.
- **State:** `UiState` (hover, press, focus, hover ticks, the highlighted row, a thumb grab, the tooltip point) and widget values are hashed simulation state; `disabled` lives on `UiState` (the only field authors set), not on `UiButton`.
- **Drawing:** widget parts (toggle boxes, slider tracks, dropdown arrows as a wedge-masked quad, tab titles, scroll bars, focus rings) are computed once by layout (`UiLayout::parts`) and read by both drawing and hit tests; open lists and tooltips draw above every root.

## 2.7b.3 notes (text and world)
- **Typed text is input** (`InputEvent::Text(TextInput::Insert | Edit { key, select })`), sampled into the tick's `TypedText` and recorded by replays as an optional `text` list on the tick's input change. Ticks without typing write nothing new, so the format stays version 0 (a file with typing needs this build, which the header's build identity already requires).
- **Editing** is a pure function over `UiTextBox` and the engine clipboard; Up and Down move by logical lines (no layout needed); caret and selection geometry come from per-line caret stops read from parley's clusters. A focused text box consumes every key-bound action.
- **The clipboard** is a play-world resource (`UiClipboard`, read by `ui.clipboard`); the windowed player bridges it to the OS through `arboard` (BSL-1.0 allowed, P2) and injects OS pastes as typed text, so replays never read the OS.
- **A spin box is a text box plus arrow parts on one entity**, not built child entities (the refinement): nothing is spawned behind the author's back, and the text box's editing is reused as is.
- **World-space roots** are placed with the drawing camera's clip-from-world matrix (`UiFrame::clip_from_world`, filled by the renderer for captures and the window). Without a camera (the simulation, `ui.inspect`, `ui.hit_test`) they are skipped, so they take no input in M2.

## Rejected
- **egui:** immediate mode, styled like a tool, with no stable entities for agents to edit.
- **A webview in the shipped game:** a second runtime, heavy, and not deterministic.
- **Porting Godot's `Control`/`Container` system:** too large. Only the semantics are taken (anchors, stretch, `TextureRect`, `NinePatchRect`, `TextureProgressBar`).
- **cosmic-text:** the advisory above. Its API would have served.
- **A scissor command in the RHI:** Metal backend work and possible `unsafe`, for no gain over clipping in the shader.

## References
- The CSS Flexible Box Layout Module Level 1, CSS Grid Layout Module Level 1, CSS Backgrounds and Borders Module Level 3 (border-radius, border-image) and CSS Images Module Level 3 (gradients) specifications.
- WAI-ARIA Authoring Practices: the button, checkbox, radio group, slider, tabs, listbox, combobox and tooltip patterns.
- The OpenType specification (`head`, `hhea`, `hmtx`, `OS/2`); Unicode UAX #14 (line breaking) and UAX #9 (bidi).
- Godot (MIT) documentation, for semantics only: `Control` (anchors, offsets, grow direction, layout presets), the multiple-resolutions guide (stretch modes and aspects), `Container`, `TextureRect`, `NinePatchRect`, `TextureProgressBar`, `ProgressBar`, `Theme` and `ButtonGroup`.
- Crate documentation: taffy, parley, fontique, swash, arboard.
- The rounded-box signed distance function (Íñigo Quílez's published 2D distance functions).
- No Unreal (UMG, Slate), Unity UI or other non-open source was consulted.
