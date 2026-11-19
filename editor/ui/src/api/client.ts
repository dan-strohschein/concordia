// A typed JSON-RPC 2.0 client for the Concordia Engine API over any message transport.

/** Sends and receives whole JSON-RPC messages as text. */
export interface Transport {
  send(text: string): void;
  onMessage(handler: (text: string) => void): void;
  close(): void;
  /** Called once if the other end goes away (the engine's host exited). */
  onClose?(handler: () => void): void;
}

/** A JSON-RPC error returned by the engine (code, `data.kind`, suggestions). */
export class JsonRpcError extends Error {
  readonly code: number;
  readonly kind: string | undefined;
  readonly suggestions: string[];
  readonly data: unknown;

  constructor(code: number, message: string, data: unknown) {
    super(message);
    this.name = "JsonRpcError";
    this.code = code;
    this.data = data;
    const d = isObject(data) ? data : {};
    this.kind = typeof d["kind"] === "string" ? d["kind"] : undefined;
    this.suggestions = Array.isArray(d["suggestions"]) ? d["suggestions"].filter((s) => typeof s === "string") : [];
  }
}

/** No response within the client's timeout. */
export class TimeoutError extends Error {
  constructor(method: string, ms: number) {
    super(`${method} timed out after ${ms} ms`);
    this.name = "TimeoutError";
  }
}

/** Parameters of an `events.event` notification. */
export type EngineEvent = Record<string, unknown> & { topic: string };

export interface ClientOptions {
  /** Milliseconds before a call rejects (default 10 000). */
  timeoutMs?: number;
  /** Called with a description of each malformed or unexpected message. */
  onProtocolError?: (problem: string) => void;
}

interface Pending {
  method: string;
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export class EngineClient {
  private readonly transport: Transport;
  private readonly timeoutMs: number;
  private readonly onProtocolError: (problem: string) => void;
  private readonly waiting = new Map<number, Pending>();
  private readonly listeners = new Set<(event: EngineEvent) => void>();
  private readonly lostListeners = new Set<(reason: string) => void>();
  private nextId = 1;
  private closed = false;
  private lostReason: string | undefined;

  constructor(transport: Transport, options: ClientOptions = {}) {
    if (options.timeoutMs !== undefined && !(options.timeoutMs > 0)) {
      throw new RangeError(`timeoutMs must be positive, got ${options.timeoutMs}`);
    }
    this.transport = transport;
    this.timeoutMs = options.timeoutMs ?? 10_000;
    this.onProtocolError = options.onProtocolError ?? (() => {});
    transport.onMessage((text) => this.receive(text));
    transport.onClose?.(() => this.lose());
  }

  /** True once the engine went away without the client closing (e.g. its host exited). */
  get lost(): boolean {
    return this.lostReason !== undefined;
  }

  /** Listens for the engine going away; returns the unsubscribe function. */
  onLost(listener: (reason: string) => void): () => void {
    this.lostListeners.add(listener);
    return () => this.lostListeners.delete(listener);
  }

  /** Calls in flight. */
  get pending(): number {
    return this.waiting.size;
  }

  /** Calls `method`; resolves with its result or rejects with a `JsonRpcError` or `TimeoutError`. */
  call<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    if (this.lostReason !== undefined) return Promise.reject(new Error(this.lostReason));
    if (this.closed) return Promise.reject(new Error("the engine connection is closed"));
    if (method.length === 0) return Promise.reject(new RangeError("method must not be empty"));
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiting.delete(id);
        reject(new TimeoutError(method, this.timeoutMs));
      }, this.timeoutMs);
      this.waiting.set(id, { method, resolve: resolve as (value: unknown) => void, reject, timer });
      this.transport.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
    });
  }

  /** Listens to `events.event` notifications; returns the unsubscribe function. */
  onEvent(listener: (event: EngineEvent) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Rejects every pending call and closes the transport. */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.rejectAll((method) => `the engine connection is closed (${method})`);
    this.transport.close();
  }

  private lose(): void {
    if (this.closed) return;
    this.closed = true;
    const reason = "the engine has exited (the process hosting it closed)";
    this.lostReason = reason;
    this.rejectAll((method) => `${reason} (${method})`);
    for (const listener of this.lostListeners) listener(reason);
  }

  private rejectAll(message: (method: string) => string): void {
    for (const [id, p] of this.waiting) {
      clearTimeout(p.timer);
      p.reject(new Error(message(p.method)));
      this.waiting.delete(id);
    }
  }

  private receive(text: string): void {
    let message: unknown;
    try {
      message = JSON.parse(text);
    } catch {
      this.onProtocolError(`not JSON: ${text.slice(0, 80)}`);
      return;
    }
    if (!isObject(message) || message["jsonrpc"] !== "2.0") {
      this.onProtocolError(`not a JSON-RPC 2.0 message: ${text.slice(0, 80)}`);
      return;
    }
    if (typeof message["method"] === "string") {
      this.notify(message);
      return;
    }
    this.settle(message);
  }

  private notify(message: Record<string, unknown>): void {
    const params = message["params"];
    if (message["method"] !== "events.event" || !isObject(params) || typeof params["topic"] !== "string") {
      this.onProtocolError(`unexpected notification ${String(message["method"])}`);
      return;
    }
    for (const listener of this.listeners) listener(params as EngineEvent);
  }

  private settle(message: Record<string, unknown>): void {
    const id = message["id"];
    const pending = typeof id === "number" ? this.waiting.get(id) : undefined;
    if (pending === undefined || typeof id !== "number") {
      this.onProtocolError(`response for unknown id ${String(id)}`);
      return;
    }
    this.waiting.delete(id);
    clearTimeout(pending.timer);
    const error = message["error"];
    if (isObject(error)) {
      const code = typeof error["code"] === "number" ? error["code"] : -32603;
      const text = typeof error["message"] === "string" ? error["message"] : "engine error";
      pending.reject(new JsonRpcError(code, text, error["data"]));
    } else {
      pending.resolve(message["result"]);
    }
  }
}

/** Opens a WebSocket to the engine (`ws://127.0.0.1:<port>/?token=…`). */
export function websocketTransport(url: string): Promise<Transport> {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    const queue: string[] = [];
    let handler: ((text: string) => void) | undefined;
    let closed: (() => void) | undefined;
    socket.onclose = () => closed?.();
    socket.onmessage = (e: MessageEvent) => {
      if (typeof e.data !== "string") return;
      if (handler) handler(e.data);
      else queue.push(e.data);
    };
    socket.onerror = () => reject(new Error(`could not connect to ${url.replace(/token=[^&]*/, "token=…")}`));
    socket.onopen = () =>
      resolve({
        send: (text) => socket.send(text),
        onMessage: (h) => {
          handler = h;
          queue.splice(0).forEach(h);
        },
        close: () => socket.close(),
        onClose: (h) => {
          closed = h;
        },
      });
  });
}
