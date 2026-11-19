# ADR-0048: Editor v1 joins M1; engine breadth before the port
- Status: Accepted; amended by ADR-0049 (agent-first scope, phases renumbered 1.11–1.16)
- Date: 2026-11-18
- Builds on: ADR-0047 (the editor UI stays web-based), ADR-0004

## Context
After using the M1 editor by hand, the author found it works well but can do very little: select and move an entity, play the simulation, and not much else.
- **No milestone plans general editor work.**
  - M3's "Studio" is Agent Studio (the agent team, task board and replay), with only one editor panel.
  - M4 adds a graph editor.
  - Menus, scene files, an entity tree with create, duplicate and reparent, an inspector that adds components, rotate and scale gizmos, and debug views are scheduled nowhere.
- **M2 ("Port begins") bundles two kinds of work:**
  - engine breadth that every later milestone needs: animation, navigation, scripting tiers, audio, in-game UI, replay, and FBX, material and shader import;
  - work specific to the reference port: the Godot importer, GDScript→Rust translation, and the playable slice.

  The author judged the engine too incomplete for a port, and wants the port near the end, after the 3D engine is broad enough to carry it.

## Decision
1. **M1 gains phases 1.11–1.15, "Editor v1",** before the gate:
   - Files: scenes, a FileSystem dock, import, drag-in instancing;
   - the Scene tree: templates, duplicate, reparent, multi-select;
   - the Inspector: add and remove components, a widget for every field type;
   - working menus and shortcuts: one action registry, a command palette, the native menu bar, Undo History;
   - viewport improvements: rotate and scale gizmos, snapping, light and camera icons, debug views, a performance panel.

   They follow ADR-0047: thin panels over the Engine API, and everything in the viewport drawn by the engine. The M1 gate runs after them. Phase 1.10 (Vulkan) stays an optional stretch.
2. **M2 becomes "Engine breadth":**
   - it keeps phases 2.3 (asset import: FBX, materials, shaders), 2.4 (animation), 2.5 (navigation), 2.6 (scripting tiers), 2.7 (audio and in-game UI) and 2.8 (replay);
   - phases 2.1 (Godot text formats and the interchange model), 2.2 (GDScript tools and the translation workflow) and 2.9 (the playable slice) move to **M5 "Bring your game"**, which is the same kind of work and comes late.
   - The detailed restructuring of the M2 and M5 plans happens in M2's refinement pass, which its plan already requires. Until then each plan carries a note pointing here.
3. **Dependencies follow the split:** M3 (replay), M4 (graphs on scripting) and M6 (plugins) depend on M2 "Engine breadth", not on the port.

## Consequences
- **The author gets a usable editor now,** as the main way to find gaps in the 3D engine. M1 grows by about five phases (several weeks with agents) before its gate.
- **The port starts later,** on a broader engine, with fewer half-built subsystems to work around.
- **The editor work adds Engine API methods** (`project.files`, `prefab.instantiate`, `viewport.drop`, `entity.templates/create/duplicate/reparent`, `component.add`, `render.stats`, and gizmo modes), all usable by agents.
- **`docs/design.md`'s milestone list and `plans/README.md` change accordingly.**
