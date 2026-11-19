// A minimal MCP client over `concordia mcp`'s stdio (newline-delimited JSON-RPC), for e2e tests
// that act as Claude Code would.
import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";

const repo = resolve(import.meta.dirname, "../../../..");

/** A minimal MCP client over the server's stdio (newline-delimited JSON-RPC). */
export class McpStdio {
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

  /** Sends `initialize` and `notifications/initialized`. */
  async initialize(): Promise<void> {
    await this.request("initialize", {
      protocolVersion: "2025-11-25",
      capabilities: {},
      clientInfo: { name: "e2e", version: "1" },
    });
    this.notify("notifications/initialized");
  }

  close(): void {
    this.child.kill();
  }
}
