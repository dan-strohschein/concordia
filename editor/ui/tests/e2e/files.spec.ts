// Files end to end (M1 phase 1.11): scenes from the Scene menu and the FileSystem dock, the
// unsaved-changes guard, and dragging a model into the viewport.
import { copyFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

import type { Page } from "@playwright/test";

import { expect, test, type Engine } from "./engine";

async function openEditor(page: Page, engine: Engine) {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  await expect(page.getByRole("tree", { name: "Scene" })).toBeVisible();
  await expect(page.getByRole("tree", { name: "Project files" })).toBeVisible();
}

async function sceneMenu(page: Page, item: RegExp) {
  await page.getByRole("menubar").getByRole("menuitem", { name: "Scene", exact: true }).click();
  await page.getByRole("menu", { name: "Scene" }).getByRole("menuitem", { name: item }).click();
}

async function answerPath(page: Page, title: string, path: string) {
  const dialog = page.getByRole("dialog", { name: title });
  await dialog.getByLabel(/Path/).fill(path);
  await dialog.getByRole("button", { name: /Create|Save|Open/ }).click();
  await expect(dialog).toBeHidden();
}

const entity = (page: Page, name: string) => page.getByRole("treeitem", { name, exact: true });

test("new_scene_save_as_and_reopen_from_the_filesystem_dock", async ({ page, engine }) => {
  await openEditor(page, engine);
  await sceneMenu(page, /New Scene/);
  await answerPath(page, "New Scene", "scenes/e2e.scene.ron");
  await expect(entity(page, "Ground")).toHaveCount(0);
  await engine.agent.call("entity.spawn", { components: { Name: "Made Here" } });
  await expect(entity(page, "Made Here")).toBeVisible();
  await expect.poll(() => page.title()).toBe("*e2e - Concordia");
  await sceneMenu(page, /Save Scene As/);
  await answerPath(page, "Save Scene As", "scenes/e2e_copy.scene.ron");
  await expect.poll(() => page.title()).toBe("e2e - Concordia");
  const files = page.getByRole("tree", { name: "Project files" });
  await files.getByText("stack.scene.ron", { exact: true }).dblclick();
  await expect(entity(page, "Ground")).toBeVisible();
  await files.getByText("e2e_copy.scene.ron", { exact: true }).dblclick();
  await expect(entity(page, "Made Here")).toBeVisible();
  await expect(entity(page, "Ground")).toHaveCount(0);
});

test("the_unsaved_prompt_guards_new_scene", async ({ page, engine }) => {
  await openEditor(page, engine);
  const { Ground } = await engine.idsByName();
  await engine.agent.call("component.set", { entity: Ground, component: "Name", value: "Changed Ground" });
  await expect.poll(() => page.title()).toMatch(/^\*/);
  await sceneMenu(page, /New Scene/);
  await answerPath(page, "New Scene", "scenes/fresh.scene.ron");
  const prompt = page.getByRole("dialog", { name: "Unsaved changes" });
  await prompt.getByRole("button", { name: "Cancel" }).click();
  await expect(entity(page, "Changed Ground")).toBeVisible();
  await sceneMenu(page, /New Scene/);
  await answerPath(page, "New Scene", "scenes/fresh.scene.ron");
  await prompt.getByRole("button", { name: "Don't Save" }).click();
  await expect(entity(page, "Changed Ground")).toHaveCount(0);
  await expect.poll(() => page.title()).toBe("fresh - Concordia");
});

test("dragging_a_model_into_the_viewport_instances_it", async ({ page, engine }) => {
  const fixture = resolve(
    import.meta.dirname,
    "../../../../crates/concordia-import-gltf/tests/fixtures/two_nodes.gltf",
  );
  mkdirSync(join(engine.dir, "models"), { recursive: true });
  copyFileSync(fixture, join(engine.dir, "models/two_nodes.gltf"));
  await openEditor(page, engine);
  const image = page.getByRole("img", { name: "Viewport" });
  await expect(image).toBeVisible({ timeout: 20_000 });
  const before = await image.getAttribute("src");
  await page
    .getByRole("tree", { name: "Project files" })
    .getByText("two_nodes.gltf", { exact: true })
    .dragTo(page.getByTestId("viewport"));
  await expect(entity(page, "two_nodes")).toHaveAttribute("aria-selected", "true");
  const names = await engine.idsByName();
  expect(names).toHaveProperty("Left");
  expect(names).toHaveProperty("Right");
  await expect.poll(() => image.getAttribute("src")).not.toBe(before);
  await page.getByRole("tree", { name: "Scene" }).click();
  await page.keyboard.press("ControlOrMeta+z");
  await expect(entity(page, "two_nodes")).toHaveCount(0);
});
