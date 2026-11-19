// The Scene dock (Godot) / Outliner: the entity tree; click selects, F2 renames, Delete despawns.
import { useCallback, useEffect, useState } from "react";

import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import { buildTree, type TreeNode } from "./tree";

interface Props {
  session: EditorSession;
  selected: string | undefined;
  onSelect: (id: string | undefined) => void;
}

export function Outliner({ session, selected, onSelect }: Props) {
  const [roots, setRoots] = useState<TreeNode[]>([]);
  const [renaming, setRenaming] = useState<string>();
  const refresh = useCallback(() => {
    session.entities().then(
      (rows) => setRoots(buildTree(rows)),
      () => setRoots([]),
    );
  }, [session]);
  useEffect(refresh, [refresh]);
  useEffect(
    () => session.onEvent((e) => (e.topic === "edit" || e.topic === "scene" ? refresh() : undefined)),
    [session, refresh],
  );

  const rename = (id: string, name: string) => {
    setRenaming(undefined);
    if (name.trim() !== "") void session.setComponent(id, "Name", name.trim());
  };
  const onKey = (e: React.KeyboardEvent) => {
    if (selected === undefined) return;
    if (e.key === "F2") setRenaming(selected);
    if (e.key === "Delete" || e.key === "Backspace") {
      void session.call("entity.despawn", { id: selected, actor: EDITOR_ACTOR });
      onSelect(undefined);
    }
  };
  const node = (n: TreeNode, depth: number): React.ReactNode => (
    <li key={n.id} role="treeitem" aria-selected={n.id === selected}>
      <div
        onClick={() => onSelect(n.id)}
        onDoubleClick={() => setRenaming(n.id)}
        style={{
          paddingLeft: 6 + depth * 14,
          background: n.id === selected ? "#3d5a80" : undefined,
          cursor: "default",
        }}
      >
        {renaming === n.id ? (
          <input
            autoFocus
            aria-label="Rename"
            defaultValue={n.label}
            onBlur={(e) => rename(n.id, e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") rename(n.id, e.currentTarget.value);
              if (e.key === "Escape") setRenaming(undefined);
            }}
          />
        ) : (
          n.label
        )}
      </div>
      {n.children.length > 0 && (
        <ul role="group" style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {n.children.map((c) => node(c, depth + 1))}
        </ul>
      )}
    </li>
  );
  return (
    <ul
      role="tree"
      aria-label="Scene"
      tabIndex={0}
      onKeyDown={onKey}
      style={{ listStyle: "none", margin: 0, padding: 0, outline: "none" }}
    >
      {roots.map((n) => node(n, 0))}
    </ul>
  );
}
