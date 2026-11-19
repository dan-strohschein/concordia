// Agent Activity logic: scene commits from `txn.history` merged with the Studio journal
// (`studio.activity`, ADR-0083), newest first, filterable by actor, role, task and kind.
import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import { agentRole } from "../studio/studio";

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

/** The distinct values of `field` in `rows`, sorted. */
export function valuesOf(rows: readonly LogRow[], field: "actor" | "task" | "kind"): string[] {
  return [...new Set(rows.flatMap((r) => (r[field] === undefined ? [] : [r[field]])))].sort();
}

/** Undoes `txn` and every later commit (one call, attributed to the editor). */
export async function undoTo(session: EditorSession, txn: string): Promise<void> {
  await session.call("txn.undo_to", { txn, actor: EDITOR_ACTOR });
}

/** A Studio journal entry from `studio.activity` (kind-specific fields vary). */
export interface JournalEntry {
  seq: number;
  ts_ms: number;
  actor: string;
  kind: string;
  [field: string]: unknown;
}

/** One line of the merged command log: a scene commit or a Studio journal entry. */
export interface LogRow {
  key: string;
  time_ms: number;
  actor: string;
  kind: string;
  task: string | undefined;
  text: string;
  /** The scene commit, for selecting what it touched and undoing to it. */
  commit?: HistoryEntry;
}

/** What the merged log shows. */
export interface LogFilter {
  actor?: string;
  role?: string;
  task?: string;
  kind?: string;
}

const str = (v: unknown): string => (typeof v === "string" ? v : JSON.stringify(v));

/** A one-line description of a Studio journal entry. */
export function describe(e: JournalEntry): string {
  const task = str(e["task"]);
  switch (e.kind) {
    case "task_created":
      return `created ${task} ${str(e["title"])} for ${str(e["role"])}`;
    case "task_changed": {
      const changes = Object.entries((e["changes"] ?? {}) as Record<string, [unknown, unknown]>);
      return `${task}: ${changes.map(([field, [from, to]]) => `${field} ${str(from)} → ${str(to)}`).join(", ")}`;
    }
    case "task_comment":
      return `${task}: ${str(e["text"])}`;
    case "task_review": {
      const comment = typeof e["comment"] === "string" && e["comment"] !== "" ? `: ${e["comment"]}` : "";
      return `${e["approved"] === true ? "approved" : "rejected"} ${task}${comment}`;
    }
    case "file_edit":
      return `${str(e["tool"])} ${str(e["path"])}`;
    case "sync":
      return `synced ${Array.isArray(e["changed"]) ? e["changed"].length : 0} files`;
    case "question_asked":
      return `asked ${str(e["to"])} ${str(e["question"])}: ${str(e["text"])}`;
    case "question_answered":
      return `answered ${str(e["question"])}: ${str(e["text"])}`;
    case "question_escalated":
      return `escalated ${str(e["question"])}: ${str(e["reason"])}`;
    default:
      return e.kind;
  }
}

/** Scene commits and Studio entries in one list, newest first. */
export function timeline(history: readonly HistoryEntry[], journal: readonly JournalEntry[]): LogRow[] {
  const commits: LogRow[] = history
    .filter((h) => h.kind === "commit")
    .map((h) => ({
      key: `scene-${h.seq}`,
      time_ms: h.time_ms,
      actor: h.actor,
      kind: "commit",
      task: undefined,
      text: h.label,
      commit: h,
    }));
  const studio: LogRow[] = journal.map((e) => ({
    key: `studio-${e.seq}`,
    time_ms: e.ts_ms,
    actor: e.actor,
    kind: e.kind,
    task: typeof e["task"] === "string" ? e["task"] : undefined,
    text: describe(e),
  }));
  return [...commits, ...studio].sort((a, b) => b.time_ms - a.time_ms);
}

/** The rows `filter` keeps. */
export function filterRows(rows: readonly LogRow[], filter: LogFilter): LogRow[] {
  return rows.filter(
    (r) =>
      (filter.actor === undefined || r.actor === filter.actor) &&
      (filter.role === undefined || agentRole(r.actor) === filter.role) &&
      (filter.task === undefined || r.task === filter.task) &&
      (filter.kind === undefined || r.kind === filter.kind),
  );
}

/** How many journal entries one `studio.activity` call asks for. */
const PAGE = 500;

/** Studio journal entries after `since`, every page; `next` continues. */
export async function journalSince(
  session: EditorSession,
  since: number,
): Promise<{ entries: JournalEntry[]; next: number }> {
  const entries: JournalEntry[] = [];
  let next = since;
  for (;;) {
    const page = await session.call<{ entries: JournalEntry[]; next: number }>("studio.activity", {
      since: next,
      limit: PAGE,
    });
    entries.push(...page.entries);
    if (page.entries.length < PAGE || page.next === next) return { entries, next: page.next };
    next = page.next;
  }
}

/** Reads the Studio journal forward: each call returns the entries since the last one. */
export class JournalCursor {
  private next = 0;

  constructor(private readonly session: EditorSession) {}

  /** The entries written since the previous call (all of them the first time). */
  async more(): Promise<JournalEntry[]> {
    const page = await journalSince(this.session, this.next);
    this.next = page.next;
    return page.entries;
  }
}
