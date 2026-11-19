// The engine's view (the editor camera itself lives engine-side: `viewport.navigate`) and the
// projection the engine uses for it (look-at with +Y up, reverse-Z perspective or orthographic),
// so clicks and gizmos land exactly on what is drawn.

export type Vec3 = [number, number, number];

/** The engine's `View` (capture.screenshot / viewport.pick). */
export interface View {
  eye: Vec3;
  target: Vec3;
  projection:
    | { Perspective: { fov_y: number; near: number; far: number } }
    | { Orthographic: { height: number; near: number; far: number } };
}

export type Preset = "front" | "right" | "top" | "iso";

const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const scale = (a: Vec3, s: number): Vec3 => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const normalize = (a: Vec3): Vec3 => scale(a, 1 / (Math.hypot(...a) || 1));

/** Right, up and forward of a view (forward points from eye to target). */
export function basis(view: View): { right: Vec3; up: Vec3; forward: Vec3 } {
  const forward = normalize(sub(view.target, view.eye));
  const worldUp: Vec3 = Math.abs(forward[1]) > 0.999 ? [0, 0, 1] : [0, 1, 0];
  const right = normalize(cross(forward, worldUp));
  return { right, up: cross(right, forward), forward };
}

/** Pixel position of a world point in a width×height image, or undefined if it is not in front. */
export function project(view: View, point: Vec3, width: number, height: number): [number, number] | undefined {
  const { right, up, forward } = basis(view);
  const rel = sub(point, view.eye);
  const [x, y, z] = [dot(rel, right), dot(rel, up), dot(rel, forward)];
  const aspect = width / height;
  let nx: number;
  let ny: number;
  if ("Perspective" in view.projection) {
    const p = view.projection.Perspective;
    if (z <= p.near) return undefined;
    const t = Math.tan(p.fov_y / 2);
    [nx, ny] = [x / (z * t * aspect), y / (z * t)];
  } else {
    const o = view.projection.Orthographic;
    if (z < o.near || z > o.far) return undefined;
    [nx, ny] = [x / ((o.height * aspect) / 2), y / (o.height / 2)];
  }
  return [((nx + 1) / 2) * width, ((1 - ny) / 2) * height];
}

/** World units per pixel at the target's depth (for panning and gizmo drags). */
export function unitsPerPixel(view: View, height: number): number {
  if ("Orthographic" in view.projection) return view.projection.Orthographic.height / height;
  const distance = Math.hypot(...sub(view.target, view.eye));
  return (2 * distance * Math.tan(view.projection.Perspective.fov_y / 2)) / height;
}
