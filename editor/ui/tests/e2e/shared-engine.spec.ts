// One engine per project (M3 phase 3.5b, ADR-0082): `concordia mcp` attaches to the engine the
// editor uses, so an MCP edit shows up live in Agent Activity.
import { expect, test } from "./engine";
import { McpStdio } from "./mcp";

test("editor_and_mcp_share_one_command_log", async ({ page, engine }) => {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  const panel = page.getByRole("region", { name: "Agent Activity dock" });
  await expect(panel).toBeVisible();
  const id = (await engine.idsByName())["Ground"];

  const mcp = new McpStdio(engine.dir);
  try {
    await mcp.initialize();
    const { txn } = await mcp.tool("txn_begin", { label: "mcp move" });
    await mcp.tool("component_patch", { entity: id, component: "Transform", patch: { translation: [7, 0, 0] } });
    await mcp.tool("txn_commit", { txn });
  } finally {
    mcp.close();
  }

  const row = panel.getByRole("listitem").filter({ hasText: "mcp move" });
  await expect(row).toHaveCount(1);
  await expect(row).toContainText("agent:claude-code");
});
