// The Move gizmo: red/green/blue axis arrows on the selected entity. Dragging an arrow moves the
// entity along that axis; the whole drag is one transaction, so one undo restores it.
import { useCallback, useEffect, useRef, useState } from "react";

import type { EditorSession } from "../engine/session";
import { project, type Vec3, type View } from "./camera";
import { inputTags } from "./latency";

interface Props {
  session: EditorSession;
  entity: string;
  view: View;
  width: number;
  height: number;
  actor: string;
}

interface TransformValue {
  translation: Vec3;
  rotation: [number, number, number, number];
  scale: Vec3;
}

const AXES: { axis: Vec3; color: string; name: string }[] = [
  { axis: [1, 0, 0], color: "#e05050", name: "x" },
  { axis: [0, 1, 0], color: "#60c060", name: "y" },
  { axis: [0, 0, 1], color: "#5080e0", name: "z" },
];

/** World amount to move along `axis` for a pointer delta, from the axis' on-screen direction. */
export function axisDelta(
  view: View,
  origin: Vec3,
  axis: Vec3,
  delta: [number, number],
  width: number,
  height: number,
): number {
  const a = project(view, origin, width, height);
  const b = project(view, [origin[0] + axis[0], origin[1] + axis[1], origin[2] + axis[2]], width, height);
  if (!a || !b) return 0;
  const s: [number, number] = [b[0] - a[0], b[1] - a[1]];
  const length2 = s[0] * s[0] + s[1] * s[1];
  return length2 < 1e-6 ? 0 : (delta[0] * s[0] + delta[1] * s[1]) / length2;
}

export function Gizmo({ session, entity, view, width, height, actor }: Props) {
  const [transform, setTransform] = useState<TransformValue>();
  const drag = useRef<{ axis: Vec3; start: TransformValue; x: number; y: number; txn: string | undefined } | undefined>(
    undefined,
  );
  const load = useCallback(() => {
    session.entity(entity).then(
      (d) => setTransform(d.components["Transform"] as TransformValue | undefined),
      () => setTransform(undefined),
    );
  }, [session, entity]);
  useEffect(load, [load]);
  useEffect(
    () =>
      session.onEvent((e) => {
        if (drag.current === undefined && Array.isArray(e["entities"]) && e["entities"].includes(entity)) load();
      }),
    [session, entity, load],
  );
  if (!transform) return null;
  const origin = project(view, transform.translation, width, height);
  if (!origin) return null;
  const length = 60;

  const begin = (axis: Vec3) => (e: React.MouseEvent) => {
    e.stopPropagation();
    const state = { axis, start: transform, x: e.clientX, y: e.clientY, txn: undefined as string | undefined };
    drag.current = state;
    session.call<{ txn: string }>("txn.begin", { label: "Move", actor }).then(
      (r) => (state.txn = r.txn),
      () => undefined,
    );
    const move = (m: MouseEvent) => {
      inputTags.mark(m.timeStamp);
      const t = axisDelta(
        view,
        state.start.translation,
        axis,
        [m.clientX - state.x, m.clientY - state.y],
        width,
        height,
      );
      const translation = state.start.translation.map((v, i) => v + (axis[i] ?? 0) * t) as Vec3;
      const next = { ...state.start, translation };
      setTransform(next);
      void session.call("component.set", { entity, component: "Transform", value: next, actor });
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      drag.current = undefined;
      if (state.txn !== undefined) void session.call("txn.commit", { txn: state.txn, actor });
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  return (
    <svg
      width={width}
      height={height}
      style={{ position: "absolute", inset: 0, pointerEvents: "none" }}
      aria-label="Move gizmo"
    >
      {AXES.map(({ axis, color, name }) => {
        const tip = project(
          view,
          [transform.translation[0] + axis[0], transform.translation[1] + axis[1], transform.translation[2] + axis[2]],
          width,
          height,
        );
        if (!tip) return null;
        const [dx, dy] = [tip[0] - origin[0], tip[1] - origin[1]];
        const n = Math.hypot(dx, dy) || 1;
        const end = [origin[0] + (dx / n) * length, origin[1] + (dy / n) * length];
        return (
          <g
            key={name}
            style={{ pointerEvents: "all", cursor: "grab" }}
            onMouseDown={begin(axis)}
            aria-label={`Move ${name}`}
            role="button"
          >
            <line x1={origin[0]} y1={origin[1]} x2={end[0]} y2={end[1]} stroke={color} strokeWidth={4} />
            <circle cx={end[0]} cy={end[1]} r={7} fill={color} />
          </g>
        );
      })}
    </svg>
  );
}
