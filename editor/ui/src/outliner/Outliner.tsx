// The Scene dock (Godot) / Outliner: the entity tree; click selects, F2 renames, Delete despawns.
// Edits patch only the entities they touched, and only the rows in view are rendered.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import { flatten, rowWindow, withUpdates, type FlatRow } from "./rows";
import { buildTree, type EntityRow } from "./tree";

interface Props {
  session: EditorSession;
  selected: string | undefined;
  onSelect: (id: string | undefined) => void;
}

const ROW_HEIGHT = 20;
const OVERSCAN = 10;
/** Edits touching more entities than this refresh the whole list instead. */
const PATCH_LIMIT = 200;

/** The scene's rows, kept current from edit and scene events (updates applied in order). */
function useEntityRows(session: EditorSession): Map<string, EntityRow> {
  const [rows, setRows] = useState(() => new Map<string, EntityRow>());
  const queue = useRef(Promise.resolve());
  const enqueue = useCallback((work: () => Promise<void>) => {
    queue.current = queue.current.then(work, work);
  }, []);
  const refresh = useCallback(
    () =>
      enqueue(() =>
        session.entities().then(
          (all) => setRows(new Map(all.map((r) => [r.id, r]))),
          () => setRows(new Map()),
        ),
      ),
    [session, enqueue],
  );
  useEffect(refresh, [refresh]);
  useEffect(
    () =>
      session.onEvent((e) => {
        if (e.topic === "scene") return refresh();
        if (e.topic !== "edit") return;
        const ids = e["entities"];
        if (!Array.isArray(ids) || ids.length > PATCH_LIMIT) return refresh();
        const changed = ids.filter((id): id is string => typeof id === "string");
        enqueue(() => session.entityRows(changed).then((updates) => setRows((prev) => withUpdates(prev, updates))));
      }),
    [session, refresh, enqueue],
  );
  return rows;
}

/** The list's scroll position and height, for rendering only the rows in view. */
function useScrollWindow(count: number) {
  const box = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ scrollTop: 0, height: 800 });
  useEffect(() => {
    const element = box.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => setView((v) => ({ ...v, height: element.clientHeight || v.height })));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const onScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const scrollTop = e.currentTarget.scrollTop;
    setView((v) => ({ ...v, scrollTop }));
  };
  const range = rowWindow({ count, ...view, rowHeight: ROW_HEIGHT, overscan: OVERSCAN });
  return { box, onScroll, range };
}

export function Outliner({ session, selected, onSelect }: Props) {
  const rows = useEntityRows(session);
  const flat = useMemo(() => flatten(buildTree([...rows.values()])), [rows]);
  const { box, onScroll, range } = useScrollWindow(flat.length);
  const [renaming, setRenaming] = useState<string>();

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
  const item = ({ node, depth }: FlatRow) => (
    <li key={node.id} role="treeitem" aria-level={depth + 1} aria-selected={node.id === selected}>
      <div
        onClick={() => onSelect(node.id)}
        onDoubleClick={() => setRenaming(node.id)}
        style={{
          height: ROW_HEIGHT,
          lineHeight: `${ROW_HEIGHT}px`,
          paddingLeft: 6 + depth * 14,
          background: node.id === selected ? "#3d5a80" : undefined,
          cursor: "default",
          whiteSpace: "nowrap",
        }}
      >
        {renaming === node.id ? (
          <input
            autoFocus
            aria-label="Rename"
            defaultValue={node.label}
            onBlur={(e) => rename(node.id, e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") rename(node.id, e.currentTarget.value);
              if (e.key === "Escape") setRenaming(undefined);
            }}
          />
        ) : (
          node.label
        )}
      </div>
    </li>
  );
  return (
    <div
      ref={box}
      onScroll={onScroll}
      data-testid="outliner-scroll"
      style={{ flex: 1, minHeight: 0, overflowY: "auto" }}
    >
      <ul
        role="tree"
        aria-label="Scene"
        tabIndex={0}
        onKeyDown={onKey}
        style={{
          listStyle: "none",
          margin: 0,
          outline: "none",
          paddingTop: range.start * ROW_HEIGHT,
          paddingBottom: (flat.length - range.end) * ROW_HEIGHT,
          paddingLeft: 0,
          paddingRight: 0,
        }}
      >
        {flat.slice(range.start, range.end).map(item)}
      </ul>
    </div>
  );
}
