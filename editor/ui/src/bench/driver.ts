// The editor benchmark (`concordia-editor --bench`): drives the real UI with synthetic mouse
// events, paced by the display, and reports frame pacing, input-to-photon latency and panel
// timings. Scenario setup and stats use a separate API connection, as an agent would.
import { EngineClient, websocketTransport } from "../api/client";
import { percentile, summarize, type FrameRecord, type FrameSummary } from "./stats";

declare global {
  interface Window {
    /** Set by the native shell in bench mode. */
    __CONCORDIA_BENCH__?: { frames: number };
  }
}

const ACTOR = "agent:bench";

/** Resolves on the next display frame with its timestamp. */
const nextFrame = () => new Promise<number>((resolve) => requestAnimationFrame(resolve));

/** Page frame intervals over `frames` display frames while `step(i)` runs once per frame. */
async function paced(frames: number, step: (i: number) => void): Promise<number[]> {
  const times: number[] = [];
  for (let i = 0; i < frames; i++) {
    step(i);
    times.push(await nextFrame());
  }
  return times.slice(1).map((t, i) => t - (times[i] ?? t));
}

/** The first element matching `find`, waiting up to `ms`. */
async function waitFor<T>(find: () => T | null | undefined, what: string, ms = 20_000): Promise<T> {
  const start = performance.now();
  for (;;) {
    const found = find();
    if (found) return found;
    if (performance.now() - start > ms) throw new Error(`bench: ${what} never appeared`);
    await nextFrame();
  }
}

const treeItem = (name: string) =>
  [...document.querySelectorAll<HTMLElement>('[role="treeitem"]')].find((e) => e.textContent?.trim() === name);

function mouse(target: EventTarget, type: string, x: number, y: number, button: number) {
  target.dispatchEvent(new MouseEvent(type, { bubbles: true, clientX: x, clientY: y, button, buttons: 1 << button }));
}

/** One scenario's result. */
export interface ScenarioResult {
  viewport: FrameSummary;
  page_interval_ms: { p50: number; p95: number; max: number };
  page_missed_refreshes: number;
  edit_to_panel_ms?: { p50: number; p95: number; max: number };
}

class Bench {
  /** The page's requestAnimationFrame interval (WebKit caps it at 60 Hz). */
  refreshMs = 1000 / 60;
  /** The display's refresh interval, for viewport pacing. */
  displayMs = 1000 / 60;

  constructor(
    private readonly client: EngineClient,
    private readonly frames: number,
  ) {}

  private async stats(): Promise<FrameRecord[]> {
    await new Promise((r) => setTimeout(r, 300)); // let the last presented callbacks land
    return (await this.client.call<{ frames: FrameRecord[] }>("viewport.stats", { clear: true })).frames;
  }

  private result(frames: FrameRecord[], intervals: number[]): ScenarioResult {
    return {
      viewport: summarize(frames, this.displayMs),
      page_interval_ms: { p50: percentile(intervals, 50), p95: percentile(intervals, 95), max: Math.max(...intervals) },
      page_missed_refreshes: intervals.reduce((n, i) => n + Math.max(0, Math.round(i / this.refreshMs) - 1), 0),
    };
  }

  async idle(): Promise<ScenarioResult> {
    const intervals = await paced(120, () => undefined);
    this.refreshMs = percentile(intervals, 50);
    return this.result(await this.stats(), intervals);
  }

  /** Middle-drag orbit, 3 px per display frame; `during(i)` runs alongside each frame. */
  async orbit(during: (i: number) => void = () => undefined): Promise<ScenarioResult> {
    const slot = await waitFor(() => document.getElementById("viewport-slot"), "viewport");
    const box = slot.getBoundingClientRect();
    const [x, y] = [box.left + box.width / 2, box.top + box.height / 2];
    await this.stats();
    mouse(slot, "mousedown", x, y, 1);
    const intervals = await paced(this.frames, (i) => {
      mouse(slot, "mousemove", x + 3 * (i + 1), y, 1);
      during(i);
    });
    mouse(slot, "mouseup", x + 3 * this.frames, y, 1);
    return this.result(await this.stats(), intervals);
  }

  /** Orbits while an agent renames an entity every 15 frames; times each rename to the outliner. */
  async orbitWithEdits(probe: string): Promise<ScenarioResult> {
    const panel: Promise<number>[] = [];
    const result = await this.orbit((i) => {
      if (i % 15 === 0) panel.push(this.renameToPanel(probe, `Bench probe ${i}`));
    });
    const ms = await Promise.all(panel);
    return { ...result, edit_to_panel_ms: { p50: percentile(ms, 50), p95: percentile(ms, 95), max: Math.max(...ms) } };
  }

  private async renameToPanel(probe: string, name: string): Promise<number> {
    const start = performance.now();
    await this.client.call("component.set", { entity: probe, component: "Name", value: name, actor: ACTOR });
    await waitFor(() => treeItem(name), `outliner row ${name}`);
    return performance.now() - start;
  }

  /** Drags the selected entity's Move-x handle 1 px per display frame. */
  async gizmo(probeName: string): Promise<ScenarioResult> {
    (await waitFor(() => treeItem(probeName)?.querySelector<HTMLElement>(":scope > div"), "probe row")).click();
    (await waitFor(() => document.querySelector<HTMLElement>('button[title="Move (W)"]'), "Move tool")).click();
    const handle = await waitFor(() => document.querySelector('[aria-label="Move x"] circle'), "Move x handle");
    const box = handle.getBoundingClientRect();
    const [x, y] = [box.left + box.width / 2, box.top + box.height / 2];
    await this.stats();
    mouse(handle, "mousedown", x, y, 0);
    const intervals = await paced(this.frames, (i) => mouse(window, "mousemove", x + (i % 120) - 60, y, 0));
    mouse(window, "mouseup", x, y, 0);
    return this.result(await this.stats(), intervals);
  }

  /** Plays the simulation (inspector open on the selection) for the scenario's frames. */
  async play(): Promise<ScenarioResult> {
    await this.stats();
    (await waitFor(() => document.querySelector<HTMLElement>('button[title="Play"]'), "Play button")).click();
    const intervals = await paced(this.frames, () => undefined);
    await this.client.call("sim.stop", { actor: ACTOR });
    return this.result(await this.stats(), intervals);
  }

  /** The shell presents the same orbit from Rust, with no web layer involved. */
  async nativeBaseline(): Promise<ScenarioResult> {
    const { invoke } = await import("@tauri-apps/api/core");
    await this.stats();
    await invoke("bench_native_orbit", { frames: this.frames });
    return this.result(await this.stats(), []);
  }
}

/** Runs every scenario and hands the report to the shell. */
export async function runBench(url: string, frames: number): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    const client = new EngineClient(await websocketTransport(url));
    const info = await client.call<{ entity_count: number }>("scene.info", {});
    const probe = await client.call<{ id: string }>("entity.spawn", {
      components: {
        Name: "Bench probe",
        Transform: { translation: [0, 1, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
      },
      actor: ACTOR,
    });
    const bench = new Bench(client, frames);
    const displayHz = await invoke<number | null>("bench_display_hz");
    if (displayHz) bench.displayMs = 1000 / displayHz;
    const stage = async <T>(name: string, run: () => Promise<T>): Promise<T> => {
      await invoke("bench_progress", { stage: name });
      return run();
    };
    const report = {
      entities: info.entity_count + 1,
      device_pixel_ratio: window.devicePixelRatio,
      idle: await stage("idle", () => bench.idle()),
      orbit: await stage("orbit", () => bench.orbit()),
      native_orbit_baseline: await stage("native orbit baseline", () => bench.nativeBaseline()),
      orbit_with_agent_edits: await stage("orbit with agent edits", () => bench.orbitWithEdits(probe.id)),
      gizmo_drag: await stage("gizmo drag", async () => {
        const probeNow = await client.call<{ components: { Name: string } }>("entity.get", { id: probe.id });
        return bench.gizmo(probeNow.components.Name);
      }),
      play: await stage("play", () => bench.play()),
      refresh_ms: bench.refreshMs,
      display_hz: displayHz,
    };
    await invoke("bench_report", { report: JSON.stringify(report) });
  } catch (e) {
    await invoke("bench_report", { report: JSON.stringify({ error: String(e) }) });
  }
}
