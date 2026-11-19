// The Agent Activity panel end to end (M1 phase 1.15): an agent's edits appear live, and
// "Undo to here" rewinds the scene.
import { expect, test } from "./engine";

test("agent_edits_show_in_activity_and_undo_to_here_restores_the_scene", async ({ page, engine }) => {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  const panel = page.getByRole("region", { name: "Agent Activity dock" });
  await expect(panel).toBeVisible();
  const id = (await engine.idsByName())["Ground"];
  const translation = async () =>
    (
      await engine.agent.call<{ value: { translation: number[] } }>("component.get", {
        entity: id,
        component: "Transform",
      })
    ).value.translation;
  const before = await translation();

  for (const [i, x] of [3, 4, 5].entries()) {
    const { txn } = await engine.agent.call<{ txn: string }>("txn.begin", { label: `agent move ${i + 1}` });
    await engine.agent.call("component.patch", {
      entity: id,
      component: "Transform",
      patch: { translation: [x, before[1], before[2]] },
    });
    await engine.agent.call("txn.commit", { txn });
  }
  const rows = panel.getByRole("listitem");
  await expect(rows.filter({ hasText: "agent move" })).toHaveCount(3);
  await expect(rows.first()).toContainText("agent move 3");
  await expect(rows.first()).toContainText("agent:test");

  // Clicking an entry selects what it touched.
  await rows
    .filter({ hasText: "agent move 2" })
    .getByRole("button", { name: /agent move 2/ })
    .click();
  await expect(page.getByRole("treeitem", { name: "Ground", exact: true })).toHaveAttribute("aria-selected", "true");

  await rows.filter({ hasText: "agent move 1" }).getByRole("button", { name: "Undo to here" }).click();
  await expect.poll(translation).toEqual(before);
  await expect(rows.filter({ hasText: "agent move 1" }).getByRole("button", { name: "Undo to here" })).toBeDisabled();
});

test("edit_menu_undoes_and_view_menu_toggles_a_dock", async ({ page, engine }) => {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  const id = (await engine.idsByName())["Ground"];
  await engine.agent.call("component.patch", { entity: id, component: "Name", patch: "Floor" });
  await expect(page.getByRole("treeitem", { name: "Floor", exact: true })).toBeVisible();
  await page.getByRole("menubar").getByRole("menuitem", { name: "Edit", exact: true }).click();
  await page.getByRole("menu", { name: "Edit" }).getByRole("menuitem", { name: /Undo/ }).click();
  await expect(page.getByRole("treeitem", { name: "Ground", exact: true })).toBeVisible();

  await page.getByRole("menubar").getByRole("menuitem", { name: "View", exact: true }).click();
  await page
    .getByRole("menu", { name: "View" })
    .getByRole("menuitemcheckbox", { name: /Output/ })
    .click();
  await expect(page.getByRole("contentinfo", { name: "Output panel" })).toHaveCount(0);
});
