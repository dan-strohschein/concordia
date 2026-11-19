// Modal dialogs for scene files: a path prompt (New Scene, Open, Save As) and Godot's
// "Save changes before closing?" prompt. Escape cancels both.
import { useState } from "react";

import type { UnsavedChoice } from "./scenes";

const backdrop: React.CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "rgba(0,0,0,0.45)",
  display: "grid",
  placeItems: "center",
  zIndex: 50,
};
const box: React.CSSProperties = {
  background: "#2b2b2b",
  border: "1px solid #111",
  padding: 16,
  minWidth: 380,
  display: "flex",
  flexDirection: "column",
  gap: 10,
};
const row: React.CSSProperties = { display: "flex", justifyContent: "flex-end", gap: 8 };

export function UnsavedDialog({ scene, onChoose }: { scene: string; onChoose: (choice: UnsavedChoice) => void }) {
  return (
    <div style={backdrop}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Unsaved changes"
        style={box}
        onKeyDown={(e) => e.key === "Escape" && onChoose("cancel")}
      >
        <p style={{ margin: 0 }}>Save changes to &apos;{scene}&apos; before closing?</p>
        <div style={row}>
          <button onClick={() => onChoose("cancel")}>Cancel</button>
          <button onClick={() => onChoose("discard")}>Don&apos;t Save</button>
          <button autoFocus onClick={() => onChoose("save")}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

/** What a path prompt asks for. */
export interface PathPrompt {
  title: string;
  /** Pre-filled project-relative path. */
  initial: string;
  /** Paths offered in a list (e.g. existing scenes for Open). */
  suggestions?: string[];
  confirm: string;
}

export function PathDialog({
  prompt,
  onSubmit,
  onCancel,
}: {
  prompt: PathPrompt;
  onSubmit: (path: string) => void;
  onCancel: () => void;
}) {
  const [path, setPath] = useState(prompt.initial);
  const valid = path.trim().endsWith(".scene.ron") && !path.startsWith("/") && !path.split("/").includes("..");
  return (
    <div style={backdrop}>
      <form
        role="dialog"
        aria-modal="true"
        aria-label={prompt.title}
        style={box}
        onKeyDown={(e) => e.key === "Escape" && onCancel()}
        onSubmit={(e) => {
          e.preventDefault();
          if (valid) onSubmit(path.trim());
        }}
      >
        <strong>{prompt.title}</strong>
        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          Path (res://)
          <input autoFocus value={path} onChange={(e) => setPath(e.target.value)} aria-invalid={!valid} />
        </label>
        {!valid && <small style={{ color: "#e88" }}>A project-relative path ending in .scene.ron</small>}
        {prompt.suggestions !== undefined && prompt.suggestions.length > 0 && (
          <ul
            aria-label="Scenes"
            style={{ listStyle: "none", margin: 0, padding: 0, maxHeight: 200, overflow: "auto" }}
          >
            {prompt.suggestions.map((s) => (
              <li key={s}>
                <button
                  type="button"
                  style={{ width: "100%", textAlign: "left", background: s === path ? "#3a4a66" : "transparent" }}
                  onClick={() => setPath(s)}
                  onDoubleClick={() => onSubmit(s)}
                >
                  {s}
                </button>
              </li>
            ))}
          </ul>
        )}
        <div style={row}>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit" disabled={!valid}>
            {prompt.confirm}
          </button>
        </div>
      </form>
    </div>
  );
}
