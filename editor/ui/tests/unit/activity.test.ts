// Agent Activity logic (M1 phase 1.15, merged with the Studio journal in M3 phase 3.8): commits
// newest first, filterable by actor; Studio entries interleave by time.
import { type HistoryEntry, filterRows, timeline, valuesOf } from "../../src/activity/transactions";

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

const seqs = (rows: ReturnType<typeof timeline>) => rows.map((r) => r.commit?.seq);

test("lists commits only, newest first", () => {
  expect(seqs(timeline(history, []))).toEqual([6, 4, 2, 1]);
});

test("filters by actor", () => {
  expect(seqs(filterRows(timeline(history, []), { actor: "agent:builder" }))).toEqual([4, 1]);
  expect(filterRows(timeline(history, []), { actor: "agent:nobody" })).toEqual([]);
});

test("actors are the distinct committing actors, sorted", () => {
  expect(valuesOf(timeline(history, []), "actor")).toEqual(["agent:builder", "agent:lighter#2", "human:local"]);
});

test("undone commits keep their flag", () => {
  expect(timeline(history, []).find((r) => r.commit?.seq === 4)?.commit?.undone).toBe(true);
});

test("studio entries interleave by time and filter by role and task", () => {
  const journal = [
    {
      seq: 1,
      ts_ms: 1_700_000_000_003,
      actor: "agent:builder#1",
      kind: "task_comment",
      task: "T-0001",
      text: "walls up",
    },
  ];
  const rows = timeline(history, journal);
  expect(rows.map((r) => r.text)).toEqual(["edit 6", "edit 4", "T-0001: walls up", "edit 2", "edit 1"]);
  expect(filterRows(rows, { role: "builder" }).map((r) => r.text)).toEqual(["edit 4", "T-0001: walls up", "edit 1"]);
  expect(filterRows(rows, { task: "T-0001" })).toHaveLength(1);
});
