// The editor end to end in a browser (M1 phase 1.9): the same UI as the native shell, talking to a
// real headless engine over the WebSocket Engine API.
import type { Page } from "@playwright/test";

import { expect, test, type Engine } from "./engine";

async function openEditor(page: Page, engine: Engine) {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  await expect(page.getByRole("tree", { name: "Scene" })).toBeVisible();
}

const translationX = async (engine: Engine, id: string): Promise<number> => {
  const got = await engine.agent.call<{ value: { translation: number[] } }>("component.get", {
    entity: id,
    component: "Transform",
  });
  return got.value.translation[0] ?? NaN;
};

test("outliner_lists_scene_entities", async ({ page, engine }) => {
  await openEditor(page, engine);
  const names = Object.keys(await engine.idsByName());
  for (const name of ["Ground", "Camera", "Sun", "Pyramid 0.0", "Dropped 2"]) expect(names).toContain(name);
  for (const name of names) await expect(page.getByRole("treeitem", { name, exact: true }).first()).toBeVisible();
});

test("selecting_entity_shows_its_components_in_inspector", async ({ page, engine }) => {
  await openEditor(page, engine);
  await page.getByRole("treeitem", { name: "Pyramid 0.0", exact: true }).click();
  const inspector = page.getByRole("region", { name: "Inspector dock" });
  for (const section of ["Collider", "MeshRenderer", "Name", "RigidBody", "Transform"]) {
    await expect(inspector.getByText(section, { exact: true })).toBeVisible();
  }
  await expect(inspector.getByLabel("Translation x")).toHaveValue("-1.575");
});

test("editing_transform_x_updates_engine_and_is_logged_with_human_actor", async ({ page, engine }) => {
  await openEditor(page, engine);
  const id = (await engine.idsByName())["Dropped 0"] ?? "";
  await page.getByRole("treeitem", { name: "Dropped 0", exact: true }).click();
  const x = page.getByRole("region", { name: "Inspector dock" }).getByLabel("Translation x");
  await x.fill("4.25");
  await x.press("Enter");
  await expect.poll(() => translationX(engine, id)).toBeCloseTo(4.25);
  const log = await engine.agent.call<{ records: { message: string; fields?: Record<string, unknown> }[] }>(
    "log.tail",
    {},
  );
  expect(log.records.some((r) => JSON.stringify(r).includes("human:local"))).toBe(true);
});

test("undo_shortcut_calls_txn_undo_and_restores_value", async ({ page, engine }) => {
  await openEditor(page, engine);
  const id = (await engine.idsByName())["Dropped 1"] ?? "";
  const before = await translationX(engine, id);
  await page.getByRole("treeitem", { name: "Dropped 1", exact: true }).click();
  const x = page.getByRole("region", { name: "Inspector dock" }).getByLabel("Translation x");
  await x.fill("9");
  await x.press("Enter");
  await expect.poll(() => translationX(engine, id)).toBeCloseTo(9);
  await page.getByRole("tree", { name: "Scene" }).click();
  await page.keyboard.press("ControlOrMeta+z");
  await expect.poll(() => translationX(engine, id)).toBeCloseTo(before);
});

test("agent_edit_appears_live_in_inspector", async ({ page, engine }) => {
  await openEditor(page, engine);
  const id = (await engine.idsByName())["Dropped 2"] ?? "";
  await page.getByRole("treeitem", { name: "Dropped 2", exact: true }).click();
  const x = page.getByRole("region", { name: "Inspector dock" }).getByLabel("Translation x");
  await expect(x).toHaveValue("0.1");
  const start = Date.now();
  await engine.agent.call("component.patch", {
    entity: id,
    component: "Transform",
    patch: { translation: [-7, 6, 1.6] },
  });
  await expect(x).toHaveValue("-7", { timeout: 500 });
  expect(Date.now() - start).toBeLessThan(500);
});

test("step_button_advances_sim_and_viewport_image_changes", async ({ page, engine }) => {
  await openEditor(page, engine);
  const image = page.getByRole("img", { name: "Viewport" });
  await expect(image).toBeVisible({ timeout: 20_000 });
  const before = await image.getAttribute("src");
  for (let i = 0; i < 30; i++) await page.getByTitle("Step 1 tick").click();
  await expect.poll(async () => image.getAttribute("src"), { timeout: 20_000 }).not.toBe(before);
  const info = await engine.agent.call<{ tick: number }>("sim.pause", {});
  expect(info.tick).toBeGreaterThanOrEqual(30);
});
