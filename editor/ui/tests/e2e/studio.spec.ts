// The Studio panel end to end (M3 phase 3.8, ADR-0083): task changes made through the real
// `concordia mcp` (stamped as the Studio hook would) show up live on the board and in the command
// log, and a human approval in the panel moves a task to done as `human:local`.
import { expect, test } from "./engine";
import { McpStdio } from "./mcp";

test.use({ studio: true });

const live = { timeout: process.env["CI"] ? 15_000 : 1_000 };

/** The `_studio` stamp the PreToolUse hook adds (ADR-0080). */
let toolUse = 0;
const stamp = (role: string, agent: string) => ({ _studio: { role, agent, tool_use_id: `tu-${++toolUse}` } });

test("studio_panel_reflects_task_updates_made_over_mcp_within_one_second", async ({ page, engine }) => {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  const board = page.getByRole("region", { name: "Task board" });
  await expect(board).toBeVisible();

  const mcp = new McpStdio(engine.dir);
  try {
    await mcp.initialize();
    const created = await mcp.tool("task_create", {
      title: "Arena floor",
      role: "level-designer",
      ...stamp("director", "s1/main"),
    });
    const id = (created["task"] as { id: string }).id;
    await expect(board.getByRole("list", { name: "todo" }).getByRole("listitem", { name: new RegExp(id) })).toBeVisible(
      live,
    );

    await mcp.tool("task_claim", { id, ...stamp("level-designer", "s1/a1") });
    const card = board.getByRole("list", { name: "in_progress" }).getByRole("listitem", { name: new RegExp(id) });
    await expect(card).toBeVisible(live);
    await expect(card).toContainText("agent:level-designer#1");

    const log = page.getByRole("region", { name: "Agent Activity dock" });
    await expect(log.getByRole("listitem").filter({ hasText: `created ${id}` })).toContainText(
      "agent:director#1",
      live,
    );
    await expect(log.getByLabel("role level-designer").first()).toBeVisible(live);
  } finally {
    mcp.close();
  }
});

test("approve_in_panel_moves_task_to_done_and_logs_human_actor", async ({ page, engine }) => {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  await expect(page.getByRole("region", { name: "Studio panel" })).toBeVisible();

  const { task } = await engine.agent.call<{ task: { id: string } }>("task.create", {
    title: "Arena walls",
    role: "level-designer",
    actor: "agent:director#1",
  });
  await engine.agent.call("task.claim", { id: task.id, actor: "agent:level-designer#1" });
  await engine.agent.call("task.update", { id: task.id, status: "in_review", actor: "agent:level-designer#1" });

  await page.getByRole("tab", { name: "Approvals (1)" }).click({ timeout: live.timeout });
  const pending = page.getByRole("list", { name: "Pending approvals" }).getByRole("listitem");
  await pending.getByRole("textbox", { name: `Comment on ${task.id}` }).fill("walls checked");
  await pending.getByRole("button", { name: "Approve" }).click();
  await expect(pending).toHaveCount(0, live);

  const after = await engine.agent.call<{ task: { status: string }; history: { actor: string; kind: string }[] }>(
    "task.get",
    { id: task.id },
  );
  expect(after.task.status).toBe("done");
  expect(after.history.at(-1)).toMatchObject({ actor: "human:local", kind: "task_review", approved: true });
});
