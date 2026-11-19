// The viewport: Godot's tool strip, View menu and navigation (middle-drag orbit, Shift+middle-drag
// pan, wheel zoom; Alt+left-drag emulates the middle button), click-to-select through
// `viewport.pick`, and the Move gizmo. In a browser the picture is a streamed
// `capture.screenshot` from the editor's own view (ADR-0043 approach D).
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import { type Preset, type Vec3, type View } from "./camera";
import { FILE_DRAG_TYPE } from "../files/FileSystem";
import { placeAsset } from "../files/place";
import type { ProjectFile } from "../files/tree";
import { FrameQueue } from "./frameQueue";
import { GizmoDrag } from "./gizmoDrag";
import { inputTags } from "./latency";
import { isNative, onNativeView, reportSlot } from "./native";
import { type NavParams, useNavigate } from "./navigate";

type Tool = "select" | "move";

/** A mouse press in the viewport: where, which button, and whether it grabbed the gizmo. */
interface Drag {
  x: number;
  y: number;
  button: number;
  shift: boolean;
  moved: boolean;
  grab: Promise<boolean> | undefined;
}

interface Capture {
  image: { mime_type: string; data_base64: string };
}

interface Props {
  session: EditorSession;
  world: "edit" | "play";
  selected: string | undefined;
  onSelect: (id: string | undefined) => void;
  /** Draw the ground grid (View menu). */
  grid: boolean;
  onToggleGrid: () => void;
}

/** Renders the latest view: streamed `capture.screenshot` in a browser, `viewport.present` into the
 * native view in the shell, through a `FrameQueue`. */
function useStreamedView(session: EditorSession, world: "edit" | "play") {
  const native = isNative();
  const [src, setSrc] = useState<string>();
  const [status, setStatus] = useState<string>();
  const queue = useMemo(
    () =>
      new FrameQueue<Frame>((job) => {
        const dpr = window.devicePixelRatio || 1;
        const call = native
          ? session
              .call("viewport.present", {
                view: job.view,
                width: Math.round(job.width * dpr),
                height: Math.round(job.height * dpr),
                world: job.world,
                ...inputTags.current(),
              })
              .then(() => setStatus(undefined))
          : session
              .call<Capture>("capture.screenshot", {
                view: job.view,
                width: job.width,
                height: job.height,
                world: job.world,
                overlays: true,
              })
              .then((c) => {
                setSrc(`data:${c.image.mime_type};base64,${c.image.data_base64}`);
                setStatus(undefined);
              });
        return call.catch((e: unknown) => setStatus(String(e)));
      }),
    [session, native],
  );
  const request = useCallback(
    (view: View, width: number, height: number) => queue.request({ view, width, height, world }),
    [queue, world],
  );
  return { src, status, request };
}

/** One viewport frame to draw. */
interface Frame {
  view: View;
  width: number;
  height: number;
  world: "edit" | "play";
}

export function Viewport({ session, world, selected, onSelect, grid, onToggleGrid }: Props) {
  const native = isNative();
  const [tool, setTool] = useState<Tool>("select");
  const [view, setView] = useState<View>();
  const [size, setSize] = useState({ width: 640, height: 360 });
  const slot = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | undefined>(undefined);
  const gizmo = useMemo(() => new GizmoDrag(session, EDITOR_ACTOR), [session]);
  const { src, status, request } = useStreamedView(session, world);
  const navigate = useNavigate(session, setView);
  const latest = useRef({ view, size });
  useEffect(() => {
    latest.current = { view, size };
  }, [view, size]);

  useEffect(() => navigate({}), [navigate]);
  useEffect(() => (native ? onNativeView(setView) : undefined), [native]);
  useEffect(() => {
    const element = slot.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      const box = entry?.contentRect;
      if (box) setSize({ width: Math.max(16, Math.round(box.width)), height: Math.max(16, Math.round(box.height)) });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  // Redraw on resize, world changes and, in a browser, camera moves. Natively the engine redraws
  // camera moves itself (`viewport.navigate`).
  const redraw = useCallback(() => {
    const { view: v, size: s } = latest.current;
    if (v) request(v, s.width, s.height);
  }, [request]);
  const hasView = view !== undefined;
  useEffect(redraw, [redraw, size, hasView]);
  useEffect(() => {
    if (!native) redraw();
  }, [native, redraw, view]);
  useEffect(() => session.onEvent(redraw), [session, redraw]);
  // The engine draws the selection and the Move gizmo (ADR-0047); the page only says what to show.
  useEffect(() => {
    const overlays = {
      selection: selected === undefined ? [] : [selected],
      gizmo: tool === "move" && selected !== undefined ? { entity: selected } : null,
      grid,
      scale: window.devicePixelRatio || 1,
    };
    void session.call("viewport.overlays", overlays).then(() => (native ? undefined : redraw()));
  }, [session, selected, tool, grid, native, redraw]);
  // In the native shell: keep the Metal view on the slot (while playing, the engine redraws it each tick).
  useEffect(() => {
    if (!native || !slot.current) return;
    void reportSlot(slot.current);
  }, [native, size]);

  const focusSelection = useCallback(async () => {
    if (selected === undefined) return;
    const data = await session.entity(selected);
    const t = (data.components["Transform"] as { translation?: Vec3 } | undefined)?.translation;
    if (t) navigate({ focus: t });
  }, [session, selected, navigate]);

  useEffect(() => {
    const keys = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
      if (e.key === "q") setTool("select");
      if (e.key === "w") setTool("move");
      if (e.key === "f") void focusSelection();
      const byCode: Record<string, NavParams> = {
        Numpad1: { preset: "front" },
        Numpad3: { preset: "right" },
        Numpad7: { preset: "top" },
        Numpad5: { toggle_ortho: true },
      };
      const move = byCode[e.code];
      if (move) navigate(move);
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, [focusSelection, navigate]);

  const pick = (x: number, y: number) => {
    if (!view) return;
    session
      .call<{ entity: string | null }>("viewport.pick", { view, x, y, width: size.width, height: size.height, world })
      .then(
        (hit) => onSelect(hit.entity ?? undefined),
        () => onSelect(undefined),
      );
  };
  const local = (e: React.MouseEvent) => {
    const box = slot.current?.getBoundingClientRect();
    return { x: Math.floor(e.clientX - (box?.left ?? 0)), y: Math.floor(e.clientY - (box?.top ?? 0)) };
  };
  const onMouseDown = (e: React.MouseEvent) => {
    const button = e.altKey && e.button === 0 ? 1 : e.button;
    // In a browser, a left press may grab the engine-drawn gizmo (natively the shell does this).
    const grab =
      !native && button === 0 && tool === "move" && selected !== undefined
        ? gizmo.start(local(e).x, local(e).y, size)
        : undefined;
    drag.current = { x: e.clientX, y: e.clientY, button, shift: e.shiftKey, moved: false, grab };
  };
  const onMouseMove = (e: React.MouseEvent) => {
    const d = drag.current;
    if (!d) return;
    const [dx, dy] = [e.clientX - d.x, e.clientY - d.y];
    if (Math.abs(dx) + Math.abs(dy) > 3) d.moved = true;
    if (d.grab) gizmo.move(local(e).x, local(e).y, e.timeStamp);
    if (d.button !== 1) return;
    navigate(d.shift ? { pan: [dx, dy], height: size.height } : { orbit: [dx, dy] });
    [d.x, d.y] = [e.clientX, e.clientY];
  };
  const release = (d: Drag | undefined) => {
    if (d?.grab) void d.grab.then(() => gizmo.end());
  };
  const onMouseUp = (e: React.MouseEvent) => {
    const d = drag.current;
    drag.current = undefined;
    release(d);
    if (d && d.button === 0 && !d.moved) {
      const p = local(e);
      if (p.x >= 0 && p.y >= 0 && p.x < size.width && p.y < size.height) pick(p.x, p.y);
    }
  };
  const onWheel = (e: React.WheelEvent) => navigate({ zoom: Math.sign(e.deltaY) });
  const onViewMenu = (choice: string) => {
    if (choice === "grid") onToggleGrid();
    else if (choice === "ortho") navigate({ toggle_ortho: true });
    else if (choice === "focus") void focusSelection();
    else if (choice !== "") navigate({ preset: choice as Preset });
  };

  return (
    <div style={{ display: "grid", gridTemplateRows: "28px 1fr", height: "100%", minHeight: 0 }}>
      <div
        role="toolbar"
        aria-label="Viewport tools"
        style={{ display: "flex", gap: 4, alignItems: "center", padding: "0 6px", background: "#262626" }}
      >
        <button aria-pressed={tool === "select"} onClick={() => setTool("select")} title="Select (Q)">
          Select
        </button>
        <button aria-pressed={tool === "move"} onClick={() => setTool("move")} title="Move (W)">
          Move
        </button>
        <span style={{ flex: 1 }} />
        <select aria-label="View" value="" onChange={(e) => onViewMenu(e.target.value)}>
          <option value="">View ▾</option>
          <option value="grid">Toggle grid</option>
          <option value="ortho">Toggle orthographic (Num 5)</option>
          <option value="front">Front (Num 1)</option>
          <option value="right">Right (Num 3)</option>
          <option value="top">Top (Num 7)</option>
          <option value="iso">Isometric</option>
          <option value="focus">Focus selection (F)</option>
        </select>
      </div>
      <div
        ref={slot}
        id="viewport-slot"
        data-testid="viewport"
        style={{
          position: "relative",
          minHeight: 0,
          background: isNative() ? "transparent" : "#111",
          overflow: "hidden",
          userSelect: "none",
        }}
        onMouseDown={onMouseDown}
        onMouseMove={onMouseMove}
        onMouseUp={onMouseUp}
        onMouseLeave={() => {
          release(drag.current);
          drag.current = undefined;
        }}
        onWheel={onWheel}
        onContextMenu={(e) => e.preventDefault()}
        onDragOver={(e) => {
          if (world !== "edit" || !e.dataTransfer.types.includes(FILE_DRAG_TYPE)) return;
          e.preventDefault();
          e.dataTransfer.dropEffect = "copy";
        }}
        onDrop={(e) => {
          const data = e.dataTransfer.getData(FILE_DRAG_TYPE);
          if (world !== "edit" || data === "" || !view) return;
          e.preventDefault();
          const p = local(e);
          const x = Math.min(Math.max(p.x, 0), size.width - 1);
          const y = Math.min(Math.max(p.y, 0), size.height - 1);
          placeAsset(session, JSON.parse(data) as ProjectFile, { view, x, y, ...size }).then(onSelect, (err: unknown) =>
            console.warn("drop failed", err),
          );
        }}
      >
        {src && (
          <img
            alt="Viewport"
            draggable={false}
            src={src}
            style={{ width: "100%", height: "100%", display: "block", pointerEvents: "none" }}
          />
        )}
        {status && <p style={{ position: "absolute", bottom: 4, left: 8, color: "#f88" }}>{status}</p>}
        <span style={{ position: "absolute", top: 6, left: 8, opacity: 0.7 }}>
          {world === "play" ? "Playing · " : ""}
          {view && "Orthographic" in view.projection ? "Orthographic" : "Perspective"}
        </span>
      </div>
    </div>
  );
}
