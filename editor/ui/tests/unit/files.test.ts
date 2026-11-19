// Files (M1 phase 1.11): the FileSystem tree, recent scenes, the window title, scene commands with
// the unsaved-changes guard, and placing a dropped model.
import { vi } from "vitest";

import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { type DropPoint, placeAsset } from "../../src/files/place";
import { RecentScenes } from "../../src/files/recent";
import { SceneFiles, type UnsavedChoice } from "../../src/files/scenes";
import { windowTitle } from "../../src/files/title";
import { buildTree, type ProjectFile } from "../../src/files/tree";
import { FakeEngine, type Handler, oneEntity } from "./support/engine";

const file = (path: string, kind: ProjectFile["kind"] = "other", asset: string | null = null): ProjectFile => ({
  path,
  kind,
  size: 1,
  asset,
  importer: kind === "model" ? "gltf" : null,
});

test("the tree nests folders before files, each sorted by name", () => {
  const root = buildTree([
    file("scenes/b.scene.ron", "scene"),
    file("concordia.toml"),
    file("models/props/crate.glb", "model"),
    file("scenes/a.scene.ron", "scene"),
    file("models/tree.gltf", "model"),
  ]);
  expect(root.name).toBe("res://");
  expect(root.folders.map((f) => f.name)).toEqual(["models", "scenes"]);
  expect(root.files.map((f) => f.path)).toEqual(["concordia.toml"]);
  const [models, scenes] = root.folders;
  expect(models?.path).toBe("models");
  expect(models?.folders.map((f) => f.path)).toEqual(["models/props"]);
  expect(models?.files.map((f) => f.path)).toEqual(["models/tree.gltf"]);
  expect(scenes?.files.map((f) => f.path)).toEqual(["scenes/a.scene.ron", "scenes/b.scene.ron"]);
});

class MemoryStore {
  data = new Map<string, string>();
  getItem(k: string) {
    return this.data.get(k) ?? null;
  }
  setItem(k: string, v: string) {
    this.data.set(k, v);
  }
}

test("recent scenes put the newest first, drop duplicates and keep ten", () => {
  const store = new MemoryStore();
  const recent = new RecentScenes(store);
  for (let i = 0; i < 12; i++) recent.add(`scenes/${i}.scene.ron`);
  recent.add("scenes/5.scene.ron");
  const list = new RecentScenes(store).list();
  expect(list).toHaveLength(10);
  expect(list[0]).toBe("scenes/5.scene.ron");
  expect(list.filter((p) => p === "scenes/5.scene.ron")).toHaveLength(1);
});

test("recent scenes survive unusable storage", () => {
  const broken = {
    getItem: () => {
      throw new Error("denied");
    },
    setItem: () => {
      throw new Error("denied");
    },
  };
  const recent = new RecentScenes(broken);
  recent.add("scenes/a.scene.ron");
  expect(recent.list()).toEqual(["scenes/a.scene.ron"]);
  const garbage = new MemoryStore();
  garbage.setItem("concordia.recentScenes", "{not json");
  expect(new RecentScenes(garbage).list()).toEqual([]);
  expect(new RecentScenes(undefined).list()).toEqual([]);
});

test("the window title marks unsaved changes", () => {
  expect(windowTitle({ name: "Stack", path: "scenes/stack.scene.ron", dirty: false })).toBe("Stack - Concordia");
  expect(windowTitle({ name: "Stack", path: "scenes/stack.scene.ron", dirty: true })).toBe("*Stack - Concordia");
  expect(windowTitle(undefined)).toBe("Concordia");
});

async function scenes(dirty: boolean, choice: UnsavedChoice, extra: Record<string, Handler> = {}) {
  const engine = new FakeEngine({
    ...oneEntity({ translation: [0, 0, 0] }),
    "scene.info": () => ({ scene_id: "s", name: "Stack", path: "scenes/stack.scene.ron", dirty, entity_count: 1 }),
    "scene.save": (p) => ({ path: p["path"] ?? "scenes/stack.scene.ron", bytes: 1, hash: "h" }),
    "scene.open": (p) => ({ scene_id: "s2", entity_count: 0, history: "none", path: p["path"] }),
    "scene.new": (p) => ({ scene_id: "s3", path: p["path"] }),
    ...extra,
  });
  const ask = vi.fn(async () => choice);
  const recent = new RecentScenes(new MemoryStore());
  const files = new SceneFiles(await EditorSession.open(new EngineClient(engine)), recent, ask);
  return { engine, ask, recent, files };
}

test("opening over unsaved changes asks first; save saves, then opens", async () => {
  const { engine, ask, recent, files } = await scenes(true, "save");
  expect(await files.open("scenes/other.scene.ron")).toBe(true);
  expect(ask).toHaveBeenCalledWith("Stack");
  expect(engine.calls.map((c) => c.method).filter((m) => m.startsWith("scene.") && m !== "scene.info")).toEqual([
    "scene.save",
    "scene.open",
  ]);
  expect(engine.called("scene.open")[0]).toMatchObject({ path: "scenes/other.scene.ron", discard: false });
  expect(recent.list()).toEqual(["scenes/other.scene.ron"]);
});

test("don't save discards the changes; cancel does nothing", async () => {
  const discard = await scenes(true, "discard");
  expect(await discard.files.newScene("scenes/new.scene.ron", "New")).toBe(true);
  expect(discard.engine.called("scene.save")).toHaveLength(0);
  expect(discard.engine.called("scene.new")[0]).toMatchObject({
    path: "scenes/new.scene.ron",
    name: "New",
    discard: true,
    actor: "human:local",
  });
  const cancel = await scenes(true, "cancel");
  expect(await cancel.files.open("scenes/other.scene.ron")).toBe(false);
  expect(cancel.engine.called("scene.open")).toHaveLength(0);
});

test("a clean scene opens without asking; save as saves to the new path", async () => {
  const { engine, ask, recent, files } = await scenes(false, "cancel");
  expect(await files.open("scenes/other.scene.ron")).toBe(true);
  expect(ask).not.toHaveBeenCalled();
  await files.saveAs("scenes/copy.scene.ron");
  expect(engine.called("scene.save").at(-1)).toMatchObject({ path: "scenes/copy.scene.ron" });
  expect(recent.list()[0]).toBe("scenes/copy.scene.ron");
});

test("with no scene open nothing is guarded", async () => {
  const { ask, files } = await scenes(false, "cancel", {
    "scene.info": () => {
      throw new Error("no scene");
    },
  });
  expect(await files.info()).toBeUndefined();
  expect(await files.open("scenes/a.scene.ron")).toBe(true);
  expect(ask).not.toHaveBeenCalled();
});

const at: DropPoint = {
  view: { eye: [0, 5, 5], target: [0, 0, 0], projection: { Perspective: { fov_y: 0.9, near: 0.1, far: 100 } } },
  x: 10,
  y: 20,
  width: 100,
  height: 80,
};

async function placing() {
  const engine = new FakeEngine({
    ...oneEntity({ translation: [0, 0, 0] }),
    "asset.import": () => ({ id: "a-new", cached: false, sub_assets: [] }),
    "viewport.drop": () => ({ position: [1, 0, 2], entity: null }),
    "prefab.instantiate": () => ({ root: "r1", entities: ["r1", "c1"] }),
  });
  return { engine, session: await EditorSession.open(new EngineClient(engine)) };
}

test("dropping an unimported model imports it, then instantiates it where it lands", async () => {
  const { engine, session } = await placing();
  expect(await placeAsset(session, file("models/tree.gltf", "model"), at)).toBe("r1");
  expect(engine.called("asset.import")[0]).toMatchObject({ path: "models/tree.gltf", actor: "human:local" });
  expect(engine.called("viewport.drop")[0]).toEqual({ view: at.view, x: 10, y: 20, width: 100, height: 80 });
  expect(engine.called("prefab.instantiate")[0]).toEqual({
    asset: "asset:a-new",
    translation: [1, 0, 2],
    actor: "human:local",
  });
});

test("an imported model is not re-imported; other files cannot be placed", async () => {
  const { engine, session } = await placing();
  await placeAsset(session, file("models/tree.gltf", "model", "a-old"), at);
  expect(engine.called("asset.import")).toHaveLength(0);
  expect(engine.called("prefab.instantiate")[0]).toMatchObject({ asset: "asset:a-old" });
  await expect(placeAsset(session, file("notes.txt"), at)).rejects.toThrow(/model/);
});
