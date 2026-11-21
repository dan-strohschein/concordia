// Editing a material in the Inspector end to end (M2 phase 2.3c), against a real headless engine:
// pick it in the FileSystem dock, change a slab, see the file rewritten, and follow an agent's edit.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./engine";

const PATH = "materials/red_painted.material.ron";

type Material = { nodes: Record<string, { Slab?: Record<string, unknown> }> };

test("edit_a_slab_from_the_inspector_and_follow_an_agents_edit", async ({ page, engine }) => {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  await page.getByRole("treeitem", { name: "red_painted.material.ron" }).click();
  const inspector = page.getByRole("region", { name: "Inspector dock" });
  await expect(inspector.getByText(PATH)).toBeVisible();
  await expect(inspector.getByText(/An M1 material/)).toBeVisible();
  const surface = inspector.getByRole("group", { name: "surface" });
  const roughness = surface.getByLabel("Roughness", { exact: true });
  await expect(roughness).toHaveValue("0.6");

  await roughness.fill("0.25");
  await roughness.press("Enter");
  await expect(inspector.getByText(/An M1 material/)).toHaveCount(0);
  const read = await engine.agent.call<{ material: Material; migrated: boolean }>("material.get", { path: PATH });
  expect(read.migrated).toBe(false);
  expect(read.material.nodes["surface"]?.Slab?.["roughness"]).toBeCloseTo(0.25, 6);
  const text = readFileSync(join(engine.dir, PATH), "utf8");
  expect(text).toContain('"surface": Slab(');
  const log = await engine.agent.call<{ records: { message: string; fields: Record<string, string> }[] }>("log.tail", {
    level: "info",
  });
  expect(log.records.find((r) => r.message === "material saved")?.fields["actor"]).toBe("human:local");

  // An invalid edit is refused by the engine and stays as a draft until reverted.
  await inspector.getByLabel("Rename surface").fill("paint");
  await inspector.getByLabel("Rename surface").press("Enter");
  await expect(inspector.getByRole("alert")).toContainText("surface");
  await inspector.getByRole("button", { name: "Revert" }).click();
  await expect(inspector.getByRole("alert")).toHaveCount(0);
  await expect(surface).toBeVisible();

  // An agent's edit shows up in the open Inspector.
  const agentEdit = structuredClone(read.material);
  const slab = agentEdit.nodes["surface"]?.Slab;
  if (slab) slab["metallic"] = 1;
  await engine.agent.call("material.set", { path: PATH, material: agentEdit });
  await expect(surface.getByLabel("Metallic", { exact: true })).toHaveValue("1");
});
