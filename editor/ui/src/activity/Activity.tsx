// The Agent Activity dock: a live list of transactions by actor. Clicking one selects what it
// touched; "Undo to here" rewinds the scene to before it.
import { useEffect, useMemo, useState } from "react";

import { Coalescer } from "../engine/coalesce";
import type { EditorSession } from "../engine/session";
import { activityRows, actorsOf, history, undoTo, type HistoryEntry } from "./transactions";

interface Props {
  session: EditorSession;
  onSelect: (id: string) => void;
}

const time = (ms: number) => new Date(ms).toLocaleTimeString();

export function Activity({ session, onSelect }: Props) {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [actor, setActor] = useState<string>();
  const [error, setError] = useState<string>();
  const loader = useMemo(
    () =>
      new Coalescer(async () => {
        await history(session).then(setEntries, () => setEntries([])); // no scene open: empty
      }),
    [session],
  );
  useEffect(() => {
    loader.request();
    return session.onEvent((e) => (e.topic === "edit" || e.topic === "scene" ? loader.request() : undefined));
  }, [session, loader]);
  const rewind = (txn: string) => undoTo(session, txn).catch((e: unknown) => setError(String(e)));
  return (
    <div style={{ padding: 4, fontSize: 12 }}>
      <label>
        Actor{" "}
        <select value={actor ?? ""} onChange={(e) => setActor(e.target.value === "" ? undefined : e.target.value)}>
          <option value="">All</option>
          {actorsOf(entries).map((a) => (
            <option key={a} value={a}>
              {a}
            </option>
          ))}
        </select>
      </label>
      {error && (
        <p role="alert" style={{ color: "#f88" }}>
          {error}
        </p>
      )}
      <ul aria-label="Transactions" style={{ listStyle: "none", margin: "4px 0", padding: 0 }}>
        {activityRows(entries, actor).map((e) => (
          <li key={e.seq} style={{ display: "flex", gap: 4, padding: "2px 0", opacity: e.undone ? 0.45 : 1 }}>
            <button
              type="button"
              title={`${e.ops} ops on ${e.entities.length} entities`}
              onClick={() => e.entities[0] !== undefined && onSelect(e.entities[0])}
              style={{ flex: 1, textAlign: "left", background: "transparent", border: "none", color: "inherit" }}
            >
              {e.label} <span style={{ opacity: 0.6 }}>{e.actor}</span>{" "}
              <span style={{ opacity: 0.4 }}>{time(e.time_ms)}</span>
            </button>
            <button type="button" disabled={e.undone || e.txn === null} onClick={() => e.txn !== null && rewind(e.txn)}>
              Undo to here
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
