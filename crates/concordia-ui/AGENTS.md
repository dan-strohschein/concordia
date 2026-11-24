# concordia-ui: agent guide

## Purpose
The in-game UI (M2 phase 2.7b, ADR-0060): retained widgets as ECS components on ordinary entities, laid out by taffy (flex, grid, stack) plus Godot-style anchors, text shaped by parley and rasterized by swash into a glyph atlas, `.theme.ron` themes, and the draw list the renderer's UI pass draws. `#![forbid(unsafe_code)]`; no GPU code. 2.7b.1 is the display half (layout, styles, text, images, nine-patches, progress, separators, the draw list, `ui.inspect`); 2.7b.2 adds the controls, `ui.interact` (pointer, wheel and keyboard inside the simulation), focus, scrolling, sounds and `ui.hit_test`; 2.7b.3 adds text boxes, spin boxes, lists, typed text, the engine clipboard and world-space roots.

## Public entry points
- **Components** (`register_types`): `UiRoot` (canvas, `Stretch`, `Aspect`, `RootSpace`, `z`), `UiNode` (`Length` sizes, `Edges` margin and padding, `gap`, `Layout::{Flex, Grid, Stack}`, `Position::{InFlow, Anchored}`, `grow`, `shrink`, `order`, `visible`, `clip`, `z`), `UiStyle` (`Background::{Solid, Linear, Radial}` with `GradientStop`s, `Border`, radii, opacity, text defaults, `Cursor`), `UiThemed`, `UiText` (`TextRun`s, `TextAlign`, `VAlign`, `Wrap`, `Overflow`, line height, `max_lines`), `UiImage` (`ImageMode`, region, tint, flips), `UiNinePatch` (margins, `AxisStretch`), `UiProgress` (`FillMode`, `RadialFill`, textures or `track` / `bar` fills), `UiSeparator`. Each has `check()` returning `UiError::Field` naming the field.
- **Anchors and stretch:** `Anchored::preset(Preset, size, margin)` (the 16 Godot presets, `PRESETS`), `anchored_rect`, `canvas_map` → `CanvasMap` (`apply`).
- **Themes:** `Theme` / `ThemeClass` (`.theme.ron`), `read_theme`, `write_theme`, `resolve` (inline, then the state's style, then the base; text fields inherit from the parent), `ThemeImporter` (`theme`), `WidgetState`, `ResolvedStyle`.
- **Fonts and text:** `UiFonts` (built-in Noto Sans and Noto Sans Mono; `family`), `FontSource`, `NoFonts`, `Font`, `read_font`, `font_families`, `FontImporter` (`font`: ttf, otf), `SANS`, `MONO`; `shape` → `TextBlock` (`TextLine`, `PlacedGlyph`, underlines, colors, fonts), `ELLIPSIS`.
- **Atlas:** `GlyphAtlas` (`glyph` → `GlyphSlot`, `pages` → `AtlasPage`), `PAGE_SIZE`.
- **Assets:** `UiAssets` (images, themes, fonts), `ImageData`, `MemoryAssets`, `DbAssets` over an `AssetDb` with an `AssetCache`.
- **Layout and drawing:** `UiScene::gather` (`UiTree`, `UiElement`, problems), `compute_layout` → `UiLayout` (`RootLayout`, `NodeLayout`, texts, order, problems), the paint geometry (`image_quads`, `nine_patch_quads`, `progress_fill`, `separator_rect`, `background_paint`, `half_texel_inset`), `draw_list` → `concordia_api::UiDrawList`, and `UiContext` (fonts and atlas; `layout`, `draw`, `draw_scene`).
- **Controls (2.7b.2):** `UiState` (engine-written except `disabled`; `Grab`), `UiButton` (`ActionOn`), `UiToggle` (`ToggleLook`, groups), `UiSlider` (`snap`, `fraction`, `key_step`), `UiDropdown`, `UiTabs`, `UiTooltip`, `UiScroll` (`ScrollBar`), `UiFocus`, `UiSounds`, `UiNode.pointer` (`PointerFilter`), and the signals `UiPressed`, `UiToggled`, `UiValueChanged`, `UiSelected`, `UiFocusChanged`.
- **Parts:** `UiLayout::parts` (`Parts::{Toggle, Slider, Dropdown, Tabs, Scroll}` with `List`, `Row`, `Label`, `Bar`) and `UiLayout::tips` (`Tip`), from the pure geometry in `parts.rs` (`toggle_knob`, `slider_parts`, `slider_value_at`, `dropdown_arrow`, `list_rect`, `first_row`, `clamp_offset`, `scroll_bars`, `offset_for_thumb`, `tip_rect`); `widget_state` picks a node's theme state.
- **Interaction:** `UiInput::read` (the `ui_*` actions: `POINTER`, `CLICK`, `SCROLL`, `VIEWPORT`, `KEYS`), `hit_test` / `hit_with` → `Hit` (`HitPart`), `Index`, `stops`, `interact` (one tick: hover, press, drag, release, wheel, keys; writes values and `UiState`, raises signals and `PlayCue`; returns whether the pointer was consumed), focus (`tab_order`, `step`, `arrow`, `Dir`, `focusable`), and the `ui.interact` system (`add_interact`, `preload`, `INTERACT`).
- **Text entry (2.7b.3):** `UiTextBox` (`selection`, `shown`, `shown_offset`, `text_offset`, `MASK`), `UiSpinBox` (`fit`, `format`), `UiList`, the signals `UiTextChanged` and `UiTextSubmitted`, `UiClipboard` (a play-world resource), and `edit` → `EditResult` (one typed input; Up/Down by logical lines). Text layout records caret stops per line (`TextLine::start` / `end` / `stops`; `caret_x`, `byte_at`); layout records `Parts::TextBox` (`TextBoxParts`: text, origin, caret, selection, scroll, spin arrows).
- **World-space roots (2.7b.3):** `compute_layout_seen` places `RootSpace::World` roots with a clip-from-world matrix (`UiFrame::clip_from_world`; `UiTree::position` from the entity's world transform): centered on the projection at `pixel_size`, hidden behind the camera, past its depth range or off screen, far to near before screen roots. `compute_layout` (no camera) skips them.
- **Engine:** `UiModule` (`[ui]`; `UiShared` / `UiCore` as `Engine::state`; `start_play_in` snapshots the play world's UI assets and adds `ui.interact`), `UI_BOUNDS_LAYER`, `UiInspect` (`ui.inspect`, with each node's `state`), `UiHitTest` (`ui.hit_test`), `UiClipboardGet` (`ui.clipboard`), `importers()`.

## Invariants
- **Presentation only:** layout rects, shaped text, the atlas and draw lists are derived and never stored in a world. Only the components are hashed (`ui_never_changes_the_world_hash_at_1_and_8_threads`). Layout is one pure function of the components, the assets' bytes and the viewport.
- **Interaction is simulation:** `ui.interact` (exclusive, `PrePhysics`, after `input.sample` and before `nav.click` when present) reads only actions, so replays reproduce it; it lays out with its own `UiFonts` and the asset snapshot from play start (never the presentation cache), for the `ui_viewport` action's size or the first root's canvas. `UiState`, widget values and signals are hashed.
- **Typing:** typed text (`TypedText`) goes only to the focused, enabled text box; while one has focus `ui.interact` consumes the keyboard (`ConsumedActions::consume_keys`) and the text box swallows arrows, Accept and Page keys (it edits through typed text). Byte offsets always sit on character boundaries; a password box never copies or cuts.
- **World roots take input** when the module has a lens (`UiModule::with_lens`, a `UiLens`): the simulation and the query methods lay them out through it; the app injects the renderer's `world_ui_lens`. Without one (no `[render]`), they are skipped as in M2.
- **Text sizes its box** (Godot's `Label`): with `Overflow::Visible`, a node's minimum height is its text's height at the box's fixed width, and `Wrap::None` text sets its minimum width (`Pass::text_floor`); `Clip` and `Ellipsis` opt out. A wrapping text leaf whose width comes from layout (stretched, percent, auto) is measured at that width, and its fixed `Px` height is only a floor (`text_leaf_floor`, M3 3.9a): in a squeezed column, siblings without text give way first.
- **The play asset snapshot** covers the play world's UI components and every template's (`preload`, `preload_templates`).
- **The pointer:** the topmost node that `stops` it takes it (interactive widgets, scrolls, tooltips, pictures, boxes with a background or border; `PointerFilter` overrides); open lists are above everything and scroll bars above their content. A held press keeps the pointer consumed. A disabled widget is never hovered, pressed or focused.
- **No system fonts:** fonts load from bytes only (fontique without its `system` feature), so shaping is identical on every machine.
- **Coordinates:** UI space is logical pixels, origin top left, y down. `viewport = ui × map.scale + map.offset`; target pixels are that times the frame's scale.
- **Anchored nodes** take their size from anchors and offsets (setting `width` / `height` is a check failure), lay out their own subtree as an island, and leave their parent's flow.
- **Siblings** lay out by (`order`, id) and draw by (`z`, `order`, id); roots draw by (`z`, id).
- **Invalid components** are dropped with a reported problem (the node keeps a default `UiNode`); a missing asset draws nothing and warns once.
- **Text:** every run's style falls back to the widget's resolved style; an ellipsized block keeps the longest prefix that, with "…", fits its line count.
- **Nine-patches** shrink their margins by the CSS `border-image` factor when the box is too small; nine-patch and region UVs are pulled in by half a texel.

## How to test
`cargo nextest run -p concordia-ui`. Test headers derive every expected value by hand:
- `tests/validate.rs`: each rule names its field.
- `tests/anchors.rs`, `tests/stretch.rs`: Godot's anchor and stretch rules on fixed numbers.
- `tests/text.rs`: metrics from the fonts' own tables (`tests/common`: `head`, `hhea`, `OS/2`, `cmap`, `hmtx`); wrapping, alignment, runs, ellipsis, line height, the importer.
- `tests/layout.rs`: flex, grid, stack, anchored islands, intrinsic sizes, stretch maps, order and problems.
- `tests/paint.rs`: image modes, tiles, nine-patch slices, shrinking and tiling, progress fills, separators, gradients.
- `tests/atlas.rs`, `tests/draw.rs` (with an insta snapshot of the HUD fixture's draw list), `tests/theme.rs`, `tests/engine.rs` (the module, `ui.inspect`, scenes, tweens, the hash boundary).
- `tests/controls_layout.rs`, `tests/controls_draw.rs` (with an insta snapshot of the controls fixture, `tests/common/controls.rs`): parts, state styles, tabs, scroll clamping and clipping, tooltip placement, and their quads.
- `tests/interact.rs` (the play-world rig in `tests/common/play.rs`): buttons, hover, toggles and radios, sliders, dropdowns, tabs, tooltips, wheel and bar drags, Tab and arrow focus, Accept, sounds, viewport input, consumption, `ui.hit_test`.
- `tests/text_edit.rs` (editing, validation), `tests/text_widgets.rs` (caret, selection, scroll, password, spin arrows; typing, clicks and drags, the clipboard, spin boxes, lists), `tests/text_draw.rs` (their quads and the text fixture's snapshot), `tests/world_ui.rs` (projection, parents, offset and pixel size, hiding, far-to-near order).
- `tests/golden.rs`: goldens per feature through the renderer (`tests/golden/ui/`; macOS GPU, skipped elsewhere unless `CONCORDIA_REQUIRE_GPU=1`). A missing golden writes its candidate to `target/golden-candidates/ui/`.

## Common tasks
- **A new widget:** a component with `check()`, registered in `components.rs`, gathered in `scene.rs`, measured in `layout.rs` if it has an intrinsic size, painted in `draw.rs` (pure geometry in `paint.rs`), plus a validation row, a geometry test and a golden.
- **A new control:** also its parts in `parts.rs` (recorded by `layout.rs`), its drawing in `draw/controls.rs`, its hit parts in `hit.rs` and its behaviour in `interact.rs` (press, drag, release, keys), with an `interact.rs` test through the play rig.
- **A new style field:** add it to `UiStyle` (optional) and `ResolvedStyle`, `UiStyle::or`, `ResolvedStyle::over` (and `inherited` if text inherits it), then `check()`.

## Don'ts
- Don't read system fonts or the clock; don't keep layout or atlas state in the world.
- Don't read raw device events or the window in `ui.interact`; only actions replay.
- Don't add GPU or `unsafe` code here: drawing is the renderer's (`concordia-render`'s UI pass, through `concordia_api::UiDrawList`).

## References
- CSS Flexible Box Layout Level 1, CSS Grid Layout Level 1, CSS Backgrounds and Borders Level 3 (radii, `border-image`), CSS Images Level 3 (gradients).
- The OpenType specification (`head`, `hhea`, `OS/2`, `cmap` format 4, `hmtx`); Unicode UAX #14 and UAX #9.
- Godot (MIT) documentation, for semantics only: `Control` (anchors, offsets, grow direction, layout presets, `mouse_filter`, focus neighbours), the multiple-resolutions guide (stretch modes and aspects), `TextureRect`, `NinePatchRect`, `TextureProgressBar`, `ProgressBar`, `Theme`, `BaseButton`, `CheckBox`, `CheckButton`, `ButtonGroup`, `HSlider` / `VSlider`, `OptionButton`, `TabContainer`, `ScrollContainer`, tooltips.
- Godot (MIT) documentation, for semantics only (2.7b.3): `LineEdit`, `TextEdit`, `SpinBox`, `ItemList`.
- WAI-ARIA Authoring Practices (textbox, spinbutton and listbox too): the button (activate on release), checkbox, radio group, slider, listbox / combobox, tabs and keyboard focus patterns.
- Crate documentation: taffy, parley, fontique, swash.
- Fonts: Noto Sans v2.015 and Noto Sans Mono v2.014 (SIL OFL 1.1; `fonts/OFL.txt`, `fonts/VENDOR.md`).
- No Unreal (UMG, Slate), Unity UI or other non-open source was consulted.

## Status
M2 phase 2.7b.1 (display widgets, layout, text, themes, the draw list and its renderer pass, `ui.inspect`, the `ui_bounds` layer; the UI/UX role) and 2.7b.2 (controls, `ui.interact`, focus, scrolling, tooltips, sounds, pointer consumption, `ui.hit_test`, cursors). and 2.7b.3 (text boxes, spin boxes, lists, typed text, the clipboard, world-space roots). M3 phase 3.0c: text sizes its box; world roots take input through an injected lens; the play asset snapshot covers templates.

## Engineering rules
These apply to every change, by humans and agents alike.
1. **TDD on every phase.** Write the tests first, see them fail, implement, then refactor.
2. **SOLID.** Depend on abstractions at crate boundaries; extend through traits rather than by editing callers.
3. **No duplicate functionality.** Before writing any implementation, search for existing functionality (grep, `cargo doc`, crate `AGENTS.md` files). Extend or refactor what exists; never write a parallel version.
4. **Single responsibility at every level:** function, type, module and crate.
5. **Small functions** that do one thing.
6. **Validate parameters.** Every function checks its inputs:
   - Rust: encode invariants in types (newtypes, enums, `NonZero*`); validate at boundaries and return `Result` with a descriptive error; use `debug_assert!` for internal invariants. Public APIs never panic on bad input.
   - TypeScript: strict types, plus runtime validation at API and IPC boundaries.
7. **Short comments.** One-line doc comments on public items; no essays, and no comments that restate the code.
8. **Idiomatic code.**
   - Rust: follow the Rust API Guidelines and keep it clippy-clean; `thiserror` in libraries, `anyhow` only in binaries and tests.
   - Editor UI: idiomatic React and TypeScript, with function components, hooks, strict mode, ESLint and Prettier.
