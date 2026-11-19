// The Inspector over a real EditorSession and a fake engine (M1 phase 1.9).
import { act, fireEvent, render, screen } from "@testing-library/react";

import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { Inspector } from "../../src/inspector/Inspector";
import { FakeEngine, oneEntity } from "./support/engine";

async function setup(translation = [1, 2, 3]) {
  const engine = new FakeEngine(oneEntity({ translation }));
  const session = await EditorSession.open(new EngineClient(engine));
  render(<Inspector session={session} entity="e1" />);
  const x = await screen.findByLabelText("Translation x");
  return { engine, session, x };
}

test("inspector_emits_component_set_with_actor_human_local_on_commit", async () => {
  const { engine, x } = await setup();
  fireEvent.change(x, { target: { value: "5" } });
  fireEvent.change(x, { target: { value: "5.5" } });
  expect(engine.called("component.set")).toHaveLength(0);
  await act(async () => {
    fireEvent.blur(x);
  });
  const sets = engine.called("component.set");
  expect(sets).toHaveLength(1);
  expect(sets[0]).toMatchObject({ entity: "e1", component: "Transform", actor: "human:local" });
  expect((sets[0]?.["value"] as { translation: number[] }).translation).toEqual([5.5, 2, 3]);
});

test("enter commits, escape reverts, and invalid numbers never reach the engine", async () => {
  const { engine, x } = await setup();
  fireEvent.change(x, { target: { value: "abc" } });
  await act(async () => {
    fireEvent.keyDown(x, { key: "Enter" });
  });
  expect(engine.called("component.set")).toHaveLength(0);
  expect((x as HTMLInputElement).value).toBe("1");
  fireEvent.change(x, { target: { value: "9" } });
  fireEvent.keyDown(x, { key: "Escape" });
  expect((x as HTMLInputElement).value).toBe("1");
  fireEvent.change(x, { target: { value: "7" } });
  await act(async () => {
    fireEvent.keyDown(x, { key: "Enter" });
  });
  expect(engine.called("component.set")).toHaveLength(1);
});

test("an agent's edit event refreshes the open inspector", async () => {
  const { engine } = await setup();
  const handlers = oneEntity({ translation: [42, 2, 3] });
  engine.calls = [];
  Object.assign(engine, { handlers });
  await act(async () => {
    engine.event({ topic: "edit", method: "component.set", actor: "agent:test#1", entities: ["e1"], undo: false });
  });
  expect(((await screen.findByLabelText("Translation x")) as HTMLInputElement).value).toBe("42");
  expect(engine.called("entity.get").length).toBeGreaterThan(0);
});

test("the session opens as human:local and subscribes to live changes", async () => {
  const { engine } = await setup();
  expect(engine.called("session.open")[0]).toMatchObject({ actor: "human:local" });
  expect(engine.called("events.subscribe")[0]).toEqual({ topics: ["edit", "scene", "sim", "pending", "log"] });
});

test("a burst of change events refetches the entity at most twice", async () => {
  const { engine } = await setup();
  const before = engine.called("entity.get").length;
  await act(async () => {
    for (let i = 0; i < 50; i++) engine.event({ topic: "pending", entities: ["e1"] });
    await new Promise((r) => setTimeout(r, 20));
  });
  expect(engine.called("entity.get").length - before).toBeLessThanOrEqual(2);
});
