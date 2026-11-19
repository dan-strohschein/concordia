// The Agent Activity dock: a live command log. Scene commits (`txn.history`) and, in Studio
// projects, the Studio journal (`studio.activity`: tasks, reviews, file edits, questions) in one
// list, newest first, filterable by actor, role, task and kind, with role-colored actor badges.
// Clicking a commit selects what it touched; "Undo to here" rewinds the scene to before it.
import { useEffect, useMemo, useState } from "react";

import { Coalescer } from "../engine/coalesce";
import type { EditorSession } from "../engine/session";
import { RoleBadge } from "../studio/RoleBadge";
import { agentRole, roleColors, type StudioState } from "../studio/studio";
import {
  filterRows,
  history,
  type HistoryEntry,
  type JournalEntry,
  JournalCursor,
  type LogFilter,
  type LogRow,
  timeline,
  undoTo,
  valuesOf,
} from "./transactions";

interface Props {
  session: EditorSession;
  onSelect: (id: string) => void;
  /** Agent Studio state, for the Studio journal half of the log. */
  studio?: StudioState;
}

/** Journal entries kept in memory. */
const KEEP = 1000;

const time = (ms: number) => new Date(ms).toLocaleTimeString();

/** Scene history, refreshed by `edit` and `scene` events. */
function useHistory(session: EditorSession): HistoryEntry[] {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
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
  return entries;
}

/** The Studio journal in team mode, appended to on `studio` events. */
function useJournal(session: EditorSession, teamMode: boolean): JournalEntry[] {
  const [entries, setEntries] = useState<JournalEntry[]>([]);
  const loader = useMemo(() => {
    const cursor = new JournalCursor(session);
    return new Coalescer(async () => {
      const fresh = await cursor.more();
      if (fresh.length > 0) setEntries((old) => [...old, ...fresh].slice(-KEEP));
    });
  }, [session]);
  useEffect(() => {
    if (!teamMode) return;
    loader.request();
    return session.onEvent((e) => (e.topic === "studio" ? loader.request() : undefined));
  }, [session, loader, teamMode]);
  return teamMode ? entries : [];
}

export function Activity({ session, onSelect, studio }: Props) {
  const teamMode = studio?.teamMode === true;
  const scene = useHistory(session);
  const journal = useJournal(session, teamMode);
  const [filter, setFilter] = useState<LogFilter>({});
  const [error, setError] = useState<string>();
  const rows = useMemo(() => timeline(scene, journal), [scene, journal]);
  const colors = roleColors(studio?.roles ?? []);
  const roles = [
    ...new Set([...(studio?.roles ?? []).map((r) => r.role), ...rows.flatMap((r) => agentRole(r.actor) ?? [])]),
  ].sort();
  const pick = (field: keyof LogFilter) => (value: string) =>
    setFilter((f) => ({ ...f, [field]: value === "" ? undefined : value }));
  const rewind = (txn: string) => undoTo(session, txn).catch((e: unknown) => setError(String(e)));
  return (
    <div style={{ padding: 4, fontSize: 12 }}>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
        <Choice label="Actor" value={filter.actor} options={valuesOf(rows, "actor")} onChange={pick("actor")} />
        {teamMode && (
          <>
            <Choice label="Role" value={filter.role} options={roles} onChange={pick("role")} />
            <Choice label="Task" value={filter.task} options={valuesOf(rows, "task")} onChange={pick("task")} />
            <Choice label="Kind" value={filter.kind} options={valuesOf(rows, "kind")} onChange={pick("kind")} />
          </>
        )}
      </div>
      {error && (
        <p role="alert" style={{ color: "#f88" }}>
          {error}
        </p>
      )}
      <ul aria-label="Activity" style={{ listStyle: "none", margin: "4px 0", padding: 0 }}>
        {filterRows(rows, filter).map((r) => (
          <Row key={r.key} row={r} color={colors[agentRole(r.actor) ?? ""]} onSelect={onSelect} onRewind={rewind} />
        ))}
      </ul>
    </div>
  );
}

interface ChoiceProps {
  label: string;
  value: string | undefined;
  options: string[];
  onChange: (value: string) => void;
}

function Choice({ label, value, options, onChange }: ChoiceProps) {
  return (
    <label>
      {label}{" "}
      <select aria-label={label} value={value ?? ""} onChange={(e) => onChange(e.target.value)}>
        <option value="">All</option>
        {options.map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
      </select>
    </label>
  );
}

interface RowProps {
  row: LogRow;
  color: string | undefined;
  onSelect: (id: string) => void;
  onRewind: (txn: string) => void;
}

function Row({ row, color, onSelect, onRewind }: RowProps) {
  const commit = row.commit;
  return (
    <li style={{ display: "flex", gap: 4, padding: "2px 0", opacity: commit?.undone ? 0.45 : 1 }}>
      <button
        type="button"
        title={commit ? `${commit.ops} ops on ${commit.entities.length} entities` : row.kind}
        onClick={() => commit?.entities[0] !== undefined && onSelect(commit.entities[0])}
        style={{ flex: 1, textAlign: "left", background: "transparent", border: "none", color: "inherit" }}
      >
        {row.text} <RoleBadge actor={row.actor} color={color} />{" "}
        <span style={{ opacity: 0.4 }}>{time(row.time_ms)}</span>
      </button>
      {commit && (
        <button
          type="button"
          disabled={commit.undone || commit.txn === null}
          onClick={() => commit.txn !== null && onRewind(commit.txn)}
        >
          Undo to here
        </button>
      )}
    </li>
  );
}
