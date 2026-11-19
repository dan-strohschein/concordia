// The Studio panel (M3 phase 3.8, ADR-0083): task board, approvals, status and the merged command
// log. Every action is an Engine API call as the local human; a fake engine plays the Studio module.
import { act, fireEvent, render, screen, within } from "@testing-library/react";

import { Activity } from "../../src/activity/Activity";
import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { StudioPanel } from "../../src/studio/StudioPanel";
import { type StudioState, type TaskView, useStudio } from "../../src/studio/studio";
import { FakeEngine, type Handler, RpcFailure } from "./support/engine";

const T0 = 1_763_500_000_000;
const NOT_PERMITTED = -32016;

const task = (id: string, status: TaskView["status"], extra: Partial<TaskView> = {}): TaskView => ({
  id,
  uid: `u-${id}`,
  title: `Task ${id}`,
  status,
  role: "level-designer",
  assignee: status === "todo" ? null : "agent:level-designer#1",
  priority: "normal",
  depends_on: [],
  created_by: "agent:director#1",
  acceptance: [{ assert: "arena has four walls" }],
  links: [],
  description: "",
  notes: [],
  review: [],
  path: `studio/tasks/${id}.task.md`,
  ...extra,
});

const ROLES = [
  { role: "director", display_name: "Director", count: 1, color: "#a855f7", agent_color: "purple" },
  { role: "level-designer", display_name: "Level Designer", count: 2, color: "#ff7f50", agent_color: "orange" },
  { role: "qa-playtester", display_name: "QA", count: 1, color: "#ef4444", agent_color: "red" },
];

/** A fake Studio engine; tests edit `state.tasks` and add handlers. */
function studio(tasks: TaskView[], extra: Record<string, Handler> = {}) {
  const state = { tasks };
  const set = (id: string, status: TaskView["status"]) => {
    state.tasks = state.tasks.map((t) => (t.id === id ? { ...t, status } : t));
    return { task: state.tasks.find((t) => t.id === id) };
  };
  const engine = new FakeEngine({
    "session.open": () => ({ session: "s1", concordia_version: "0", protocol: "1" }),
    "events.subscribe": (p) => ({ topics: p["topics"] }),
    "task.list": () => ({ tasks: state.tasks }),
    "studio.roster": () => ({
      team: "t",
      director: "director",
      main_session_actor: "director",
      roles: ROLES,
      disabled: [],
    }),
    "studio.status": () => ({
      tasks: {},
      pending_approvals: state.tasks.filter((t) => t.status === "in_review").map((t) => t.id),
      instances: [{ role: "level-designer", slot: 1, agent: "s1/a7" }],
      last_sync: { ts_ms: T0, actor: "human:local", changed: 3 },
      budgets: {
        max_turns_per_subagent: 80,
        max_mcp_calls_per_task: 400,
        max_mutations_per_task: null,
        headless_max_budget_usd: 2.5,
      },
    }),
    "task.update": (p) => set(String(p["id"]), p["status"] as TaskView["status"]),
    "task.approve": (p) => set(String(p["id"]), "done"),
    "task.reject": (p) => set(String(p["id"]), "rejected"),
    "studio.sync": () => ({
      changes: [{ path: ".claude/agents/writer.md", action: "update" }],
      refused: [],
      stale: false,
    }),
    "studio.doctor": () => ({
      checks: [
        { id: "claude", status: "ok", text: "Claude Code 2.1.328" },
        { id: "sync", status: "fail", text: "the Claude Code files are out of date; run `concordia studio sync`" },
      ],
      ok: false,
    }),
    ...extra,
  });
  return { engine, state };
}

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

function Panel({ session }: { session: EditorSession }) {
  const studioState = useStudio(session);
  return <StudioPanel session={session} studio={studioState} />;
}

async function mountPanel(engine: FakeEngine) {
  const session = await EditorSession.open(new EngineClient(engine));
  render(<Panel session={session} />);
  await settle();
  return session;
}

const column = (status: string) => screen.getByRole("list", { name: status });
const card = (id: string) => screen.getByRole("listitem", { name: new RegExp(id) });

/** Drags `id`'s card onto the `status` column. */
async function drag(id: string, status: string) {
  fireEvent.dragStart(card(id));
  fireEvent.dragOver(column(status));
  fireEvent.drop(column(status));
  await settle();
}

test("task_board_renders_one_column_per_status", async () => {
  const { engine } = studio([task("T-0001", "todo"), task("T-0002", "in_review")]);
  await mountPanel(engine);
  for (const status of ["todo", "in_progress", "in_review", "done", "blocked", "rejected"]) {
    expect(column(status)).toBeTruthy();
  }
  const review = within(column("in_review")).getByRole("listitem");
  expect(review.textContent).toContain("T-0002");
  expect(review.textContent).toContain("level-designer");
  expect(review.textContent).toContain("agent:level-designer#1");
  expect(review.textContent).toContain("arena has four walls");
});

test("cards_show_the_last_evidence_note", async () => {
  const { engine } = studio([
    task("T-0001", "in_review", {
      notes: ["- agent:level-designer#1: built it", "- agent:level-designer#1: screenshot shot-1.png"],
    }),
  ]);
  await mountPanel(engine);
  expect(card("T-0001").textContent).toContain("screenshot shot-1.png");
});

test("dragging_card_to_in_review_calls_task_update_with_human_actor", async () => {
  const { engine } = studio([task("T-0001", "in_progress")]);
  await mountPanel(engine);
  await drag("T-0001", "in_review");
  expect(engine.called("task.update")).toEqual([{ id: "T-0001", status: "in_review", actor: "human:local" }]);
  expect(within(column("in_review")).getByRole("listitem").textContent).toContain("T-0001");
});

test("card_drop_rejected_by_gate_shows_gate_reason_and_reverts", async () => {
  const { engine } = studio([task("T-0001", "todo")], {
    "task.update": () => {
      throw new RpcFailure(NOT_PERMITTED, "todo → in_review needs the task to be in progress first", {
        kind: "TaskTransition",
      });
    },
  });
  await mountPanel(engine);
  await drag("T-0001", "in_review");
  expect(screen.getByRole("alert").textContent).toContain("needs the task to be in progress first");
  expect(within(column("todo")).getByRole("listitem").textContent).toContain("T-0001");
  expect(within(column("in_review")).queryAllByRole("listitem")).toHaveLength(0);
});

test("dropping_on_done_approves_and_on_rejected_asks_for_a_comment", async () => {
  const { engine } = studio([task("T-0001", "in_review"), task("T-0002", "in_review")]);
  await mountPanel(engine);
  await drag("T-0001", "done");
  expect(engine.called("task.approve")).toEqual([{ id: "T-0001", actor: "human:local" }]);
  await drag("T-0002", "rejected");
  expect(engine.called("task.reject")).toEqual([]);
  const form = screen.getByRole("form", { name: "Reject T-0002" });
  fireEvent.change(within(form).getByRole("textbox"), { target: { value: "walls missing" } });
  fireEvent.click(within(form).getByRole("button", { name: "Reject" }));
  await settle();
  expect(engine.called("task.reject")).toEqual([{ id: "T-0002", comment: "walls missing", actor: "human:local" }]);
});

test("approvals_approve_calls_task_approve_and_removes_item", async () => {
  const { engine } = studio([task("T-0001", "in_review"), task("T-0002", "todo")]);
  await mountPanel(engine);
  fireEvent.click(screen.getByRole("tab", { name: /Approvals/ }));
  const list = screen.getByRole("list", { name: "Pending approvals" });
  const item = within(list).getByRole("listitem");
  expect(item.textContent).toContain("T-0001");
  fireEvent.change(within(item).getByRole("textbox"), { target: { value: "checked the walls" } });
  fireEvent.click(within(item).getByRole("button", { name: "Approve" }));
  await settle();
  expect(engine.called("task.approve")).toEqual([{ id: "T-0001", comment: "checked the walls", actor: "human:local" }]);
  expect(within(screen.getByRole("list", { name: "Pending approvals" })).queryAllByRole("listitem")).toHaveLength(0);
});

test("reject_requires_comment", async () => {
  const { engine } = studio([task("T-0001", "in_review")]);
  await mountPanel(engine);
  fireEvent.click(screen.getByRole("tab", { name: /Approvals/ }));
  const item = within(screen.getByRole("list", { name: "Pending approvals" })).getByRole("listitem");
  const reject = within(item).getByRole("button", { name: "Reject" });
  expect((reject as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(within(item).getByRole("textbox"), { target: { value: "   " } });
  expect((reject as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(within(item).getByRole("textbox"), { target: { value: "no lights" } });
  fireEvent.click(reject);
  await settle();
  expect(engine.called("task.reject")).toEqual([{ id: "T-0001", comment: "no lights", actor: "human:local" }]);
});

test("status_shows_instances_budgets_last_sync_and_doctor_checks", async () => {
  const { engine } = studio([]);
  await mountPanel(engine);
  fireEvent.click(screen.getByRole("tab", { name: "Status" }));
  const status = screen.getByRole("tabpanel", { name: "Status" });
  expect(status.textContent).toContain("agent:level-designer#1");
  expect(status.textContent).toContain("400");
  expect(status.textContent).toContain("2.5");
  expect(status.textContent).toContain("human:local");
  fireEvent.click(within(status).getByRole("button", { name: "Run checks" }));
  await settle();
  const checks = within(status).getByRole("list", { name: "Doctor checks" });
  expect(checks.textContent).toContain("Claude Code 2.1.328");
  expect(checks.textContent).toContain("concordia studio sync");
});

test("sync_now_calls_studio_sync_as_the_local_human", async () => {
  const { engine } = studio([]);
  await mountPanel(engine);
  fireEvent.click(screen.getByRole("tab", { name: "Status" }));
  fireEvent.click(screen.getByRole("button", { name: "Sync now" }));
  await settle();
  expect(engine.called("studio.sync")).toEqual([{ actor: "human:local" }]);
  expect(screen.getByRole("tabpanel", { name: "Status" }).textContent).toContain("1 file changed");
});

test("the_board_follows_studio_events", async () => {
  const { engine, state } = studio([task("T-0001", "todo")]);
  await mountPanel(engine);
  state.tasks = [task("T-0001", "in_progress")];
  await act(async () => {
    engine.event({ topic: "studio", kind: "task_changed" });
    await new Promise((r) => setTimeout(r, 0));
  });
  expect(within(column("in_progress")).getByRole("listitem").textContent).toContain("T-0001");
});

test("the_studio_panel_is_hidden_outside_team_mode", async () => {
  const engine = new FakeEngine({
    "session.open": () => ({ session: "s1", concordia_version: "0", protocol: "1" }),
    "events.subscribe": (p) => ({ topics: p["topics"] }),
  });
  await mountPanel(engine);
  expect(screen.queryByRole("region", { name: "Studio panel" })).toBeNull();
});

// --- The command log: Agent Activity merged with the Studio journal ---

const history = [
  {
    seq: 1,
    kind: "commit",
    txn: "t1",
    target: null,
    actor: "agent:level-designer#1",
    label: "spawn Wall",
    ops: 1,
    time_ms: T0 + 1000,
    entities: ["e1"],
    undone: false,
  },
  {
    seq: 2,
    kind: "commit",
    txn: "t2",
    target: null,
    actor: "human:local",
    label: "move Crate",
    ops: 1,
    time_ms: T0 + 3000,
    entities: ["e2"],
    undone: false,
  },
];
const journal = [
  {
    seq: 1,
    v: 1,
    ts_ms: T0,
    actor: "agent:director#1",
    kind: "task_created",
    task: "T-0001",
    title: "Arena",
    role: "level-designer",
  },
  {
    seq: 2,
    v: 1,
    ts_ms: T0 + 2000,
    actor: "agent:level-designer#1",
    kind: "file_edit",
    path: "scenes/arena.scene.ron",
    tool: "Write",
    before: null,
    after: "ab",
  },
  {
    seq: 3,
    v: 1,
    ts_ms: T0 + 4000,
    actor: "agent:qa-playtester#1",
    kind: "task_review",
    task: "T-0001",
    approved: true,
    comment: "ok",
    changes: {},
  },
];

async function mountLog(withStudio = true) {
  const engine = new FakeEngine({
    "session.open": () => ({ session: "s1", concordia_version: "0", protocol: "1" }),
    "events.subscribe": (p) => ({ topics: p["topics"] }),
    "txn.history": () => ({ entries: history }),
    ...(withStudio ? { "studio.activity": () => ({ entries: journal, next: 3 }) } : {}),
  });
  const session = await EditorSession.open(new EngineClient(engine));
  const team: StudioState = {
    teamMode: withStudio,
    tasks: [],
    roles: ROLES,
    status: undefined,
    reload: () => {},
  };
  render(<Activity session={session} onSelect={() => {}} studio={team} />);
  await settle();
  return engine;
}

const rows = () => within(screen.getByRole("list", { name: "Activity" })).getAllByRole("listitem");

test("command_log_merges_scene_commits_and_studio_entries_newest_first", async () => {
  await mountLog();
  expect(rows().map((r) => r.textContent ?? "")).toEqual([
    expect.stringContaining("approved T-0001"),
    expect.stringContaining("move Crate"),
    expect.stringContaining("scenes/arena.scene.ron"),
    expect.stringContaining("spawn Wall"),
    expect.stringContaining("created T-0001"),
  ]);
  const undo = within(rows()[1] as HTMLElement).queryByRole("button", { name: "Undo to here" });
  expect(undo).toBeTruthy();
  expect(within(rows()[0] as HTMLElement).queryByRole("button", { name: "Undo to here" })).toBeNull();
});

test("command_log_filters_by_role_and_actor", async () => {
  await mountLog();
  fireEvent.change(screen.getByRole("combobox", { name: "Role" }), { target: { value: "level-designer" } });
  expect(rows().map((r) => r.textContent ?? "")).toEqual([
    expect.stringContaining("scenes/arena.scene.ron"),
    expect.stringContaining("spawn Wall"),
  ]);
  fireEvent.change(screen.getByRole("combobox", { name: "Role" }), { target: { value: "" } });
  fireEvent.change(screen.getByRole("combobox", { name: "Actor" }), { target: { value: "human:local" } });
  expect(rows()).toHaveLength(1);
  fireEvent.change(screen.getByRole("combobox", { name: "Actor" }), { target: { value: "" } });
  fireEvent.change(screen.getByRole("combobox", { name: "Task" }), { target: { value: "T-0001" } });
  expect(rows()).toHaveLength(2);
  fireEvent.change(screen.getByRole("combobox", { name: "Task" }), { target: { value: "" } });
  fireEvent.change(screen.getByRole("combobox", { name: "Kind" }), { target: { value: "file_edit" } });
  expect(rows()).toHaveLength(1);
});

test("command_log_shows_actor_badge_for_agent_entries", async () => {
  await mountLog();
  const badges = screen.getAllByLabelText(/^role level-designer$/);
  expect(badges.length).toBe(2);
  for (const badge of badges) {
    expect(badge.textContent).toBe("agent:level-designer#1");
    expect((badge as HTMLElement).style.borderColor).toBe("rgb(255, 127, 80)");
  }
  expect(screen.queryByLabelText(/^role local$/)).toBeNull();
});

test("command_log_has_no_studio_parts_outside_team_mode", async () => {
  const engine = await mountLog(false);
  expect(rows()).toHaveLength(2);
  expect(screen.queryByRole("combobox", { name: "Role" })).toBeNull();
  expect(engine.called("studio.activity")).toEqual([]);
});
