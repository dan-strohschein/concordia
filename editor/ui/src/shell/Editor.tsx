// The editor window, laid out like Godot 4 (docs/design/editor-ux.md): menu bar with run controls,
// Scene + FileSystem docks on the left, the viewport in the center, the Inspector on the right and
// the Output panel at the bottom, plus the Agent Activity dock under the Inspector. In Agent Studio
// projects the bottom panel also has a Studio tab (ADR-0083). Shortcuts follow Godot.
import { useCallback, useEffect, useState } from "react";

import { Activity } from "../activity/Activity";
import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import { FileSystem } from "../files/FileSystem";
import { Inspector } from "../inspector/Inspector";
import { Outliner } from "../outliner/Outliner";
import { EscalationBadge, TeamDesk } from "../questions/TeamQuestions";
import { useTeamQuestions } from "../questions/questions";
import { Output } from "../output/Output";
import { StudioPanel } from "../studio/StudioPanel";
import { useStudio } from "../studio/studio";
import { isNative } from "../viewport/native";
import { Viewport } from "../viewport/Viewport";
import { ALL_DOCKS, DOCKS, TEAM_DOCKS, visibleDocks, windowGrid, type DockSet } from "./docks";
import { MenuBar, type Menu } from "./MenuBar";
import { useSceneMenu } from "./useSceneMenu";

const dock: React.CSSProperties = { background: "#232323", overflow: "auto", minHeight: 0 };
const title: React.CSSProperties = { padding: "3px 8px", background: "#2d2d2d", fontWeight: 600 };

export function Editor({ session }: { session: EditorSession }) {
  const [selected, setSelected] = useState<string>();
  const [world, setWorld] = useState<"edit" | "play">("edit");
  const scene = useSceneMenu(session);
  const [shown, setShown] = useState<DockSet>(ALL_DOCKS);
  const [grid, setGrid] = useState(true);
  const team = useTeamQuestions(session);
  const studio = useStudio(session);
  const teamMode = studio.teamMode || team.teamMode;
  const [bottom, setBottom] = useState<"Output" | "Studio">("Studio");
  const revealQuestions = useCallback(() => setShown((s) => ({ ...s, "Team Questions": true })), []);
  const visible = visibleDocks(shown, teamMode);
  const layout = windowGrid(visible);
  const bottomTabs = (["Output", "Studio"] as const).filter((d) => visible[d]);
  const bottomTab = bottomTabs.includes(bottom) ? bottom : bottomTabs[0];
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
  const editMenu: Menu = {
    label: "Edit",
    items: [
      { label: "Undo", shortcut: "⌘Z", run: () => act("txn.undo") },
      { label: "Redo", shortcut: "⇧⌘Z", run: () => act("txn.redo") },
    ],
  };
  const viewMenu: Menu = {
    label: "View",
    items: [
      ...DOCKS.filter((d) => teamMode || !TEAM_DOCKS.includes(d)).map((d) => ({
        label: d,
        checked: shown[d],
        run: () => setShown((s) => ({ ...s, [d]: !s[d] })),
      })),
      "separator",
      { label: "Grid", checked: grid, run: () => setGrid((g) => !g) },
    ],
  };
  const run = (method: string, next: "edit" | "play", params: Record<string, unknown> = {}) => {
    void session.call(method, params).then(() => setWorld(next));
  };
  return (
    <div
      style={{
        display: "grid",
        gridTemplateRows: layout.rows,
        gridTemplateColumns: layout.columns,
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
          menus={[
            scene.menu,
            editMenu,
            viewMenu,
            ...["Project", "Debug", "Editor", "Help"].map((label) => ({ label, items: [] })),
          ]}
        />
        <span style={{ flex: 1, textAlign: "center", fontWeight: 600 }}>
          3D
          <EscalationBadge team={team} />
        </span>
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
      <div style={{ display: "grid", gridAutoRows: "minmax(0, 1fr)", minHeight: 0 }}>
        {shown.Scene && (
          <section
            style={{ ...dock, display: "flex", flexDirection: "column", overflow: "hidden" }}
            aria-label="Scene dock"
          >
            <div style={title}>Scene</div>
            <Outliner session={session} selected={selected} onSelect={setSelected} />
          </section>
        )}
        {shown.FileSystem && (
          <section style={{ ...dock, borderTop: "1px solid #111" }} aria-label="FileSystem dock">
            <div style={title}>FileSystem</div>
            <FileSystem session={session} onOpenScene={scene.openScene} />
          </section>
        )}
      </div>
      <main style={{ minHeight: 0 }}>
        <Viewport
          session={session}
          world={world}
          selected={selected}
          onSelect={setSelected}
          grid={grid}
          onToggleGrid={() => setGrid((g) => !g)}
        />
      </main>
      <div style={{ display: "grid", gridAutoRows: "minmax(0, 1fr)", minHeight: 0 }}>
        {shown.Inspector && (
          <section style={dock} aria-label="Inspector dock">
            <div style={title}>Inspector</div>
            <Inspector session={session} entity={selected} />
          </section>
        )}
        {shown["Agent Activity"] && (
          <section style={{ ...dock, borderTop: "1px solid #111" }} aria-label="Agent Activity dock">
            <div style={title}>Agent Activity</div>
            <Activity session={session} onSelect={setSelected} studio={studio} />
          </section>
        )}
        <TeamDesk session={session} team={team} shown={shown["Team Questions"]} onReveal={revealQuestions} />
      </div>
      {bottomTab !== undefined && (
        <footer
          style={{
            gridColumn: "1 / 4",
            ...dock,
            borderTop: "1px solid #111",
            display: "flex",
            flexDirection: "column",
          }}
          aria-label="Bottom panel"
        >
          <div role="tablist" aria-label="Bottom panel" style={{ ...title, display: "flex", gap: 12 }}>
            {bottomTabs.map((d) => (
              <button
                key={d}
                type="button"
                role="tab"
                aria-selected={d === bottomTab}
                onClick={() => setBottom(d)}
                style={{
                  background: "transparent",
                  border: "none",
                  color: "inherit",
                  font: "inherit",
                  opacity: d === bottomTab ? 1 : 0.55,
                  padding: 0,
                }}
              >
                {d}
              </button>
            ))}
          </div>
          <div style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
            {bottomTab === "Output" && (
              <section aria-label="Output panel">
                <Output session={session} />
              </section>
            )}
            {bottomTab === "Studio" && <StudioPanel session={session} studio={studio} />}
          </div>
        </footer>
      )}
      {scene.dialogs}
    </div>
  );
}
