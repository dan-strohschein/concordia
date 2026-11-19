// The Inspector's Add/Remove Component end to end (M1 phase 1.14), against a real headless engine.
import { expect, test } from "./engine";

test("add_a_point_light_from_the_inspector_remove_it_and_undo", async ({ page, engine }) => {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  await page.getByRole("treeitem", { name: "Ground", exact: true }).click();
  const inspector = page.getByRole("region", { name: "Inspector dock" });
  const section = inspector.getByRole("group", { name: "PointLight" });
  await expect(inspector.getByLabel("Translation x")).toBeVisible();
  await expect(section).toHaveCount(0);

  await inspector.getByRole("button", { name: "Add Component" }).click();
  await inspector.getByRole("searchbox", { name: "Search components" }).fill("point");
  await inspector.getByRole("option", { name: /PointLight/ }).click();
  await expect(section).toBeVisible();
  const id = (await engine.idsByName())["Ground"];
  const light = await engine.agent.call<{ value: { intensity_lumens: number } }>("component.get", {
    entity: id,
    component: "PointLight",
  });
  expect(light.value.intensity_lumens).toBeGreaterThan(0);

  await inspector.getByRole("button", { name: "Remove PointLight" }).click();
  await expect(section).toHaveCount(0);

  await page.getByRole("tree", { name: "Scene" }).focus();
  await page.keyboard.press("ControlOrMeta+z");
  await expect(section).toBeVisible();
});
