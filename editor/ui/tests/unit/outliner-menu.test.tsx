// The Scene dock's right-click menu (M1 phase 1.13): add a template, rename, duplicate and delete,
// each one Engine API call (entity.create / entity.duplicate / entity.despawn).
import { act, fireEvent, render, screen } from "@testing-library/react";
import { vi } from "vitest";

import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { Outliner } from "../../src/outliner/Outliner";
import { FakeEngine, oneEntity } from "./support/engine";

async function scene() {
  const engine = new FakeEngine({
    ...oneEntity({ translation: [0, 0, 0] }),
    "entity.templates": () => ({
      templates: [
        { name: "Cube", description: "A cube.", components: ["MeshRenderer"] },
        { name: "Point Light", description: "A bulb.", components: ["PointLight"] },
      ],
    }),
    "entity.create": () => ({ id: "new1" }),
    "entity.duplicate": () => ({ ids: ["copy1"] }),
    "entity.despawn": () => ({}),
  });
  const session = await EditorSession.open(new EngineClient(engine));
  const onSelect = vi.fn();
  render(<Outliner session={session} selected="e1" onSelect={onSelect} />);
  await screen.findByText("Crate");
  return { engine, onSelect };
}

const choose = async (name: RegExp) => {
  await act(async () => {
    fireEvent.click(await screen.findByRole("menuitem", { name }));
  });
};

test("right-clicking an entity offers templates as children, and creating selects the new entity", async () => {
  const { engine, onSelect } = await scene();
  fireEvent.contextMenu(screen.getByText("Crate"));
  expect(await screen.findByRole("menu", { name: "Entity" })).toBeTruthy();
  for (const item of [/Rename/, /Duplicate/, /Delete/])
    expect(screen.getByRole("menuitem", { name: item })).toBeTruthy();
  await choose(/Add Point Light/);
  expect(engine.called("entity.create")[0]).toEqual({ template: "Point Light", parent: "e1", actor: "human:local" });
  expect(onSelect).toHaveBeenLastCalledWith("new1");
  expect(screen.queryByRole("menu")).toBeNull();
});

test("right-clicking empty space adds top-level entities and offers no entity actions", async () => {
  const { engine } = await scene();
  fireEvent.contextMenu(screen.getByTestId("outliner-scroll"));
  await screen.findByRole("menu", { name: "Entity" });
  expect(screen.queryByRole("menuitem", { name: /Delete/ })).toBeNull();
  await choose(/Add Cube/);
  expect(engine.called("entity.create")[0]).toEqual({ template: "Cube", actor: "human:local" });
});

test("duplicate and delete act on the whole subtree", async () => {
  const { engine, onSelect } = await scene();
  fireEvent.contextMenu(screen.getByText("Crate"));
  await choose(/Duplicate/);
  expect(engine.called("entity.duplicate")[0]).toEqual({ ids: ["e1"], actor: "human:local" });
  expect(onSelect).toHaveBeenLastCalledWith("copy1");
  fireEvent.contextMenu(screen.getByText("Crate"));
  await choose(/Delete/);
  expect(engine.called("entity.despawn")[0]).toEqual({ id: "e1", recursive: true, actor: "human:local" });
});

test("⌘D duplicates and Delete removes the subtree from the keyboard", async () => {
  const { engine } = await scene();
  const tree = screen.getByRole("tree", { name: "Scene" });
  await act(async () => {
    fireEvent.keyDown(tree, { key: "d", metaKey: true });
  });
  expect(engine.called("entity.duplicate")).toHaveLength(1);
  await act(async () => {
    fireEvent.keyDown(tree, { key: "Delete" });
  });
  expect(engine.called("entity.despawn")[0]).toMatchObject({ id: "e1", recursive: true });
});
