// The FileSystem dock (Godot's): the project's files as a `res://` tree from `project.files`.
// Double-clicking a scene opens it; files that can be imported show an Import button; models can
// be dragged into the viewport. The listing refreshes when a scene is saved or opened.
import { useCallback, useEffect, useState } from "react";

import type { EditorSession } from "../engine/session";
import { importFile } from "./place";
import { baseName, buildTree, type FolderNode, type ProjectFile } from "./tree";

/** The drag-and-drop type carrying a `ProjectFile` as JSON. */
export const FILE_DRAG_TYPE = "application/x-concordia-file";

const ICONS: Record<ProjectFile["kind"], string> = { scene: "🎬", prefab: "📦", model: "🧊", image: "🖼", other: "📄" };

export function FileSystem({ session, onOpenScene }: { session: EditorSession; onOpenScene: (path: string) => void }) {
  const [root, setRoot] = useState<FolderNode>();
  const [error, setError] = useState<string>();
  const refresh = useCallback(() => {
    session.call<{ files: ProjectFile[] }>("project.files").then(
      (r) => {
        setRoot(buildTree(r.files));
        setError(undefined);
      },
      (e: unknown) => setError(String(e)),
    );
  }, [session]);
  useEffect(() => {
    refresh();
    return session.onEvent((e) => {
      if (e.topic === "scene") refresh();
    });
  }, [session, refresh]);
  const onImport = (file: ProjectFile) =>
    void importFile(session, file.path).then(refresh, (e: unknown) => setError(String(e)));
  if (error !== undefined) return <p style={{ padding: 8, color: "#e88" }}>{error}</p>;
  if (root === undefined) return <p style={{ padding: 8, opacity: 0.5 }}>Loading…</p>;
  return (
    <ul role="tree" aria-label="Project files" style={{ listStyle: "none", margin: 0, padding: "2px 4px" }}>
      <Folder node={root} depth={0} onOpenScene={onOpenScene} onImport={onImport} />
    </ul>
  );
}

interface RowProps {
  depth: number;
  onOpenScene: (path: string) => void;
  onImport: (file: ProjectFile) => void;
}

function Folder({ node, depth, ...rest }: RowProps & { node: FolderNode }) {
  const [open, setOpen] = useState(true);
  return (
    <li role="treeitem" aria-expanded={open}>
      <div style={{ paddingLeft: depth * 12, cursor: "default" }} onClick={() => setOpen(!open)}>
        {open ? "▾" : "▸"} 📁 <span>{node.name}</span>
      </div>
      {open && (
        <ul role="group" style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {node.folders.map((f) => (
            <Folder key={f.path} node={f} depth={depth + 1} {...rest} />
          ))}
          {node.files.map((f) => (
            <File key={f.path} file={f} depth={depth + 1} {...rest} />
          ))}
        </ul>
      )}
    </li>
  );
}

function File({ file, depth, onOpenScene, onImport }: RowProps & { file: ProjectFile }) {
  const name = baseName(file.path);
  const draggable = file.kind === "model";
  return (
    <li role="treeitem" title={file.path}>
      <div
        draggable={draggable}
        onDragStart={(e) => {
          e.dataTransfer.setData(FILE_DRAG_TYPE, JSON.stringify(file));
          e.dataTransfer.effectAllowed = "copy";
        }}
        onDoubleClick={() => file.kind === "scene" && onOpenScene(file.path)}
        style={{ paddingLeft: depth * 12 + 12, display: "flex", gap: 6, alignItems: "center", cursor: "default" }}
      >
        <span aria-hidden>{ICONS[file.kind]}</span>
        <span style={{ flex: 1 }}>{name}</span>
        {file.importer !== null && file.asset === null && (
          <button aria-label={`Import ${name}`} style={{ fontSize: 11 }} onClick={() => onImport(file)}>
            Import
          </button>
        )}
      </div>
    </li>
  );
}
