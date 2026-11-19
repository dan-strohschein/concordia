// The projection the engine uses for a view (M1 phase 1.9; the orbit camera's tests moved to
// crates/concordia-render/tests/orbit.rs with the camera in 1.9d).
import { project, type View } from "../../src/viewport/camera";

/** What the editor camera's view is at target 0, yaw 0, pitch 0, distance 10. */
const front: View = {
  eye: [0, 0, 10],
  target: [0, 0, 0],
  projection: { Perspective: { fov_y: (75 * Math.PI) / 180, near: 0.01, far: 1000 } },
};
test("project maps the target to the image center and +x to the right", () => {
  const center = project(front, [0, 0, 0], 400, 300);
  expect(center?.[0]).toBeCloseTo(200);
  expect(center?.[1]).toBeCloseTo(150);
  const right = project(front, [1, 0, 0], 400, 300);
  expect((right?.[0] ?? 0) > 200).toBe(true);
  expect(project(front, [0, 0, 20], 400, 300)).toBeUndefined();
});
