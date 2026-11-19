// The Inspector's Add Component button: a searchable list of the components the entity can gain.
import { useState } from "react";

import type { EditorSession } from "../engine/session";
import { addComponent, componentChoices, componentRows, type ComponentRow } from "./components";

interface Props {
  session: EditorSession;
  entity: string;
  present: readonly string[];
  onError: (message: string) => void;
}

export function AddComponent({ session, entity, present, onError }: Props) {
  const [rows, setRows] = useState<ComponentRow[]>();
  const [query, setQuery] = useState("");
  const open = () => {
    setQuery("");
    componentRows(session).then(setRows, (e: unknown) => onError(String(e)));
  };
  const pick = (name: string) => {
    setRows(undefined);
    addComponent(session, entity, name).catch((e: unknown) => onError(String(e)));
  };
  return (
    <div style={{ marginTop: 8 }}>
      <button type="button" onClick={() => (rows ? setRows(undefined) : open())} style={{ width: "100%" }}>
        Add Component
      </button>
      {rows && (
        <div style={{ border: "1px solid #444", padding: 4, marginTop: 4 }}>
          <input
            type="search"
            aria-label="Search components"
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Escape" && setRows(undefined)}
            style={{ width: "100%", boxSizing: "border-box" }}
          />
          <ul role="listbox" aria-label="Components" style={{ listStyle: "none", margin: 0, padding: 0 }}>
            {componentChoices(rows, present, query).map((r) => (
              <li
                key={r.name}
                role="option"
                aria-selected={false}
                title={r.description}
                onClick={() => pick(r.name)}
                style={{ padding: "2px 4px", cursor: "pointer" }}
              >
                {r.name} <span style={{ opacity: 0.5 }}>{r.category ?? ""}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
