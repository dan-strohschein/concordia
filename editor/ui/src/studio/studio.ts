// Studio panel state (ADR-0083): the task board, roster and status, live through `studio` events.
// Every action is one Engine API call as the local human; the engine enforces the gates.
import { useEffect, useMemo, useState } from "react";

import { JsonRpcError } from "../api/client";
import { Coalescer } from "../engine/coalesce";
import { EDITOR_ACTOR, type EditorSession } from "../engine/session";

/** JSON-RPC "method not found": the engine has no Studio module. */
const METHOD_NOT_FOUND = -32601;

/** Calls a Studio method; `undefined` when the project has no Agent Studio. */
export async function callStudio<T>(
  session: EditorSession,
  method: string,
  params: Record<string, unknown> = {},
): Promise<T | undefined> {
  try {
    return await session.call<T>(method, params);
  } catch (e) {
    if (e instanceof JsonRpcError && e.code === METHOD_NOT_FOUND) return undefined;
    throw e;
  }
}

/** The role of an agent actor (`agent:qa-playtester#1` → `qa-playtester`); undefined for others. */
export function agentRole(actor: string): string | undefined {
  return /^agent:([^#]+)/.exec(actor)?.[1];
}

/** Role id → the editor's color for it. */
export function roleColors(roles: readonly RoleView[]): Record<string, string> {
  return Object.fromEntries(roles.flatMap((r) => (r.color ? [[r.role, r.color]] : [])));
}

/** The task statuses, in board order. */
export const STATUSES = ["todo", "in_progress", "in_review", "done", "blocked", "rejected"] as const;
export type TaskStatus = (typeof STATUSES)[number];

/** A `task.list` entry. */
export interface TaskView {
  id: string;
  uid: string;
  title: string;
  status: TaskStatus;
  role: string;
  assignee: string | null;
  priority: string;
  depends_on: string[];
  created_by: string;
  acceptance: { assert: string; playtest?: string | null; replay?: string | null }[];
  links: string[];
  description: string;
  notes: string[];
  review: string[];
  path: string;
}

/** A `studio.roster` role (the fields the panel uses). */
export interface RoleView {
  role: string;
  display_name: string;
  count: number;
  color?: string | null;
  agent_color?: string | null;
}

/** `studio.status`. */
export interface StatusView {
  tasks: Record<string, number>;
  pending_approvals: string[];
  instances: { role: string; slot: number; agent: string }[];
  last_sync: { ts_ms: number; actor: string; changed: number } | null;
  budgets: {
    max_turns_per_subagent: number | null;
    max_mcp_calls_per_task: number | null;
    max_mutations_per_task: number | null;
    headless_max_budget_usd: number | null;
  };
}

/** The Studio panel's shared state. */
export interface StudioState {
  teamMode: boolean;
  tasks: TaskView[];
  roles: RoleView[];
  status: StatusView | undefined;
  reload: () => void;
}

type Loaded = Omit<StudioState, "reload">;

const EMPTY: Loaded = { teamMode: false, tasks: [], roles: [], status: undefined };

/** The board, roster and status; `teamMode` false when the project has no Agent Studio. */
export async function loadStudio(session: EditorSession): Promise<Loaded> {
  const [tasks, roster, status] = await Promise.all([
    callStudio<{ tasks: TaskView[] }>(session, "task.list"),
    callStudio<{ roles: RoleView[] }>(session, "studio.roster"),
    callStudio<StatusView>(session, "studio.status"),
  ]);
  if (tasks === undefined) return EMPTY;
  return { teamMode: true, tasks: tasks.tasks, roles: roster?.roles ?? [], status };
}

/** Loads the Studio state and keeps it current from `studio` events. */
export function useStudio(session: EditorSession): StudioState {
  const [state, setState] = useState<Loaded>(EMPTY);
  const loader = useMemo(
    () =>
      new Coalescer(async () => {
        setState(await loadStudio(session));
      }),
    [session],
  );
  useEffect(() => {
    loader.request();
    return session.onEvent((e) => (e.topic === "studio" ? loader.request() : undefined));
  }, [session, loader]);
  return { ...state, reload: () => loader.request() };
}

/** Moves a task to `to`: done approves, rejected rejects (with `comment`), anything else updates. */
export async function moveTask(session: EditorSession, id: string, to: TaskStatus, comment?: string): Promise<void> {
  if (to === "done") return approveTask(session, id, comment);
  if (to === "rejected") return rejectTask(session, id, comment ?? "");
  await session.call("task.update", { id, status: to, actor: EDITOR_ACTOR });
}

/** Approves a task in review (`task.approve`). */
export async function approveTask(session: EditorSession, id: string, comment?: string): Promise<void> {
  const note = comment?.trim();
  await session.call("task.approve", { id, ...(note ? { comment: note } : {}), actor: EDITOR_ACTOR });
}

/** Rejects a task in review; a reason is required. */
export async function rejectTask(session: EditorSession, id: string, comment: string): Promise<void> {
  if (comment.trim() === "") throw new RangeError("a rejection needs a reason");
  await session.call("task.reject", { id, comment: comment.trim(), actor: EDITOR_ACTOR });
}

/** `studio.sync` result. */
export interface SyncResult {
  changes: { path: string; action: string }[];
  refused: string[];
  stale: boolean;
}

/** Regenerates the Claude Code files (`studio.sync`). */
export function syncNow(session: EditorSession): Promise<SyncResult> {
  return session.call<SyncResult>("studio.sync", { actor: EDITOR_ACTOR });
}

/** One `studio.doctor` check. */
export interface DoctorCheck {
  id: string;
  status: "ok" | "note" | "fail";
  text: string;
}

/** Runs the machine checks (`studio.doctor`). */
export async function runDoctor(session: EditorSession): Promise<DoctorCheck[]> {
  return (await session.call<{ checks: DoctorCheck[] }>("studio.doctor")).checks;
}
