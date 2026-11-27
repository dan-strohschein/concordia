// The graph panel end to end (M4 4.11b): an agent and a human edit the same graph file through
// the Engine API; the panel follows the agent live, undo restores the file's bytes, and the
// command log credits both.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import type { Page } from "@playwright/test";

import { expect, test, type Engine } from "./engine";

const PATH = "graphs/agent.graph.ron";
const id = (n: number) => `0193b0f2-0000-7000-8000-${n.toString(16).padStart(12, "0")}`;

/** The agent's graph: on tick, branch on dt > 0.01. */
async function agentGraph(engine: Engine) {
  await engine.agent.call("graph.apply", {
    path: PATH,
    kind: "Event",
    actor: "agent:test#1",
    edits: [
      { AddNode: { id: id(1), op: "event.on_tick", at: [0, 0] } },
      { AddNode: { id: id(2), op: "math.gt", at: [260, 0] } },
      { AddNode: { id: id(3), op: "branch", at: [520, 0] } },
      { Connect: { from: id(1), from_pin: "dt", to: id(2), to_pin: "a" } },
      { SetValue: { node: id(2), pin: "b", value: { Float: 0.01 } } },
      { Connect: { from: id(2), from_pin: "out", to: id(3), to_pin: "cond" } },
      { ConnectExec: { from: id(1), from_pin: "out", to: id(3), to_pin: "in" } },
    ],
  });
}

const fileText = (engine: Engine) => readFileSync(join(engine.dir, PATH), "utf8");

async function openGraph(page: Page, engine: Engine) {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  const files = page.getByRole("tree", { name: "Project files" });
  await expect(files).toBeVisible();
  await files.getByText("agent.graph.ron", { exact: true }).dblclick();
  await expect(page.getByTestId(`graph-node-${id(3)}`)).toBeVisible();
}

test("graph_panel_reflects_agent_edit_within_one_second", async ({ page, engine }) => {
  await agentGraph(engine);
  await openGraph(page, engine);
  const start = Date.now();
  await engine.agent.call("graph.apply", {
    path: PATH,
    actor: "agent:test#1",
    edits: [{ AddNode: { id: id(4), op: "math.add", at: [260, 160] } }],
  });
  await expect(page.getByTestId(`graph-node-${id(4)}`)).toBeVisible({ timeout: 1000 });
  expect(Date.now() - start).toBeLessThan(1000);
});

test("graph_panel_undo_round_trips_file_bytes", async ({ page, engine }) => {
  await agentGraph(engine);
  const agentBytes = fileText(engine);
  await openGraph(page, engine);
  const b = page.getByLabel(`${id(2)} b`);
  await b.fill("0.5");
  await b.press("Enter");
  await expect.poll(() => fileText(engine)).not.toBe(agentBytes);
  expect(fileText(engine)).toContain("Float(0.5)");
  // Undo from the editor (outside a text field).
  await page.getByRole("tree", { name: "Project files" }).click();
  await page.keyboard.press("ControlOrMeta+z");
  await expect.poll(() => fileText(engine)).toBe(agentBytes);
  await expect(page.getByLabel(`${id(2)} b`)).toHaveValue("0.01");
});

test("human_edit_of_agent_graph_logs_both_actors", async ({ page, engine }) => {
  await agentGraph(engine);
  await openGraph(page, engine);
  const b = page.getByLabel(`${id(2)} b`);
  await b.fill("0.25");
  await b.press("Enter");
  await expect(page.getByTestId(`author-${id(2)}`)).toHaveText("human:local");
  await expect(page.getByTestId(`author-${id(3)}`)).toContainText("agent:test");
  const history = await engine.agent.call<{ entries: { actor: string; label: string; undone: boolean }[] }>(
    "txn.history",
    {},
  );
  const edits = history.entries.filter((e) => e.label === "graph.apply" && !e.undone).map((e) => e.actor);
  expect(edits).toEqual(["agent:test#1", "human:local"]);
});
