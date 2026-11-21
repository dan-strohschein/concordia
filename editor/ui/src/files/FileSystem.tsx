// The FileSystem dock (Godot's): the project's files as a `res://` tree from `project.files`.
// Double-clicking a scene opens it; clicking a material shows it in the Inspector; files that can be
// imported show an Import button; models can be dragged into the viewport. The listing refreshes
// when a scene is saved or opened and when an asset is written or imported.
import { useCallback, useEffect, useState } from "react";

import type { EditorSession } from "../engine/session";
import { importFile } from "./place";
import { baseName, buildTree, type FolderNode, type ProjectFile } from "./tree";

/** The drag-and-drop type carrying a `ProjectFile` as JSON. */
export const FILE_DRAG_TYPE = "application/x-concordia-file";

const ICONS: Record<ProjectFile["kind"], string> = {
  scene: "🎬",
  prefab: "📦",
  model: "🧊",
  image: "🖼",
  material: "🎨",
  other: "📄",
};

interface FileSystemProps {
  session: EditorSession;
  onOpenScene: (path: string) => void;
  /** Called when a material is clicked (shown in the Inspector). */
  onSelectMaterial?: (path: string) => void;
  /** The material the Inspector shows, highlighted. */
  selected?: string | undefined;
}

export function FileSystem({ session, onOpenScene, onSelectMaterial, selected }: FileSystemProps) {
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
      if (e.topic === "scene" || e.topic === "asset") refresh();
    });
  }, [session, refresh]);
  const onImport = (file: ProjectFile) =>
    void importFile(session, file.path).then(refresh, (e: unknown) => setError(String(e)));
  if (error !== undefined) return <p style={{ padding: 8, color: "#e88" }}>{error}</p>;
  if (root === undefined) return <p style={{ padding: 8, opacity: 0.5 }}>Loading…</p>;
  return (
    <ul role="tree" aria-label="Project files" style={{ listStyle: "none", margin: 0, padding: "2px 4px" }}>
      <Folder
        node={root}
        depth={0}
        onOpenScene={onOpenScene}
        onImport={onImport}
        onSelectMaterial={onSelectMaterial ?? (() => undefined)}
        selected={selected}
      />
    </ul>
  );
}

interface RowProps {
  depth: number;
  onOpenScene: (path: string) => void;
  onImport: (file: ProjectFile) => void;
  onSelectMaterial: (path: string) => void;
  selected: string | undefined;
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

function File({ file, depth, onOpenScene, onImport, onSelectMaterial, selected }: RowProps & { file: ProjectFile }) {
  const name = baseName(file.path);
  const draggable = file.kind === "model";
  const isSelected = file.path === selected;
  return (
    <li role="treeitem" title={file.path} aria-label={name} aria-selected={isSelected}>
      <div
        draggable={draggable}
        onDragStart={(e) => {
          e.dataTransfer.setData(FILE_DRAG_TYPE, JSON.stringify(file));
          e.dataTransfer.effectAllowed = "copy";
        }}
        onClick={() => file.kind === "material" && onSelectMaterial(file.path)}
        onDoubleClick={() => file.kind === "scene" && onOpenScene(file.path)}
        style={{
          paddingLeft: depth * 12 + 12,
          display: "flex",
          gap: 6,
          alignItems: "center",
          cursor: "default",
          background: isSelected ? "#2f4a6a" : undefined,
        }}
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
