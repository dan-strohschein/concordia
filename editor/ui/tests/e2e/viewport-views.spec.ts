// Viewport views end to end (M1 phase 1.16): a debug view mode changes the picture, E and R pick
// the rotate and scale gizmos, and the stats readout reports the frame.
import { expect, test } from "./engine";

test("view_mode_changes_the_picture_and_e_r_switch_the_gizmo", async ({ page, engine }) => {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  const image = page.getByRole("img", { name: "Viewport" });
  await expect(image).toBeVisible({ timeout: 20_000 });
  const lit = await image.getAttribute("src");

  await page.getByLabel("View", { exact: true }).selectOption("mode:wireframe");
  await expect.poll(() => image.getAttribute("src")).not.toBe(lit);
  await expect(page.getByRole("status", { name: "Render stats" })).toContainText("draws");

  await page.getByRole("treeitem", { name: "Ground", exact: true }).click();
  await page.getByRole("tree", { name: "Scene" }).focus();
  await page.keyboard.press("e");
  await expect(page.getByRole("button", { name: "Rotate", exact: true })).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("r");
  await expect(page.getByRole("button", { name: "Scale", exact: true })).toHaveAttribute("aria-pressed", "true");
});
