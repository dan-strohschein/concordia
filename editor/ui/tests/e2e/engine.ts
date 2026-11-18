// Test fixture: a headless engine on a temporary copy of the example project, plus an "agent"
// API connection for setting up scenes and checking results from outside the editor.
import { type ChildProcess, spawn } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { test as base } from "@playwright/test";

import { EngineClient, websocketTransport } from "../../src/api/client";

const repo = resolve(import.meta.dirname, "../../../..");

export interface Engine {
  /** `ws://…?token=…` for the editor page. */
  url: string;
  /** An agent connection (`agent:test#1`). */
  agent: EngineClient;
  /** Ids of entities by Name in the edit world. */
  idsByName: () => Promise<Record<string, string>>;
}

async function waitForFile(path: string, ms: number): Promise<void> {
  const start = Date.now();
  while (!existsSync(path)) {
    if (Date.now() - start > ms) throw new Error(`no ${path} after ${ms} ms`);
    await new Promise((r) => setTimeout(r, 50));
  }
}

export const test = base.extend<{ engine: Engine }>({
  engine: async ({}, use) => {
    const dir = mkdtempSync(join(tmpdir(), "concordia-e2e-"));
    cpSync(join(repo, "examples/m1-physics-stack"), dir, { recursive: true });
    const binary = join(repo, "target/debug/concordia");
    const child: ChildProcess = spawn(binary, ["headless", "--ws", "--port", "0", "--project", dir], {
      stdio: "ignore",
    });
    try {
      const file = join(dir, ".concordia/headless.json");
      await waitForFile(file, 20_000);
      const info = JSON.parse(readFileSync(file, "utf8")) as { url: string; token: string };
      const url = `${info.url}/?token=${info.token}`;
      const agent = new EngineClient(await websocketTransport(url));
      await agent.call("session.open", { client: "e2e-agent", actor: "agent:test#1" });
      await agent.call("scene.open", { path: "scenes/stack.scene.ron" });
      const idsByName = async () => {
        const result = await agent.call<{ rows: { id: string; components: { Name?: string } }[] }>("query.run", {
          with: ["Name"],
          limit: 1000,
        });
        return Object.fromEntries(result.rows.map((r) => [r.components.Name ?? r.id, r.id]));
      };
      await use({ url, agent, idsByName });
      agent.close();
    } finally {
      child.kill();
      rmSync(dir, { recursive: true, force: true });
    }
  },
});

export const expect = test.expect;
