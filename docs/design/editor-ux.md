# Editor UX

Living document. The author's direction (2026-11-18):
- The editor's **layout and UX follow Godot 4.6.**
- **Godot is the reference** for widget behaviour, gizmo interaction and docking.
- **Features Godot lacks** (Concordia-specific, or Unreal-style) are listed under *Needs visual design* and left basic until designed.
- **Look and feel stays deliberately basic** for now: a plain dark theme with system fonts and no polish.

Godot is MIT-licensed, so following its UX is compatible with the clean-room rule (`docs/clean-room.md`). Unreal is used only as a list of features, never as a source of design or code.

> Written from Godot 4.x's long-standing default layout (4.4–4.5 era). Items marked **(verify 4.6)** should be checked against Godot 4.6 by the author before or during implementation.

## Default layout (Godot's 3D workspace)
```
┌───────────────────────────────────────────────────────────────────────────┐
│ Menu: Scene  Project  Debug  Editor  Help      [3D]          ▶ ⏸ ■ (run)  │
├──────────────┬──────────────────────────────────────────┬─────────────────┤
│ Scene dock   │ Viewport toolbar: Select Move Rotate     │ Inspector       │
│ (Outliner)   │ Scale │ Local/Global │ Snap │ View menu  │                 │
│              │ ┌──────────────────────────────────────┐ │                 │
├──────────────┤ │            3D viewport               │ │                 │
│ FileSystem   │ │  (perspective label top-left,        │ │                 │
│ dock         │ │   axis gizmo top-right)              │ │                 │
│              │ └──────────────────────────────────────┘ │                 │
├──────────────┴──────────────────────────────────────────┴─────────────────┤
│ Bottom panel tabs: Output │ (Debugger, Animation, … later)                 │
└───────────────────────────────────────────────────────────────────────────┘
```
| Godot | Concordia | M1 |
|---|---|---|
| Scene dock (node tree) | Outliner (entity tree from `Parent` links) | yes |
| FileSystem dock | Project files and assets (`asset.list`, import) | placeholder dock; browsing later |
| Inspector dock | Inspector (components as sections, schema-driven widgets) | yes |
| 3D viewport and its toolbar | Viewport (native Metal, ADR-0043) | yes: Select and Move only |
| Run controls (top right) | Play / Pause / Step / Stop (`sim.*`) | yes, with Step added (see below) |
| Bottom panel: Output | Log (`log.tail`) | yes |
| History dock | Undo history (`txn.history`) | later |
| Workspace switcher (2D/3D/Script/AssetLib) | 3D only | only "3D" shown |

## Interaction (Godot conventions)
- **Viewport navigation:**
  - middle mouse: orbit; Shift+middle mouse: pan; wheel: zoom;
  - right mouse held + WASD/QE: free-look;
  - F: focus the selection;
  - View menu / numpad: 1 front, 3 right, 7 top (Ctrl for the opposite side), 5 toggles perspective/orthographic;
  - Concordia adds an **isometric preset** to the View menu (needs visual design; Godot has none).
- **Tools:** Q select, W move, E rotate, R scale (M1 implements Q and W). The gizmo uses red X, green Y and blue Z arrows, plane handles, and a centre handle for screen-space moves.
- **Selection:** click selects; Shift+click adds; selection is shared between the viewport and the Outliner.
- **Editing:**
  - Ctrl/Cmd+Z undo, Ctrl/Cmd+Shift+Z redo, Ctrl/Cmd+S save;
  - F2 rename; Delete deletes; Ctrl/Cmd+D duplicates;
  - drag in the Outliner reparents.
- **Inspector:**
  - one section per component, collapsible, with the label on the left and the editor on the right;
  - numbers drag-to-scrub, or click to type;
  - `Vec3` as three coloured x/y/z fields;
  - rotations shown as Euler degrees;
  - colours as a swatch that opens a picker;
  - assets (`AssetRef`) as a resource picker;
  - an edit commits on Enter or focus loss (one undo step per gesture).
- **Docking (verify 4.6):** Godot docks can be moved between slots and tabbed. M1 ships the fixed default layout; rearrangeable docks come later.

## Needs visual design (no Godot equivalent)
Shown in a basic form until designed:
- **Components on entities.** Godot nodes have one type; Concordia entities carry many components. This needs an "Add Component" control and per-section remove/menu actions.
- **Agents.** Live attribution of who changed what (`human:*` vs `agent:*` in `log.tail`), a highlight when an agent edits the open selection, and agent connection status.
- **Edit world vs play world.** Godot runs the game in a separate window; Concordia simulates inside the editor. The UI needs a clear "playing" indicator, and must show that edits target the edit world.
- **Step N ticks and the world hash.** These are deterministic-simulation controls next to Play/Pause.
- **Isometric camera preset** and an orthographic height control.
- **Screenshot capture** (`capture.screenshot`), as a viewport action.
- **Journal and crash recovery status** (Unreal-like "restore" prompts).

## Look and feel (for now)
A dark neutral theme, system UI font, 13 px, compact rows, and no icons beyond simple glyphs. No animations, theming system or polish until a dedicated pass.
