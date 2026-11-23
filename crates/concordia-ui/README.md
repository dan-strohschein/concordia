# concordia-ui

Concordia's in-game UI: retained widgets as ECS components (`UiRoot`, `UiNode`, `UiStyle`, `UiThemed`, `UiText`, `UiImage`, `UiNinePatch`, `UiProgress`, `UiSeparator`) saved in ordinary scenes and edited through the Engine API like any component. Containers lay out with taffy (CSS flexbox and grid, plus a stack), and nodes can be pinned with Godot-style anchors and offsets. Roots scale from a design canvas to any window. Text is shaped by parley (styled runs, wrapping, alignment, ellipsis) and rasterized by swash into a glyph atlas, from bundled Noto fonts or imported ones. `.theme.ron` themes give classes per-state styles. The `[ui]` engine module serves `ui.inspect` (layout as numbers) and hands the renderer a draw list for its UI pass. Layout, text and atlases are presentation: they never change a world hash (ADR-0060).

See `AGENTS.md` for invariants, tests and references.
