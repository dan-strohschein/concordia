// The Move gizmo: red/green/blue axis arrows on the selected entity. Dragging an arrow moves the
// entity along that axis through `viewport.gizmo` (engine-side; one transaction, so one undo).
// In the native shell the handles are reported to Rust, which drags them without the page.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Coalescer } from "../engine/coalesce";

import type { EditorSession } from "../engine/session";
import { project, type Vec3, type View } from "./camera";
import { inputTags } from "./latency";
import { isNative, reportGizmoHandles } from "./native";

interface Props {
  session: EditorSession;
  entity: string;
  view: View;
  width: number;
  height: number;
  actor: string;
}

/** One drawn axis handle (viewport pixels). */
export interface Handle {
  axis: "x" | "y" | "z";
  color: string;
  origin: [number, number];
  end: [number, number];
}

const AXES: { axis: Vec3; color: string; name: Handle["axis"] }[] = [
  { axis: [1, 0, 0], color: "#e05050", name: "x" },
  { axis: [0, 1, 0], color: "#60c060", name: "y" },
  { axis: [0, 0, 1], color: "#5080e0", name: "z" },
];
const LENGTH = 60;
/** Handle radius in pixels (the circle at each arrow's end). */
export const HANDLE_RADIUS = 7;

/** The arrows for an entity at `translation`, each `LENGTH` pixels along its axis on screen. */
export function handles(view: View, translation: Vec3 | undefined, width: number, height: number): Handle[] {
  if (!translation) return [];
  const origin = project(view, translation, width, height);
  if (!origin) return [];
  return AXES.flatMap(({ axis, color, name }) => {
    const tip = project(
      view,
      [translation[0] + axis[0], translation[1] + axis[1], translation[2] + axis[2]],
      width,
      height,
    );
    if (!tip) return [];
    const [dx, dy] = [tip[0] - origin[0], tip[1] - origin[1]];
    const n = Math.hypot(dx, dy);
    if (n < 1e-3) return []; // the axis points at the eye: no usable handle
    return [{ axis: name, color, origin, end: [origin[0] + (dx / n) * LENGTH, origin[1] + (dy / n) * LENGTH] }];
  });
}

interface Drag {
  latest: { x: number; y: number; seq: number; input_time_ms: number } | undefined;
  busy: boolean;
}

export function Gizmo({ session, entity, view, width, height, actor }: Props) {
  const [translation, setTranslation] = useState<Vec3>();
  const drag = useRef<Drag | undefined>(undefined);
  const svg = useRef<SVGSVGElement>(null);
  const loader = useMemo(
    () =>
      new Coalescer(() =>
        session.entity(entity).then(
          (d) => setTranslation((d.components["Transform"] as { translation?: Vec3 } | undefined)?.translation),
          () => setTranslation(undefined),
        ),
      ),
    [session, entity],
  );
  const load = useCallback(() => loader.request(), [loader]);
  useEffect(load, [load]);
  useEffect(
    () =>
      session.onEvent((e) => {
        if (drag.current === undefined && Array.isArray(e["entities"]) && e["entities"].includes(entity)) load();
      }),
    [session, entity, load],
  );
  const arrows = useMemo(() => handles(view, translation, width, height), [view, translation, width, height]);
  useEffect(() => {
    if (!isNative()) return;
    reportGizmoHandles(
      entity,
      arrows.map((a) => ({ axis: a.axis, x: a.end[0], y: a.end[1], radius: HANDLE_RADIUS + 3 })),
    );
    return () => reportGizmoHandles(entity, []);
  }, [entity, arrows]);

  /** Pointer position in viewport pixels. */
  const local = (e: { clientX: number; clientY: number }): [number, number] => {
    const box = svg.current?.getBoundingClientRect();
    return [e.clientX - (box?.left ?? 0), e.clientY - (box?.top ?? 0)];
  };
  const begin = (axis: Handle["axis"]) => (e: React.MouseEvent) => {
    e.stopPropagation();
    const [x, y] = local(e);
    const state: Drag = { latest: undefined, busy: false };
    drag.current = state;
    void session.call("viewport.gizmo", { action: "begin", entity, axis, x, y, actor });
    // Pointer positions are absolute, so only the latest one needs sending.
    const send = () => {
      const next = state.latest;
      if (state.busy || !next) return;
      state.latest = undefined;
      state.busy = true;
      session
        .call<{ translation: Vec3 | null }>("viewport.gizmo", { action: "move", ...next, width, height, actor })
        .then((r) => r.translation && setTranslation(r.translation))
        .catch(() => undefined)
        .finally(() => {
          state.busy = false;
          send();
        });
    };
    const move = (m: MouseEvent) => {
      const [mx, my] = local(m);
      state.latest = { x: mx, y: my, ...inputTags.mark(m.timeStamp) };
      send();
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      drag.current = undefined;
      void session.call("viewport.gizmo", { action: "end", actor });
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  return (
    <svg
      ref={svg}
      width={width}
      height={height}
      style={{ position: "absolute", inset: 0, pointerEvents: "none" }}
      aria-label="Move gizmo"
    >
      {arrows.map(({ axis, color, origin, end }) => (
        <g
          key={axis}
          style={{ pointerEvents: "all", cursor: "grab" }}
          onMouseDown={begin(axis)}
          aria-label={`Move ${axis}`}
          role="button"
        >
          <line x1={origin[0]} y1={origin[1]} x2={end[0]} y2={end[1]} stroke={color} strokeWidth={4} />
          <circle cx={end[0]} cy={end[1]} r={HANDLE_RADIUS} fill={color} />
        </g>
      ))}
    </svg>
  );
}
