// The Scene dock's right-click menu: add a template (as a child of the clicked entity, or at the
// top level), rename, duplicate and delete.
import { useEffect, useRef, useState } from "react";

import type { EditorSession } from "../engine/session";
import { type Menu, MenuList, popup, useDismiss } from "../shell/MenuBar";
import { addEntity, deleteEntity, duplicateEntity, type Template, templates } from "./actions";

export interface MenuTarget {
  /** The entity right-clicked, or undefined for empty space. */
  id?: string;
  x: number;
  y: number;
}

export function EntityMenu({
  session,
  target,
  onClose,
  onSelect,
  onRename,
}: {
  session: EditorSession;
  target: MenuTarget;
  onClose: () => void;
  onSelect: (id: string | undefined) => void;
  onRename: (id: string) => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  const list = useTemplates(session);
  useDismiss(root, true, onClose);
  const then = (work: Promise<string | undefined>) => void work.then(onSelect, (e: unknown) => console.warn(e));
  const { id } = target;
  const items: Menu["items"] = list.map((t) => ({
    label: `Add ${t.name}`,
    run: () => then(addEntity(session, t.name, id)),
  }));
  if (id !== undefined) {
    items.push(
      "separator",
      { label: "Rename", shortcut: "F2", run: () => onRename(id) },
      { label: "Duplicate", shortcut: "⌘D", run: () => then(duplicateEntity(session, id)) },
      { label: "Delete", shortcut: "Del", run: () => then(deleteEntity(session, id)) },
    );
  }
  return (
    <div ref={root}>
      <MenuList
        label="Entity"
        items={items}
        style={{ ...popup, position: "fixed", left: target.x, top: target.y }}
        onDone={onClose}
      />
    </div>
  );
}

function useTemplates(session: EditorSession): Template[] {
  const [list, setList] = useState<Template[]>([]);
  useEffect(() => {
    let live = true;
    templates(session).then(
      (t) => live && setList(t),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [session]);
  return list;
}
