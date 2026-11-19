// Files UI (M1 phase 1.11): the menu bar, the FileSystem dock and the unsaved-changes dialog.
import { act, fireEvent, render, screen } from "@testing-library/react";
import { vi } from "vitest";

import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { FileSystem, FILE_DRAG_TYPE } from "../../src/files/FileSystem";
import { UnsavedDialog } from "../../src/files/dialogs";
import { MenuBar, type Menu } from "../../src/shell/MenuBar";
import { FakeEngine, oneEntity } from "./support/engine";

test("a menu opens on click, runs an enabled item and closes; disabled items do nothing", () => {
  const save = vi.fn();
  const quit = vi.fn();
  const menus: Menu[] = [
    {
      label: "Scene",
      items: [
        { label: "Save Scene", shortcut: "⌘S", run: save },
        "separator",
        { label: "Quit", run: quit, enabled: false },
      ],
    },
    { label: "Help", items: [] },
  ];
  render(<MenuBar menus={menus} />);
  expect(screen.queryByRole("menu")).toBeNull();
  fireEvent.click(screen.getByRole("menuitem", { name: "Scene" }));
  expect(screen.getByRole("menu", { name: "Scene" })).toBeTruthy();
  expect(screen.getByText("⌘S")).toBeTruthy();
  fireEvent.click(screen.getByRole("menuitem", { name: /Quit/ }));
  expect(quit).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("menuitem", { name: /Save Scene/ }));
  expect(save).toHaveBeenCalledOnce();
  expect(screen.queryByRole("menu")).toBeNull();
  fireEvent.click(screen.getByRole("menuitem", { name: "Scene" }));
  fireEvent.keyDown(window, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();
});

const listing = {
  files: [
    { path: "concordia.toml", kind: "other", size: 10, asset: null, importer: null },
    { path: "models/tree.gltf", kind: "model", size: 99, asset: null, importer: "gltf" },
    { path: "scenes/stack.scene.ron", kind: "scene", size: 50, asset: null, importer: null },
  ],
};

async function dock() {
  const engine = new FakeEngine({
    ...oneEntity({ translation: [0, 0, 0] }),
    "project.files": () => listing,
    "asset.import": () => ({ id: "a1", cached: false, sub_assets: [] }),
  });
  const session = await EditorSession.open(new EngineClient(engine));
  const onOpen = vi.fn();
  render(<FileSystem session={session} onOpenScene={onOpen} />);
  await screen.findByText("stack.scene.ron");
  return { engine, onOpen };
}

test("the FileSystem dock lists project files; double-clicking a scene opens it", async () => {
  const { onOpen } = await dock();
  expect(screen.getByText("res://")).toBeTruthy();
  expect(screen.getByText("models")).toBeTruthy();
  fireEvent.doubleClick(screen.getByText("stack.scene.ron"));
  expect(onOpen).toHaveBeenCalledWith("scenes/stack.scene.ron");
});

test("unimported files offer Import, which imports and refreshes", async () => {
  const { engine } = await dock();
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Import tree.gltf" }));
  });
  expect(engine.called("asset.import")[0]).toMatchObject({ path: "models/tree.gltf", actor: "human:local" });
  expect(engine.called("project.files").length).toBeGreaterThanOrEqual(2);
  expect(screen.queryByRole("button", { name: "Import concordia.toml" })).toBeNull();
});

test("models can be dragged out of the dock; the drag carries the file", async () => {
  await dock();
  const data = new Map<string, string>();
  const dataTransfer = { setData: (t: string, v: string) => data.set(t, v), effectAllowed: "" };
  fireEvent.dragStart(screen.getByText("tree.gltf"), { dataTransfer });
  expect(JSON.parse(data.get(FILE_DRAG_TYPE) ?? "null")).toMatchObject({ path: "models/tree.gltf", kind: "model" });
  expect(screen.getByText("stack.scene.ron").closest("[draggable=true]")).toBeNull();
});

test("the FileSystem dock refreshes when a scene is saved", async () => {
  const { engine } = await dock();
  const before = engine.called("project.files").length;
  await act(async () => {
    engine.event({ topic: "scene", event: "saved", path: "scenes/new.scene.ron" });
    await new Promise((r) => setTimeout(r, 0));
  });
  expect(engine.called("project.files").length).toBe(before + 1);
});

test("the unsaved-changes dialog offers Save, Don't Save and Cancel", () => {
  const choose = vi.fn();
  render(<UnsavedDialog scene="Stack" onChoose={choose} />);
  expect(screen.getByRole("dialog").textContent).toMatch(/Save changes to 'Stack'/);
  fireEvent.click(screen.getByRole("button", { name: "Don't Save" }));
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
  expect(choose.mock.calls.map((c) => c[0])).toEqual(["discard", "save", "cancel"]);
});
