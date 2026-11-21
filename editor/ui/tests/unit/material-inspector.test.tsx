// The Inspector showing a .material.ron (M2 phase 2.3c): struct-variant enums, named-node maps,
// and edits that stay a draft until the engine accepts the material.
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { vi } from "vitest";

import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { FileSystem } from "../../src/files/FileSystem";
import { Inspector } from "../../src/inspector/Inspector";
import { FakeEngine, RpcFailure, oneEntity, schema } from "./support/engine";

const PATH = "materials/paint.material.ron";

type Json = Record<string, unknown>;
type Nodes = Record<string, Record<string, Json>>;

const carPaint = (): Json => ({
  nodes: {
    paint: { Slab: { base_color: [0.6, 0.1, 0.05, 1], roughness: 0.5, haze: null } },
    coat: { Slab: { base_color: [0, 0, 0, 1], roughness: 0.15, transmittance: [1, 1, 1], haze: null } },
    coated: { Vertical: { top: "coat", bottom: "paint" } },
  },
  root: "coated",
  alpha_mode: "Opaque",
  double_sided: false,
  unlit: false,
});

/** The engine's rules, enough for these tests: references exist and every node is used. */
function invalid(material: Json): string | undefined {
  const nodes = material["nodes"] as Nodes;
  const children = (n: string) => Object.values(nodes[n]?.["Vertical"] ?? {}) as string[];
  for (const name of Object.keys(nodes)) {
    const missing = children(name).find((c) => !(c in nodes));
    if (missing !== undefined) return `node "${name}" refers to "${missing}", which doesn't exist`;
  }
  const seen = new Set<string>();
  const visit = (n: string) => {
    if (seen.has(n)) return;
    seen.add(n);
    children(n).forEach(visit);
  };
  visit(String(material["root"]));
  const unused = Object.keys(nodes).find((n) => !seen.has(n));
  return unused === undefined ? undefined : `node "${unused}" is not used by the root "${String(material["root"])}"`;
}

async function setup(opts: { migrated?: boolean } = {}) {
  let saved = carPaint();
  const engine = new FakeEngine({
    ...oneEntity({ translation: [0, 0, 0] }),
    "schema.get": (p) => ({ schema: schema(String(p["name"])) }),
    "material.get": () => ({ material: saved, migrated: opts.migrated ?? false, id: "a1" }),
    "material.set": (p) => {
      const material = p["material"] as Json;
      const reason = invalid(material);
      if (reason !== undefined) throw new RpcFailure(-32602, `material: ${reason}`, { kind: "InvalidParams" });
      saved = material;
      return { id: "a1", written: true };
    },
  });
  const session = await EditorSession.open(new EngineClient(engine));
  render(<Inspector session={session} entity={undefined} material={PATH} />);
  await screen.findByRole("group", { name: "coat" });
  return { engine, session };
}

const node = (name: string) => screen.getByRole("group", { name });
const lastSet = (engine: FakeEngine) => engine.called("material.set").at(-1) as Json;

async function commit(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } });
  await act(async () => {
    fireEvent.blur(input);
  });
}

test("editing a slab field writes the whole material with material.set as human:local", async () => {
  const { engine } = await setup();
  expect(screen.getByText(PATH)).toBeTruthy();
  await commit(within(node("coat")).getByLabelText("Roughness"), "0.3");
  const set = lastSet(engine);
  expect(set).toMatchObject({ path: PATH, actor: "human:local" });
  const nodes = (set["material"] as Json)["nodes"] as Nodes;
  expect(nodes["coat"]?.["Slab"]?.["roughness"]).toBe(0.3);
  expect(nodes["paint"]).toEqual(carPaint()["nodes"] && (carPaint()["nodes"] as Nodes)["paint"]);
  expect(screen.queryByRole("alert")).toBeNull();
});

test("struct_variant_enum_widget switches variants and edits the chosen variant's fields", async () => {
  const { engine } = await setup();
  await act(async () => {
    fireEvent.change(screen.getByRole("combobox", { name: "Alpha Mode" }), { target: { value: "Mask" } });
  });
  expect((lastSet(engine)["material"] as Json)["alpha_mode"]).toEqual({ Mask: { cutoff: 0 } });
  await commit(await screen.findByLabelText("Cutoff"), "0.5");
  expect((lastSet(engine)["material"] as Json)["alpha_mode"]).toEqual({ Mask: { cutoff: 0.5 } });
  // A node's kind is a variant too: its Vertical fields are shown.
  expect(within(node("coated")).getByRole("combobox", { name: "coated kind" })).toHaveProperty("value", "Vertical");
  expect(within(node("coated")).getByLabelText("Top")).toHaveProperty("value", "coat");
});

test("an empty optional feature can be added with its required fields", async () => {
  const { engine } = await setup();
  await act(async () => {
    fireEvent.click(within(node("paint")).getByRole("button", { name: "Add Haze" }));
  });
  const nodes = (lastSet(engine)["material"] as Json)["nodes"] as Nodes;
  expect(nodes["paint"]?.["Slab"]?.["haze"]).toEqual({ roughness: 0, weight: 0 });
});

test("named_node_map_widget renames a node: the draft stays until the material is valid", async () => {
  const { engine } = await setup();
  await commit(screen.getByLabelText("Rename coat"), "lacquer");
  // Refused: "coated" still refers to "coat". The draft and the engine's reason stay on screen.
  expect((await screen.findByRole("alert")).textContent).toContain('refers to "coat"');
  expect(node("lacquer")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Revert" })).toBeTruthy();
  await commit(within(node("coated")).getByLabelText("Top"), "lacquer");
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.queryByRole("button", { name: "Revert" })).toBeNull();
  const nodes = (lastSet(engine)["material"] as Json)["nodes"] as Nodes;
  expect(Object.keys(nodes).sort()).toEqual(["coated", "lacquer", "paint"]);
  expect(nodes["coated"]?.["Vertical"]?.["top"]).toBe("lacquer");
});

test("adding and removing nodes goes through the draft; revert restores the saved material", async () => {
  const { engine } = await setup();
  await commit(screen.getByLabelText("New node name"), "dust");
  expect((await screen.findByRole("alert")).textContent).toContain('"dust" is not used');
  expect(within(node("dust")).getByRole("combobox", { name: "dust kind" })).toHaveProperty("value", "Slab");
  const calls = engine.called("material.set").length;
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Revert" }));
  });
  expect(screen.queryByRole("group", { name: "dust" })).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(engine.called("material.set")).toHaveLength(calls);

  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Remove paint" }));
  });
  expect((await screen.findByRole("alert")).textContent).toContain('refers to "paint"');
});

test("an asset event for the shown material reloads it; other paths are ignored", async () => {
  const { engine } = await setup();
  const before = engine.called("material.get").length;
  await act(async () => {
    engine.event({ topic: "asset", path: "materials/other.material.ron", id: "a2", actor: "agent:t#1" });
  });
  expect(engine.called("material.get")).toHaveLength(before);
  await act(async () => {
    engine.event({ topic: "asset", path: PATH, id: "a1", actor: "agent:tech-artist#1" });
  });
  expect(engine.called("material.get").length).toBeGreaterThan(before);
});

test("an M1 file says saving rewrites it as a layered material", async () => {
  await setup({ migrated: true });
  expect(screen.getByText(/M1 material/)).toBeTruthy();
});

test("clicking a material in the FileSystem dock selects it for the Inspector", async () => {
  const engine = new FakeEngine({
    ...oneEntity({ translation: [0, 0, 0] }),
    "project.files": () => ({
      files: [
        { path: PATH, kind: "material", size: 10, asset: "a1", importer: "material" },
        { path: "scenes/a.scene.ron", kind: "scene", size: 10, asset: null, importer: null },
      ],
    }),
  });
  const session = await EditorSession.open(new EngineClient(engine));
  const onSelectMaterial = vi.fn();
  render(<FileSystem session={session} onOpenScene={vi.fn()} onSelectMaterial={onSelectMaterial} selected={PATH} />);
  fireEvent.click(await screen.findByText("paint.material.ron"));
  expect(onSelectMaterial).toHaveBeenCalledWith(PATH);
  fireEvent.click(screen.getByText("a.scene.ron"));
  expect(onSelectMaterial).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("treeitem", { name: "paint.material.ron" }).getAttribute("aria-selected")).toBe("true");
  // A material written by an agent appears without a scene event.
  const listed = engine.called("project.files").length;
  await act(async () => {
    engine.event({ topic: "asset", path: "materials/new.material.ron", id: "a3", actor: "agent:t#1" });
  });
  expect(engine.called("project.files").length).toBeGreaterThan(listed);
});
