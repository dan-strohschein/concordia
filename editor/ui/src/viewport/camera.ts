// The editor's orbit camera (Godot-style) and the projection the engine uses for an explicit
// `view` (look-at with +Y up, reverse-Z perspective or orthographic), so clicks and gizmos land
// exactly on what `capture.screenshot` draws.

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

const FOV = (75 * Math.PI) / 180;
const PITCH_LIMIT = Math.PI / 2 - 0.01;
const ISO_PITCH = -(35.264 * Math.PI) / 180;

const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
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

export interface OrbitState {
  target: Vec3;
  yaw: number;
  pitch: number;
  distance: number;
  ortho?: boolean;
}

/** Where the editor's camera starts. */
export const DEFAULT_CAMERA: OrbitState = { target: [0, 1, 0], yaw: 0.6, pitch: -0.45, distance: 14 };

/** Orbits a target: middle-drag orbits, Shift+middle-drag pans, the wheel zooms (Godot). */
export class OrbitCamera {
  target: Vec3;
  yaw: number;
  pitch: number;
  distance: number;
  ortho: boolean;

  constructor(state: OrbitState) {
    this.target = [...state.target];
    this.yaw = state.yaw;
    this.pitch = Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, state.pitch));
    this.distance = Math.max(0.05, state.distance);
    this.ortho = state.ortho ?? false;
  }

  /** The engine `View` for this camera. */
  view(): View {
    const dir: Vec3 = [
      Math.sin(this.yaw) * Math.cos(this.pitch),
      -Math.sin(this.pitch),
      Math.cos(this.yaw) * Math.cos(this.pitch),
    ];
    const eye = add(this.target, scale(dir, this.distance));
    const near = Math.max(0.01, this.distance * 0.001);
    const far = Math.max(1000, this.distance * 100);
    const projection: View["projection"] = this.ortho
      ? { Orthographic: { height: 2 * this.distance * Math.tan(FOV / 2), near: 0, far: far + this.distance } }
      : { Perspective: { fov_y: FOV, near, far } };
    return { eye, target: [...this.target], projection };
  }

  orbit(dx: number, dy: number): void {
    this.yaw -= dx * 0.005;
    this.pitch = Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, this.pitch - dy * 0.005));
  }

  pan(dx: number, dy: number, viewportHeight: number): void {
    const view = this.view();
    const { right, up } = basis(view);
    const units = unitsPerPixel(view, Math.max(1, viewportHeight));
    this.target = add(this.target, add(scale(right, -dx * units), scale(up, dy * units)));
  }

  /** Positive steps zoom out, negative zoom in. */
  zoom(steps: number): void {
    this.distance = Math.max(0.05, Math.min(1e5, this.distance * Math.pow(1.1, steps)));
  }

  focus(point: Vec3): void {
    this.target = [...point];
  }

  toggleOrtho(): void {
    this.ortho = !this.ortho;
  }

  preset(name: Preset): void {
    switch (name) {
      case "front":
        [this.yaw, this.pitch] = [0, 0];
        break;
      case "right":
        [this.yaw, this.pitch] = [Math.PI / 2, 0];
        break;
      case "top":
        [this.yaw, this.pitch] = [0, -PITCH_LIMIT];
        break;
      case "iso":
        [this.yaw, this.pitch, this.ortho] = [Math.PI / 4, ISO_PITCH, true];
        break;
    }
  }
}
