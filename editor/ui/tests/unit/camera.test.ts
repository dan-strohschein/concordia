// The editor's orbit camera and its projection (M1 phase 1.9).
import { OrbitCamera, project, type Vec3 } from "../../src/viewport/camera";

const close = (a: Vec3, b: Vec3, eps = 1e-4) => a.every((v, i) => Math.abs(v - (b[i] ?? NaN)) < eps);

test("the view looks from eye to target at the orbit distance", () => {
  const cam = new OrbitCamera({ target: [1, 2, 3], yaw: 0, pitch: 0, distance: 10 });
  const view = cam.view();
  expect(close(view.target, [1, 2, 3])).toBe(true);
  expect(close(view.eye, [1, 2, 13])).toBe(true);
  expect("Perspective" in view.projection).toBe(true);
});

test("orbiting keeps the distance and clamps the pitch short of the poles", () => {
  const cam = new OrbitCamera({ target: [0, 0, 0], yaw: 0, pitch: 0, distance: 5 });
  cam.orbit(300, -10_000);
  const { eye } = cam.view();
  expect(Math.hypot(...eye)).toBeCloseTo(5);
  expect(cam.pitch).toBeGreaterThan(-Math.PI / 2);
});

test("panning moves the target across the screen, not along the view", () => {
  const cam = new OrbitCamera({ target: [0, 0, 0], yaw: 0, pitch: 0, distance: 10 });
  cam.pan(100, 0, 500);
  const { target } = cam.view();
  expect(target[0]).toBeLessThan(0);
  expect(target[2]).toBeCloseTo(0);
});

test("zoom scales the distance and keeps it positive", () => {
  const cam = new OrbitCamera({ target: [0, 0, 0], yaw: 0, pitch: 0, distance: 10 });
  cam.zoom(-1000);
  expect(cam.distance).toBeGreaterThan(0);
  cam.zoom(1);
  expect(cam.distance).toBeGreaterThan(0.05);
});

test("the isometric preset is orthographic at true isometric angles", () => {
  const cam = new OrbitCamera({ target: [0, 0, 0], yaw: 0, pitch: 0, distance: 10 });
  cam.preset("iso");
  const { eye, projection } = cam.view();
  expect("Orthographic" in projection).toBe(true);
  expect(Math.atan2(eye[1], Math.hypot(eye[0], eye[2])) * (180 / Math.PI)).toBeCloseTo(35.264, 2);
  cam.toggleOrtho();
  expect("Perspective" in cam.view().projection).toBe(true);
});

test("project maps the target to the image center and +x to the right", () => {
  const cam = new OrbitCamera({ target: [0, 0, 0], yaw: 0, pitch: 0, distance: 10 });
  const center = project(cam.view(), [0, 0, 0], 400, 300);
  expect(center?.[0]).toBeCloseTo(200);
  expect(center?.[1]).toBeCloseTo(150);
  const right = project(cam.view(), [1, 0, 0], 400, 300);
  expect((right?.[0] ?? 0) > 200).toBe(true);
  expect(project(cam.view(), [0, 0, 20], 400, 300)).toBeUndefined();
});

test("gizmo drags convert pointer motion along the axis into world units", async () => {
  const { axisDelta } = await import("../../src/viewport/Gizmo");
  const view = new OrbitCamera({ target: [0, 0, 0], yaw: 0, pitch: 0, distance: 10 }).view();
  const right = project(view, [1, 0, 0], 400, 300);
  const pixelsPerUnit = (right?.[0] ?? 0) - 200;
  expect(axisDelta(view, [0, 0, 0], [1, 0, 0], [pixelsPerUnit * 2, 0], 400, 300)).toBeCloseTo(2);
  expect(axisDelta(view, [0, 0, 0], [1, 0, 0], [0, 50], 400, 300)).toBeCloseTo(0);
  expect(axisDelta(view, [0, 0, 0], [0, 0, 1], [30, 30], 400, 300)).toBe(0);
});
