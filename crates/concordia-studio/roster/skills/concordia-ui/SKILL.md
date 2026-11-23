---
name: concordia-ui
description: "Builds in-game UI from ECS widgets (UiRoot, UiNode, UiStyle, UiThemed, UiText, UiImage, UiNinePatch, UiProgress, UiSeparator): flex, grid, stack and anchored layout, canvas stretch, .theme.ron themes, and checks layouts with ui_inspect and capture_screenshot."
when_to_use: "Use when a task needs a HUD, a menu or any on-screen widget: laying it out, styling it with colors, gradients, borders and fonts, theming it, making it scale to any window, or checking that it reads well and fits."
user-invocable: false
---
# In-game UI

The UI is ordinary entities in a scene. A **`UiRoot`** is the top of a tree; its children (by `Parent`) carry **`UiNode`** (the box and how it lays out its children) plus what they show: **`UiText`**, **`UiImage`**, **`UiNinePatch`**, **`UiProgress`** or **`UiSeparator`**. **`UiStyle`** sets the look; **`UiThemed`** names a class in a `.theme.ron`. Every edit is a normal command, saved in `.scene.ron`. Layout and drawing never touch the simulation: you can inspect and capture as often as you like.

UI space is logical pixels, origin top left, y down. A root's `canvas` is its design size; with `stretch: Scale` it scales to the window (`aspect: Keep` adds bars, `Expand` shows more, `KeepWidth` / `KeepHeight` keep one side, `Ignore` stretches). Leave `stretch: Disabled` for pixel-exact debug UI. Colors are linear RGBA arrays.

## Steps
1. **A root:** `mcp__concordia__entity_spawn {"components": {"UiRoot": {"canvas": [1280.0, 720.0], "stretch": "Scale", "aspect": "Keep"}, "UiNode": {"layout": "Stack"}}}`. Wrap a whole screen in `mcp__concordia__txn_begin {"label": "pause menu"}` … `mcp__concordia__txn_commit {}`.
2. **Pin panels with anchors** (Godot's rules: each edge sits at `anchor × parent size + offset`). A 240 × 64 panel 12 px from the top left:
   `mcp__concordia__entity_spawn {"components": {"Parent": "018bcfe5-6800-7000-a424-000000000010", "UiNode": {"position": {"Anchored": {"anchors": {"left": 0.0, "top": 0.0, "right": 0.0, "bottom": 0.0}, "offsets": {"left": 12.0, "top": 12.0, "right": 252.0, "bottom": 76.0}}}, "padding": {"left": 8.0, "top": 8.0, "right": 8.0, "bottom": 8.0}, "gap": [6.0, 6.0], "layout": {"Flex": {"direction": "Column"}}}, "UiStyle": {"background": {"Linear": {"angle_deg": 180.0, "stops": [{"offset": 0.0, "color": [0.05, 0.08, 0.2, 0.9]}, {"offset": 1.0, "color": [0.01, 0.02, 0.06, 0.9]}]}}, "border": {"width": 2.0, "color": [0.6, 0.65, 0.8, 1.0]}, "radius": [8.0, 8.0, 8.0, 8.0]}}}`.
   The 16 Godot presets (`TopRight`, `CenterBottom`, `FullRect`, …) are the `Preset` names in `concordia_ui::Anchored::preset`: right and bottom presets use anchor 1 and negative offsets (TopRight 160 × 28, margin 12: anchors left/right 1, offsets left −172, right −12, top 12, bottom 40).
3. **Lay out children** with the parent's `layout`: `{"Flex": {"direction": "Row", "justify": "SpaceBetween", "align_items": "Center"}}` (CSS Flexbox), `{"Grid": {"columns": [{"Px": 100.0}, {"Fr": 1.0}], "rows": [{"Px": 40.0}]}}` (CSS Grid), or `"Stack"` (children overlap, filling the box). Children size with `width` / `height` (`"Auto"`, `{"Px": 120.0}`, `{"Percent": 50.0}`), `grow`, `min_*` / `max_*`, `margin`; `order` sorts siblings, `z` draws higher ones on top, `clip: true` cuts children at the box, `visible: false` hides a subtree.
4. **Text:** `mcp__concordia__component_set {"entity": "018bcfe5-6800-7000-a424-000000000011", "component": "UiText", "value": {"runs": [{"text": "Deal "}, {"text": "12", "size": 28.0, "weight": 700.0, "color": [1.0, 0.55, 0.05, 1.0]}, {"text": " fire damage"}], "align": "Center", "wrap": "Word", "overflow": "Ellipsis", "max_lines": 2}}`. Runs inherit the font, size, weight, italic and color from the node's style, which inherits from its parent. Fonts are assets: `builtin:font/noto_sans`, `builtin:font/noto_sans_mono`, or a `.ttf` / `.otf` you import (`mcp__concordia__asset_import {"path": "assets/fonts/title.ttf"}`).
5. **Style:** `mcp__concordia__component_set {"entity": "018bcfe5-6800-7000-a424-000000000011", "component": "UiStyle", "value": {"font": "builtin:font/noto_sans", "font_size": 18.0, "color": [1.0, 1.0, 1.0, 1.0], "opacity": 0.9}}`. Backgrounds are `Solid`, `Linear` (CSS angle: 0 bottom→top, 90 left→right) or `Radial` (`center` and `radius` as fractions of the box), each with 2–8 stops.
6. **Pictures and bars:** `mcp__concordia__entity_spawn {"components": {"Parent": "018bcfe5-6800-7000-a424-000000000012", "UiNode": {"width": {"Px": 16.0}, "height": {"Px": 16.0}}, "UiImage": {"image": "asset:0190b5c0-0000-7000-8000-0000000000c1", "mode": "KeepAspectCentered"}}}` (modes as Godot's `TextureRect`; `region` picks part of an atlas). A frame that keeps its corners: `mcp__concordia__entity_spawn {"components": {"Parent": "018bcfe5-6800-7000-a424-000000000010", "UiNode": {}, "UiNinePatch": {"image": "asset:0190b5c0-0000-7000-8000-0000000000c2", "margins": [8, 8, 8, 8], "axis_h": "Tile"}}}`. A bar: `mcp__concordia__entity_spawn {"components": {"Parent": "018bcfe5-6800-7000-a424-000000000012", "UiNode": {"height": {"Px": 14.0}}, "UiProgress": {"value": 60.0, "max": 100.0, "fill": "LeftToRight", "track": {"Solid": [0.1, 0.1, 0.12, 1.0]}, "bar": {"Solid": [0.8, 0.1, 0.1, 1.0]}}}}`; radial gauges use `"fill": "Clockwise"` with `radial`, and textured bars set `under`, `progress` and `over` images. A rule: `mcp__concordia__entity_spawn {"components": {"Parent": "018bcfe5-6800-7000-a424-000000000010", "UiNode": {"height": {"Px": 4.0}}, "UiSeparator": {"thickness": 2.0}}}`.
7. **Theme** shared looks: write a `.theme.ron` under `ui/` (shape: `mcp__concordia__schema_get {"name": "Theme"}`), import it (`mcp__concordia__asset_import {"path": "ui/main.theme.ron"}`, then `mcp__concordia__asset_list {}` for its id) and name a class: `mcp__concordia__component_set {"entity": "018bcfe5-6800-7000-a424-000000000013", "component": "UiThemed", "value": {"theme": "asset:0190b5c0-0000-7000-8000-0000000000c3", "class": "button"}}`. A field resolves inline style first, then the class's state style (hover, pressed, disabled, focused), then its base. Animate numbers (opacity, radius, colors, a bar's value) with `Tweens` on the field, e.g. `"field": "/opacity"` of `UiStyle`.
8. **Save:** `mcp__concordia__scene_save {}`.

A theme (`ui/main.theme.ron`):
```ron
Theme(
    classes: {
        "button": (
            base: (background: Some(Solid((0.12, 0.14, 0.2, 1.0))), radius: Some((6.0, 6.0, 6.0, 6.0)), font_size: Some(18.0)),
            hover: (background: Some(Solid((0.2, 0.24, 0.34, 1.0)))),
            pressed: (background: Some(Solid((0.08, 0.09, 0.13, 1.0)))),
        ),
        "title": (
            base: (font_size: Some(32.0), weight: Some(700.0), color: Some((1.0, 0.9, 0.4, 1.0))),
        ),
    },
)
```

## Check it
- **Numbers first:** `mcp__concordia__ui_inspect {"width": 1280, "height": 720}` lays the edit world out and returns every node's `rect`, `content`, `clip`, resolved `style` and text `lines` (text, width, baseline). Check the play world with `{"world": "play"}` after `mcp__concordia__sim_step {"ticks": 1}`, and one subtree with `{"entity": "018bcfe5-6800-7000-a424-000000000010"}`. Components that fail their checks are listed in `problems` (and drawn with defaults).
- **Then pixels:** `mcp__concordia__capture_screenshot {"width": 1280, "height": 720}` and `{"width": 1920, "height": 1080}`; `{"debug": ["ui_bounds"]}` outlines every node; `{"ui": false}` shows the scene without the UI; `{"ui_scale": 2.0}` draws at high DPI. Stop the game with `mcp__concordia__sim_stop {}`.
- Read a widget back with `mcp__concordia__entity_get {"id": "018bcfe5-6800-7000-a424-000000000011"}` or `mcp__concordia__component_get {"entity": "018bcfe5-6800-7000-a424-000000000011", "component": "UiText"}`; nudge it with `mcp__concordia__component_patch {"entity": "018bcfe5-6800-7000-a424-000000000011", "component": "UiStyle", "patch": {"font_size": 20.0}}`; find widgets with `mcp__concordia__query_run {"with": ["UiNode"], "select": ["UiNode", "Parent"]}`.

## Without an image generator
You can't paint art yet. Build the look from style: gradients, borders, radii and opacity, nine-patches of art you are given, and the built-in fonts. Ask the director for icons or textures you need.

## Pitfalls
- An anchored node's size comes from its anchors and offsets: leave its `width` / `height` `"Auto"` (setting them is a reported problem).
- Text in an `"Auto"` box measures to its content; give long text a width so it wraps.
- Hidden (`visible: false`) nodes and their children take no space.
