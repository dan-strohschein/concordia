// The Scene dock end to end (M1 phase 1.13): add from the right-click menu, duplicate, delete, and
// undo each, against a real headless engine.
import type { Page } from "@playwright/test";

import { expect, test, type Engine } from "./engine";

async function openEditor(page: Page, engine: Engine) {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  await expect(page.getByRole("tree", { name: "Scene" })).toBeVisible();
}

const entity = (page: Page, name: string) => page.getByRole("treeitem", { name, exact: true });

async function menu(page: Page, on: string, item: RegExp) {
  await entity(page, on).click({ button: "right" });
  await page.getByRole("menu", { name: "Entity" }).getByRole("menuitem", { name: item }).click();
}

const undo = async (page: Page) => {
  await page.getByRole("tree", { name: "Scene" }).focus();
  await page.keyboard.press("ControlOrMeta+z");
};

test("create_duplicate_delete_from_the_scene_dock_and_undo_each", async ({ page, engine }) => {
  await openEditor(page, engine);
  await menu(page, "Ground", /Add Point Light/);
  await expect(entity(page, "Point Light")).toHaveAttribute("aria-selected", "true");
  const parent = await engine.agent.call<{ value: string }>("component.get", {
    entity: (await engine.idsByName())["Point Light"],
    component: "Parent",
  });
  expect(parent.value).toBe((await engine.idsByName())["Ground"]);

  await menu(page, "Point Light", /Duplicate/);
  await expect(entity(page, "Point Light 2")).toBeVisible();
  await undo(page);
  await expect(entity(page, "Point Light 2")).toHaveCount(0);

  await menu(page, "Ground", /Delete/);
  await expect(entity(page, "Ground")).toHaveCount(0);
  await expect(entity(page, "Point Light")).toHaveCount(0, { timeout: 5_000 });
  await undo(page);
  await expect(entity(page, "Ground")).toBeVisible();
  await expect(entity(page, "Point Light")).toBeVisible();

  await undo(page);
  await expect(entity(page, "Point Light")).toHaveCount(0);
});
