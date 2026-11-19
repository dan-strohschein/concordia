// Gizmo drags through the engine (M1 phase 1.9g): the engine finds the handle under a press and
// moves the entity; the page only forwards the pointer.
import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { GizmoDrag } from "../../src/viewport/gizmoDrag";
import { FakeEngine, oneEntity } from "./support/engine";

async function setup(axis: string | null) {
  const engine = new FakeEngine({
    ...oneEntity({ translation: [0, 0, 0] }),
    "viewport.gizmo": (p) =>
      p["action"] === "begin" ? { axis, translation: axis ? [0, 0, 0] : null } : { axis: null, translation: [1, 0, 0] },
  });
  const session = await EditorSession.open(new EngineClient(engine));
  return { engine, drag: new GizmoDrag(session, "human:local") };
}

test("a press on a handle starts a drag whose moves end in one commit", async () => {
  const { engine, drag } = await setup("x");
  expect(await drag.start(10, 20, { width: 400, height: 300 })).toBe(true);
  for (let i = 0; i < 20; i++) drag.move(10 + i, 20);
  await drag.end();
  const calls = engine.called("viewport.gizmo");
  expect(calls[0]).toMatchObject({ action: "begin", x: 10, y: 20, width: 400, height: 300, actor: "human:local" });
  expect(calls.at(-1)).toMatchObject({ action: "end" });
  const moves = calls.filter((c) => c["action"] === "move");
  expect(moves.length).toBeGreaterThan(0);
  expect(moves.length).toBeLessThan(20);
  expect(moves.at(-1)).toMatchObject({ x: 29, y: 20 });
});

test("a press that misses every handle starts nothing", async () => {
  const { engine, drag } = await setup(null);
  expect(await drag.start(5, 5, { width: 400, height: 300 })).toBe(false);
  drag.move(50, 50);
  await drag.end();
  expect(engine.called("viewport.gizmo").map((c) => c["action"])).toEqual(["begin"]);
});

test("release sends the last move and the commit at once, so a following undo lands after them", async () => {
  const { engine, drag } = await setup("x");
  await drag.start(10, 20, { width: 400, height: 300 });
  drag.move(11, 20); // in flight
  drag.move(12, 20); // pending behind it
  void drag.end();
  // An undo issued right after the release (the next keypress) must reach the engine after the commit.
  void engine.send(JSON.stringify({ jsonrpc: "2.0", id: 999, method: "txn.undo", params: {} }));
  const order = engine.calls.map((c) => (c.method === "viewport.gizmo" ? String(c.params["action"]) : c.method));
  const tail = order.slice(order.indexOf("begin"));
  expect(tail).toEqual(["begin", "move", "move", "end", "txn.undo"]);
  expect(
    engine
      .called("viewport.gizmo")
      .filter((c) => c["action"] === "move")
      .at(-1),
  ).toMatchObject({ x: 12 });
});
