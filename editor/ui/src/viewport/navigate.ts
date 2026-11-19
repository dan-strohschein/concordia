// The editor camera lives in the engine (`viewport.navigate`). In a browser the page sends moves,
// coalescing drags while a call is in flight; in the native shell, Rust handles navigation input
// and reports each new view.
import { useCallback, useMemo } from "react";

import type { EditorSession } from "../engine/session";
import type { Preset, Vec3, View } from "./camera";

/** `viewport.navigate` params. */
export interface NavParams {
  preset?: Preset;
  toggle_ortho?: boolean;
  focus?: Vec3;
  orbit?: [number, number];
  pan?: [number, number];
  height?: number;
  zoom?: number;
}

const add = (a: [number, number], b: [number, number]): [number, number] => [a[0] + b[0], a[1] + b[1]];
const only = (p: NavParams, ...keys: (keyof NavParams)[]) =>
  Object.keys(p).every((k) => keys.includes(k as keyof NavParams));

/** One move equal to `a` then `b`, or `undefined` if they cannot be combined. */
export function mergeNav(a: NavParams, b: NavParams): NavParams | undefined {
  if (a.orbit && b.orbit && only(a, "orbit") && only(b, "orbit")) return { orbit: add(a.orbit, b.orbit) };
  if (a.pan && b.pan && only(a, "pan", "height") && only(b, "pan", "height")) {
    const height = b.height ?? a.height;
    return { pan: add(a.pan, b.pan), ...(height === undefined ? {} : { height }) };
  }
  if (a.zoom !== undefined && b.zoom !== undefined && only(a, "zoom") && only(b, "zoom"))
    return { zoom: a.zoom + b.zoom };
  return undefined;
}

/** Camera moves sent one at a time, in order; drags that queue up meanwhile are merged. */
export class NavQueue {
  private busy = false;
  private readonly queue: NavParams[] = [];

  constructor(private readonly send: (p: NavParams) => Promise<void>) {}

  push(p: NavParams): void {
    const last = this.queue.at(-1);
    const merged = last === undefined ? undefined : mergeNav(last, p);
    if (merged) this.queue[this.queue.length - 1] = merged;
    else this.queue.push(p);
    this.pump();
  }

  private pump(): void {
    if (this.busy) return;
    const next = this.queue.shift();
    if (next === undefined) return;
    this.busy = true;
    void this.send(next).finally(() => {
      this.busy = false;
      this.pump();
    });
  }
}

/** Sends camera moves to `viewport.navigate`; `onView` gets each new view. */
export function useNavigate(session: EditorSession, onView: (view: View) => void): (p: NavParams) => void {
  const queue = useMemo(
    () =>
      new NavQueue((p) =>
        session.call<{ view: View }>("viewport.navigate", { ...p }).then(
          (r) => onView(r.view),
          () => undefined,
        ),
      ),
    [session, onView],
  );
  return useCallback((p: NavParams) => queue.push(p), [queue]);
}
