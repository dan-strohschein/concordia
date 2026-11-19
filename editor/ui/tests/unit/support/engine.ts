// A fake engine behind a Transport: answers calls from a handler table and records them.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import type { Transport } from "../../../src/api/client";

export type Handler = (params: Record<string, unknown>) => unknown;

export class FakeEngine implements Transport {
  calls: { method: string; params: Record<string, unknown> }[] = [];
  private receive: ((text: string) => void) | undefined;
  constructor(public handlers: Record<string, Handler>) {}
  send(text: string): void {
    const msg = JSON.parse(text) as { id: number; method: string; params: Record<string, unknown> };
    this.calls.push({ method: msg.method, params: msg.params });
    const handler = this.handlers[msg.method];
    const reply = handler
      ? { jsonrpc: "2.0", id: msg.id, result: handler(msg.params) }
      : { jsonrpc: "2.0", id: msg.id, error: { code: -32601, message: `no ${msg.method}` } };
    queueMicrotask(() => this.receive?.(JSON.stringify(reply)));
  }
  onMessage(handler: (text: string) => void): void {
    this.receive = handler;
  }
  close(): void {}
  /** Pushes an `events.event` notification. */
  event(params: Record<string, unknown>): void {
    this.receive?.(JSON.stringify({ jsonrpc: "2.0", method: "events.event", params }));
  }
  called(method: string): Record<string, unknown>[] {
    return this.calls.filter((c) => c.method === method).map((c) => c.params);
  }
}

export const schema = (name: string): unknown =>
  JSON.parse(readFileSync(join(__dirname, "../../fixtures/schemas", `${name}.schema.json`), "utf8"));

/** Handlers for a scene with one entity `e1` holding a Name and a Transform. */
export function oneEntity(transform: { translation: number[] }): Record<string, Handler> {
  return {
    "session.open": () => ({ session: "s1", concordia_version: "0", protocol: "1" }),
    "events.subscribe": (p) => ({ topics: p["topics"] }),
    "schema.get": (p) => ({ schema: schema(String(p["name"])) }),
    "entity.get": () => ({
      id: "e1",
      components: { Name: "Crate", Transform: { ...transform, rotation: [0, 0, 0, 1], scale: [1, 1, 1] } },
    }),
    "component.set": () => ({ previous: null }),
    "query.run": () => ({ rows: [{ id: "e1", components: { Name: "Crate" } }] }),
  };
}
