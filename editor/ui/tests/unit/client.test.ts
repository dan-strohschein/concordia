// EngineClient: JSON-RPC over any transport (M1 phase 1.9).
import { EngineClient, JsonRpcError, TimeoutError, type Transport } from "../../src/api/client";

/** A transport the test drives by hand. */
class FakeTransport implements Transport {
  sent: Record<string, unknown>[] = [];
  closed = false;
  private handler: ((text: string) => void) | undefined;
  send(text: string): void {
    this.sent.push(JSON.parse(text) as Record<string, unknown>);
  }
  onMessage(handler: (text: string) => void): void {
    this.handler = handler;
  }
  close(): void {
    this.closed = true;
  }
  private dropped: (() => void) | undefined;
  onClose(handler: () => void): void {
    this.dropped = handler;
  }
  /** The other end went away (the engine's host exited). */
  drop(): void {
    this.dropped?.();
  }
  deliver(message: unknown): void {
    this.handler?.(typeof message === "string" ? message : JSON.stringify(message));
  }
}

test("concordia_client_correlates_responses_by_id_out_of_order", async () => {
  const t = new FakeTransport();
  const client = new EngineClient(t);
  const a = client.call("scene.info");
  const b = client.call("schema.get", { name: "Transform" });
  expect(t.sent.map((m) => m["method"])).toEqual(["scene.info", "schema.get"]);
  expect(t.sent[1]?.["params"]).toEqual({ name: "Transform" });
  t.deliver({ jsonrpc: "2.0", id: t.sent[1]?.["id"], result: { schema: "B" } });
  t.deliver({ jsonrpc: "2.0", id: t.sent[0]?.["id"], result: { name: "A" } });
  await expect(a).resolves.toEqual({ name: "A" });
  await expect(b).resolves.toEqual({ schema: "B" });
});

test("concordia_client_rejects_on_timeout_and_jsonrpc_error", async () => {
  vi.useFakeTimers();
  try {
    const t = new FakeTransport();
    const client = new EngineClient(t, { timeoutMs: 1000 });
    const slow = client.call("sim.step", { ticks: 1 });
    vi.advanceTimersByTime(1001);
    await expect(slow).rejects.toBeInstanceOf(TimeoutError);
    const failing = client.call("entity.get", { id: "nope" });
    t.deliver({
      jsonrpc: "2.0",
      id: t.sent[1]?.["id"],
      error: { code: -32001, message: "no entity", data: { kind: "EntityNotFound", suggestions: ["query.run"] } },
    });
    const error = await failing.catch((e: unknown) => e);
    expect(error).toBeInstanceOf(JsonRpcError);
    expect((error as JsonRpcError).code).toBe(-32001);
    expect((error as JsonRpcError).kind).toBe("EntityNotFound");
    expect((error as JsonRpcError).suggestions).toEqual(["query.run"]);
  } finally {
    vi.useRealTimers();
  }
});

test("events are delivered to listeners until they unsubscribe", () => {
  const t = new FakeTransport();
  const client = new EngineClient(t);
  const seen: unknown[] = [];
  const stop = client.onEvent((event) => seen.push(event));
  t.deliver({ jsonrpc: "2.0", method: "events.event", params: { topic: "edit", entities: ["e1"], actor: "agent:x" } });
  stop();
  t.deliver({ jsonrpc: "2.0", method: "events.event", params: { topic: "edit", entities: ["e2"], actor: "agent:x" } });
  expect(seen).toEqual([{ topic: "edit", entities: ["e1"], actor: "agent:x" }]);
});

test("malformed messages are reported, not thrown, and unknown ids are ignored", () => {
  const t = new FakeTransport();
  const problems: string[] = [];
  const client = new EngineClient(t, { onProtocolError: (p) => problems.push(p) });
  t.deliver("not json");
  t.deliver({ jsonrpc: "1.0", id: 1, result: {} });
  t.deliver({ jsonrpc: "2.0", id: 999, result: {} });
  expect(problems).toHaveLength(3);
  expect(client.pending).toBe(0);
});

test("close rejects pending calls and closes the transport", async () => {
  const t = new FakeTransport();
  const client = new EngineClient(t);
  const pending = client.call("scene.info");
  client.close();
  await expect(pending).rejects.toThrow(/closed/);
  expect(t.closed).toBe(true);
  await expect(client.call("scene.info")).rejects.toThrow(/closed/);
});

test("a_lost_engine_connection_rejects_pending_and_later_calls_and_tells_listeners", async () => {
  const t = new FakeTransport();
  const client = new EngineClient(t);
  const lost: string[] = [];
  client.onLost((reason) => lost.push(reason));
  const pending = client.call("scene.info");
  t.drop();
  await expect(pending).rejects.toThrow(/engine.*exited/);
  await expect(client.call("scene.info")).rejects.toThrow(/engine.*exited/);
  expect(client.lost).toBe(true);
  expect(lost).toHaveLength(1);
  client.close();
  t.drop();
  expect(lost).toHaveLength(1);
});

test("closing the client on purpose is not a lost engine", () => {
  const t = new FakeTransport();
  const client = new EngineClient(t);
  const lost: string[] = [];
  client.onLost((reason) => lost.push(reason));
  client.close();
  t.drop();
  expect(lost).toEqual([]);
  expect(client.lost).toBe(false);
});
