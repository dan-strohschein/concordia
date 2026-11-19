// Agent Activity logic: the committed transactions from `txn.history`, newest first, by actor.
import { EDITOR_ACTOR, type EditorSession } from "../engine/session";

/** A `txn.history` entry. */
export interface HistoryEntry {
  seq: number;
  kind: string;
  txn: string | null;
  target: string | null;
  actor: string;
  label: string;
  ops: number;
  time_ms: number;
  entities: string[];
  undone: boolean;
}

/** How many history events the panel reads. */
const LIMIT = 500;

/** Recent history, oldest first. */
export async function history(session: EditorSession): Promise<HistoryEntry[]> {
  return (await session.call<{ entries: HistoryEntry[] }>("txn.history", { limit: LIMIT })).entries;
}

/** Commits by `actor` (all actors when undefined), newest first. */
export function activityRows(entries: readonly HistoryEntry[], actor: string | undefined): HistoryEntry[] {
  return entries.filter((e) => e.kind === "commit" && (actor === undefined || e.actor === actor)).reverse();
}

/** The distinct actors that committed, sorted. */
export function actorsOf(entries: readonly HistoryEntry[]): string[] {
  return [...new Set(entries.filter((e) => e.kind === "commit").map((e) => e.actor))].sort();
}

/** Undoes `txn` and every later commit (one call, attributed to the editor). */
export async function undoTo(session: EditorSession, txn: string): Promise<void> {
  await session.call("txn.undo_to", { txn, actor: EDITOR_ACTOR });
}
