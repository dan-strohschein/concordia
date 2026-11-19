// One engine per project (M3 phase 3.5b, ADR-0082): `concordia mcp` attaches to the engine the
// editor uses, so an MCP edit shows up live in Agent Activity.
import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";

import { expect, test } from "./engine";

const repo = resolve(import.meta.dirname, "../../../..");

/** A minimal MCP client over the server's stdio (newline-delimited JSON-RPC). */
class McpStdio {
  private readonly child: ChildProcessWithoutNullStreams;
  private readonly waiting = new Map<number, (message: Record<string, unknown>) => void>();
  private nextId = 1;

  constructor(project: string) {
    this.child = spawn(join(repo, "target/debug/concordia"), ["mcp", "--project", project]);
    createInterface({ input: this.child.stdout }).on("line", (line) => {
      const message = JSON.parse(line) as Record<string, unknown>;
      const id = message["id"];
      if (typeof id === "number") this.waiting.get(id)?.(message);
    });
  }

  request(method: string, params: Record<string, unknown>): Promise<Record<string, unknown>> {
    const id = this.nextId++;
    return new Promise((resolve) => {
      this.waiting.set(id, resolve);
      this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });
  }

  notify(method: string): void {
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method })}\n`);
  }

  async tool(name: string, args: Record<string, unknown>): Promise<Record<string, unknown>> {
    const reply = await this.request("tools/call", { name, arguments: args });
    const result = reply["result"] as { isError?: boolean; structuredContent?: Record<string, unknown> };
    if (result.isError) throw new Error(`${name}: ${JSON.stringify(result)}`);
    return result.structuredContent ?? {};
  }

  close(): void {
    this.child.kill();
  }
}

test("editor_and_mcp_share_one_command_log", async ({ page, engine }) => {
  await page.goto(`/?engine=${encodeURIComponent(engine.url)}`);
  const panel = page.getByRole("region", { name: "Agent Activity dock" });
  await expect(panel).toBeVisible();
  const id = (await engine.idsByName())["Ground"];

  const mcp = new McpStdio(engine.dir);
  try {
    await mcp.request("initialize", {
      protocolVersion: "2025-11-25",
      capabilities: {},
      clientInfo: { name: "e2e", version: "1" },
    });
    mcp.notify("notifications/initialized");
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
