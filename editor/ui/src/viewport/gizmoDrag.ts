// A Move-gizmo drag driven from the page (browser mode; the native shell drags natively). The
// engine draws the gizmo, finds the handle under the press and moves the entity (`viewport.gizmo`);
// the page only forwards the pointer, sending the latest position while a call is in flight.
import type { EditorSession } from "../engine/session";
import { inputTags } from "./latency";

interface Size {
  width: number;
  height: number;
}

export class GizmoDrag {
  private active = false;
  private busy = false;
  private size: Size = { width: 1, height: 1 };
  private latest: { x: number; y: number; seq?: number; input_time_ms?: number } | undefined;
  private sending: Promise<void> = Promise.resolve();

  constructor(
    private readonly session: EditorSession,
    private readonly actor: string,
  ) {}

  /** Presses at (x, y) in a `size` viewport; resolves whether the press grabbed a handle. */
  async start(x: number, y: number, size: Size): Promise<boolean> {
    this.size = size;
    const r = await this.session.call<{ axis: string | null }>("viewport.gizmo", {
      action: "begin",
      x,
      y,
      ...size,
      actor: this.actor,
    });
    this.active = r.axis !== null;
    if (this.active) this.flush();
    return this.active;
  }

  /** The pointer moved (viewport pixels); `timeStamp` tags it for latency stats. */
  move(x: number, y: number, timeStamp?: number): void {
    this.latest = { x, y, ...(timeStamp === undefined ? {} : inputTags.mark(timeStamp)) };
    if (this.active) this.flush();
  }

  /** Releases: commits the move as one undo step. */
  async end(): Promise<void> {
    if (!this.active) return;
    // Let the last position land before committing.
    while (this.busy || this.latest !== undefined) {
      if (!this.busy) this.flush();
      await this.sending;
    }
    this.active = false;
    await this.session.call("viewport.gizmo", { action: "end", actor: this.actor });
  }

  private flush(): void {
    const next = this.latest;
    if (this.busy || next === undefined) return;
    this.latest = undefined;
    this.busy = true;
    this.sending = this.session
      .call("viewport.gizmo", { action: "move", ...next, ...this.size, actor: this.actor })
      .then(
        () => undefined,
        () => undefined,
      )
      .finally(() => {
        this.busy = false;
        if (this.active) this.flush();
      });
  }
}
