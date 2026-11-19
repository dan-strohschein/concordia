// The Outliner at scale: incremental updates from edit events, and only visible rows rendered
// (M1 phase 1.9d).
import { act, render, screen } from "@testing-library/react";

import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { Outliner } from "../../src/outliner/Outliner";
import { flatten, rowWindow } from "../../src/outliner/rows";
import { buildTree, type EntityRow } from "../../src/outliner/tree";
import { FakeEngine, oneEntity } from "./support/engine";

const row = (id: string, name: string, parent?: string): EntityRow => ({
  id,
  components: { Name: name, ...(parent === undefined ? {} : { Parent: parent }) },
});

test("flatten lists the tree depth-first with levels", () => {
  const tree = buildTree([row("w", "World"), row("l", "Lamp", "w"), row("b", "Box")]);
  expect(flatten(tree).map((r) => [r.node.label, r.depth])).toEqual([
    ["Box", 0],
    ["World", 0],
    ["Lamp", 1],
  ]);
});

test("the row window covers the visible rows plus overscan, clamped to the list", () => {
  expect(rowWindow({ count: 10_000, scrollTop: 2_000, height: 400, rowHeight: 20, overscan: 5 })).toEqual({
    start: 95,
    end: 125,
  });
  expect(rowWindow({ count: 3, scrollTop: 0, height: 400, rowHeight: 20, overscan: 5 })).toEqual({ start: 0, end: 3 });
  expect(() => rowWindow({ count: 1, scrollTop: 0, height: 400, rowHeight: 0, overscan: 0 })).toThrow(RangeError);
});

function bigScene(n: number) {
  const rows = Array.from({ length: n }, (_, i) => ({
    id: `e${i}`,
    components: { Name: `Item ${String(i).padStart(5, "0")}` },
  }));
  const names = new Map(rows.map((r) => [r.id, r.components.Name]));
  const engine = new FakeEngine({
    ...oneEntity({ translation: [0, 0, 0] }),
    "query.run": () => ({ rows }),
    "entity.get": (p) => ({ id: p["id"], components: { Name: names.get(String(p["id"])) } }),
  });
  return { engine, names };
}

test("an edit event refetches only the edited entities", async () => {
  const { engine, names } = bigScene(5_000);
  const session = await EditorSession.open(new EngineClient(engine));
  render(<Outliner session={session} selected={undefined} onSelect={() => undefined} />);
  await screen.findByRole("treeitem", { name: "Item 00000" });
  expect(screen.getAllByRole("treeitem").length).toBeLessThan(100);
  names.set("e0", "Item 00000 renamed");
  const queries = engine.called("query.run").length;
  await act(async () => {
    engine.event({ topic: "edit", method: "component.set", entities: ["e0"] });
  });
  await screen.findByRole("treeitem", { name: "Item 00000 renamed" });
  expect(engine.called("query.run").length).toBe(queries);
  expect(engine.called("entity.get").map((p) => p["id"])).toEqual(["e0"]);
});

test("scene changes and very large edits refresh everything", async () => {
  const { engine } = bigScene(10);
  const session = await EditorSession.open(new EngineClient(engine));
  render(<Outliner session={session} selected={undefined} onSelect={() => undefined} />);
  await screen.findByRole("treeitem", { name: "Item 00000" });
  const before = engine.called("query.run").length;
  await act(async () => {
    engine.event({ topic: "scene", event: "opened" });
    engine.event({ topic: "edit", entities: Array.from({ length: 500 }, (_, i) => `e${i}`) });
  });
  expect(engine.called("query.run").length).toBe(before + 2);
});
