// Agent Activity filter logic (M1 phase 1.15): commits newest first, filterable by actor.
import { activityRows, actorsOf, type HistoryEntry } from "../../src/activity/transactions";

const entry = (seq: number, kind: string, actor: string, extra: Partial<HistoryEntry> = {}): HistoryEntry => ({
  seq,
  kind,
  txn: kind === "commit" ? `t${seq}` : null,
  target: null,
  actor,
  label: `edit ${seq}`,
  ops: 1,
  time_ms: 1_700_000_000_000 + seq,
  entities: [`e${seq}`],
  undone: false,
  ...extra,
});

const history: HistoryEntry[] = [
  entry(1, "commit", "agent:builder"),
  entry(2, "commit", "human:local"),
  entry(3, "save", "human:local"),
  entry(4, "commit", "agent:builder", { undone: true }),
  entry(5, "undo", "human:local", { target: "t4" }),
  entry(6, "commit", "agent:lighter#2"),
];

test("lists commits only, newest first", () => {
  expect(activityRows(history, undefined).map((r) => r.seq)).toEqual([6, 4, 2, 1]);
});

test("filters by actor", () => {
  expect(activityRows(history, "agent:builder").map((r) => r.seq)).toEqual([4, 1]);
  expect(activityRows(history, "agent:nobody")).toEqual([]);
});

test("actors are the distinct committing actors, sorted", () => {
  expect(actorsOf(history)).toEqual(["agent:builder", "agent:lighter#2", "human:local"]);
});

test("undone commits keep their flag", () => {
  expect(activityRows(history, undefined).find((r) => r.seq === 4)?.undone).toBe(true);
});
