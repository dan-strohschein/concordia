// The editor window, laid out like Godot 4 (docs/design/editor-ux.md): menu bar with run controls,
// Scene + FileSystem docks on the left, the viewport in the center, the Inspector on the right and
// the Output panel at the bottom. Shortcuts follow Godot.
import { useEffect, useState } from "react";

import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import { FileSystem } from "../files/FileSystem";
import { Inspector } from "../inspector/Inspector";
import { Outliner } from "../outliner/Outliner";
import { Output } from "../output/Output";
import { isNative } from "../viewport/native";
import { Viewport } from "../viewport/Viewport";
import { MenuBar } from "./MenuBar";
import { useSceneMenu } from "./useSceneMenu";

const dock: React.CSSProperties = { background: "#232323", overflow: "auto", minHeight: 0 };
const title: React.CSSProperties = { padding: "3px 8px", background: "#2d2d2d", fontWeight: 600 };

export function Editor({ session }: { session: EditorSession }) {
  const [selected, setSelected] = useState<string>();
  const [world, setWorld] = useState<"edit" | "play">("edit");
  const scene = useSceneMenu(session);
  const act = (method: string, params: Record<string, unknown> = {}) =>
    void session.call(method, { ...params, actor: EDITOR_ACTOR });
  useEffect(() => {
    const keys = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod || e.target instanceof HTMLInputElement) return;
      if (e.key.toLowerCase() === "z") {
        e.preventDefault();
        act(e.shiftKey ? "txn.redo" : "txn.undo");
      }
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  });
  const run = (method: string, next: "edit" | "play", params: Record<string, unknown> = {}) => {
    void session.call(method, params).then(() => setWorld(next));
  };
  return (
    <div
      style={{
        display: "grid",
        gridTemplateRows: "30px 1fr 140px",
        gridTemplateColumns: "250px 1fr 300px",
        height: "100vh",
        background: isNative() ? "transparent" : "#1b1b1b",
        color: "#ddd",
        font: "13px system-ui",
      }}
    >
      <header
        style={{
          gridColumn: "1 / 4",
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: "0 8px",
          background: "#2a2a2a",
        }}
      >
        <MenuBar
          menus={[scene.menu, ...["Project", "Debug", "Editor", "Help"].map((label) => ({ label, items: [] }))]}
        />
        <span style={{ flex: 1, textAlign: "center", fontWeight: 600 }}>3D</span>
        {scene.error !== undefined && (
          <span role="alert" style={{ color: "#e88", maxWidth: 400, overflow: "hidden", textOverflow: "ellipsis" }}>
            {scene.error}
          </span>
        )}
        <div role="toolbar" aria-label="Run" style={{ display: "flex", gap: 4 }}>
          <button title="Play" onClick={() => run("sim.play", "play")}>
            ▶
          </button>
          <button title="Pause" onClick={() => run("sim.pause", "play")}>
            ⏸
          </button>
          <button title="Step 1 tick" onClick={() => run("sim.step", "play", { ticks: 1 })}>
            ⏭
          </button>
          <button title="Stop" onClick={() => run("sim.stop", "edit")}>
            ■
          </button>
        </div>
      </header>
      <div style={{ display: "grid", gridTemplateRows: "1fr 1fr", minHeight: 0 }}>
        <section
          style={{ ...dock, display: "flex", flexDirection: "column", overflow: "hidden" }}
          aria-label="Scene dock"
        >
          <div style={title}>Scene</div>
          <Outliner session={session} selected={selected} onSelect={setSelected} />
        </section>
        <section style={{ ...dock, borderTop: "1px solid #111" }} aria-label="FileSystem dock">
          <div style={title}>FileSystem</div>
          <FileSystem session={session} onOpenScene={scene.openScene} />
        </section>
      </div>
      <main style={{ minHeight: 0 }}>
        <Viewport session={session} world={world} selected={selected} onSelect={setSelected} />
      </main>
      <section style={dock} aria-label="Inspector dock">
        <div style={title}>Inspector</div>
        <Inspector session={session} entity={selected} />
      </section>
      <footer style={{ gridColumn: "1 / 4", ...dock, borderTop: "1px solid #111" }} aria-label="Output panel">
        <div style={title}>Output</div>
        <Output session={session} />
      </footer>
      {scene.dialogs}
    </div>
  );
}
