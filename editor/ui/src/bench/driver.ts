// The editor benchmark (`concordia-editor --bench`): drives the real UI with synthetic mouse
// events, paced by the display, and reports frame pacing, input-to-photon latency and panel
// timings. Scenario setup and stats use a separate API connection, as an agent would.
import { EngineClient, websocketTransport } from "../api/client";
import { project, type Vec3, type View } from "../viewport/camera";
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

/** Scrolls the virtualized outliner until a row starting with `prefix` is rendered. */
async function revealRow(prefix: string): Promise<void> {
  const scroller = await waitFor(
    () => document.querySelector<HTMLElement>('[data-testid="outliner-scroll"]'),
    "outliner",
  );
  const found = () =>
    [...document.querySelectorAll<HTMLElement>('[role="treeitem"]')].find((e) =>
      e.textContent?.trim().startsWith(prefix),
    );
  for (let top = 0; top <= scroller.scrollHeight; top += Math.max(100, scroller.clientHeight / 2)) {
    scroller.scrollTop = top;
    scroller.dispatchEvent(new Event("scroll"));
    await nextFrame();
    const row = found();
    if (row) return row.scrollIntoView({ block: "center" });
  }
  throw new Error(`bench: no outliner row starts with ${prefix}`);
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

  /** Orbit through native input (synthetic AppKit drags, one per display refresh); `during(i)`
   * runs alongside each page frame. */
  async orbit(during: (i: number) => void = () => undefined): Promise<ScenarioResult> {
    const { invoke } = await import("@tauri-apps/api/core");
    await this.stats();
    const drag = invoke("bench_native_drag", { frames: this.frames * 2 });
    const intervals = await paced(this.frames, during);
    await drag;
    return this.result(await this.stats(), intervals);
  }

  /** Orbits while an agent renames an entity every 15 frames; times each rename to the outliner. */
  async orbitWithEdits(probe: string): Promise<ScenarioResult> {
    await revealRow("Bench probe");
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

  /** Drags the selected entity's Move-x handle with native input, one step per display refresh. */
  async gizmo(probeName: string, probe: string): Promise<ScenarioResult> {
    const { invoke } = await import("@tauri-apps/api/core");
    await revealRow(probeName);
    (await waitFor(() => treeItem(probeName)?.querySelector<HTMLElement>(":scope > div"), "probe row")).click();
    (await waitFor(() => document.querySelector<HTMLElement>('button[title="Move (W)"]'), "Move tool")).click();
    const slot = (await waitFor(() => document.getElementById("viewport-slot"), "viewport")).getBoundingClientRect();
    const from = await this.gizmoTip(probe, slot.width, slot.height);
    await nextFrame(); // let the overlays reach the engine
    await this.stats();
    const drag = invoke("bench_native_drag", { frames: this.frames * 2, from, alt: false });
    const intervals = await paced(this.frames, () => undefined);
    await drag;
    return this.result(await this.stats(), intervals);
  }

  /** Where the engine draws the Move gizmo's x arrow tip for `entity` (viewport points). */
  private async gizmoTip(entity: string, width: number, height: number): Promise<[number, number]> {
    const { view } = await this.client.call<{ view: View }>("viewport.navigate", {});
    const t = await this.client.call<{ value: { translation: Vec3 } }>("component.get", {
      entity,
      component: "Transform",
    });
    const [x, y, z] = t.value.translation;
    const o = project(view, [x, y, z], width, height);
    const e = project(view, [x + 1, y, z], width, height);
    if (!o || !e) throw new Error("bench: the gizmo is not in view");
    const n = Math.hypot(e[0] - o[0], e[1] - o[1]);
    return [o[0] + ((e[0] - o[0]) / n) * 60, o[1] + ((e[1] - o[1]) / n) * 60];
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
        return bench.gizmo(probeNow.components.Name, probe.id);
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
