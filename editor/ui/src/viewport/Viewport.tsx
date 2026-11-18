// The viewport: Godot's tool strip, View menu and navigation (middle-drag orbit, Shift+middle-drag
// pan, wheel zoom; Alt+left-drag emulates the middle button), click-to-select through
// `viewport.pick`, and the Move gizmo. In a browser the picture is a streamed
// `capture.screenshot` from the editor's own view (ADR-0043 approach D).
import { useCallback, useEffect, useRef, useState } from "react";

import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import { type Preset, type Vec3, type View } from "./camera";
import { Gizmo } from "./Gizmo";
import { inputTags } from "./latency";
import { isNative, onNativeView, reportSlot } from "./native";
import { type NavParams, useNavigate } from "./navigate";

type Tool = "select" | "move";

interface Capture {
  image: { mime_type: string; data_base64: string };
}

interface Props {
  session: EditorSession;
  world: "edit" | "play";
  selected: string | undefined;
  onSelect: (id: string | undefined) => void;
}

/** Renders the latest view: streamed `capture.screenshot` in a browser, `viewport.present` into the
 * native view in the shell. At most one request in flight; the newest one queued. */
function useStreamedView(session: EditorSession, world: "edit" | "play") {
  const native = isNative();
  const [src, setSrc] = useState<string>();
  const [status, setStatus] = useState<string>();
  const state = useRef({ busy: false, next: undefined as undefined | { view: View; width: number; height: number } });
  const request = useCallback(
    (view: View, width: number, height: number) => {
      state.current.next = { view, width, height };
      const run = () => {
        const job = state.current.next;
        if (state.current.busy || job === undefined) return;
        state.current = { busy: true, next: undefined };
        const dpr = window.devicePixelRatio || 1;
        const call = native
          ? session
              .call("viewport.present", {
                view: job.view,
                width: Math.round(job.width * dpr),
                height: Math.round(job.height * dpr),
                world,
                ...inputTags.current(),
              })
              .then(() => setStatus(undefined))
          : session
              .call<Capture>("capture.screenshot", { view: job.view, width: job.width, height: job.height, world })
              .then((c) => {
                setSrc(`data:${c.image.mime_type};base64,${c.image.data_base64}`);
                setStatus(undefined);
              });
        call
          .catch((e: unknown) => setStatus(String(e)))
          .finally(() => {
            state.current.busy = false;
            run();
          });
      };
      run();
    },
    [session, world, native],
  );
  return { src, status, request };
}

export function Viewport({ session, world, selected, onSelect }: Props) {
  const native = isNative();
  const [tool, setTool] = useState<Tool>("select");
  const [view, setView] = useState<View>();
  const [size, setSize] = useState({ width: 640, height: 360 });
  const slot = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; button: number; shift: boolean; moved: boolean } | undefined>(undefined);
  const { src, status, request } = useStreamedView(session, world);
  const navigate = useNavigate(session, setView);
  const latest = useRef({ view, size });
  latest.current = { view, size };

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
  // In the native shell: keep the Metal view on the slot, and redraw every display frame while playing.
  useEffect(() => {
    if (!native || !slot.current) return;
    void reportSlot(slot.current);
  }, [native, size]);
  useEffect(() => {
    if (!native || world !== "play") return;
    let frame = requestAnimationFrame(function tick() {
      redraw();
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [native, world, redraw]);

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
    drag.current = {
      x: e.clientX,
      y: e.clientY,
      button: e.altKey && e.button === 0 ? 1 : e.button,
      shift: e.shiftKey,
      moved: false,
    };
  };
  const onMouseMove = (e: React.MouseEvent) => {
    const d = drag.current;
    if (!d) return;
    const [dx, dy] = [e.clientX - d.x, e.clientY - d.y];
    if (Math.abs(dx) + Math.abs(dy) > 3) d.moved = true;
    if (d.button !== 1) return;
    navigate(d.shift ? { pan: [dx, dy], height: size.height } : { orbit: [dx, dy] });
    [d.x, d.y] = [e.clientX, e.clientY];
  };
  const onMouseUp = (e: React.MouseEvent) => {
    const d = drag.current;
    drag.current = undefined;
    if (d && d.button === 0 && !d.moved) {
      const p = local(e);
      if (p.x >= 0 && p.y >= 0 && p.x < size.width && p.y < size.height) pick(p.x, p.y);
    }
  };
  const onWheel = (e: React.WheelEvent) => navigate({ zoom: Math.sign(e.deltaY) });
  const onViewMenu = (choice: string) => {
    if (choice === "ortho") navigate({ toggle_ortho: true });
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
        onMouseLeave={() => (drag.current = undefined)}
        onWheel={onWheel}
        onContextMenu={(e) => e.preventDefault()}
      >
        {src && (
          <img
            alt="Viewport"
            draggable={false}
            src={src}
            style={{ width: "100%", height: "100%", display: "block", pointerEvents: "none" }}
          />
        )}
        {tool === "move" && selected !== undefined && view && (
          <Gizmo
            session={session}
            entity={selected}
            view={view}
            width={size.width}
            height={size.height}
            actor={EDITOR_ACTOR}
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
