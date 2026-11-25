// Viewport tools and readouts (M1 phase 1.16): Q/W/E/R pick the tool, each gizmo tool maps to an
// engine gizmo mode, and render.stats becomes one short line.
import { VIEW_MODES, gizmoMode, statsLine, toolForKey } from "../../src/viewport/tools";

test("q w e r choose select, move, rotate and scale; modified keys do not", () => {
  expect(["q", "w", "e", "r"].map((key) => toolForKey({ key }))).toEqual(["select", "move", "rotate", "scale"]);
  expect(toolForKey({ key: "R" })).toBe("scale");
  expect(toolForKey({ key: "r", metaKey: true })).toBeUndefined();
  expect(toolForKey({ key: "r", ctrlKey: true })).toBeUndefined();
  expect(toolForKey({ key: "x" })).toBeUndefined();
});

test("only the transform tools show a gizmo", () => {
  expect(gizmoMode("select")).toBeUndefined();
  expect(gizmoMode("move")).toBe("move");
  expect(gizmoMode("rotate")).toBe("rotate");
  expect(gizmoMode("scale")).toBe("scale");
});

test("view modes list every engine mode once, lit first", () => {
  const ids = VIEW_MODES.map((m) => m.id);
  expect(ids[0]).toBe("lit");
  expect(ids).toHaveLength(18);
  expect(new Set(ids).size).toBe(18);
  for (const id of [
    "light_clusters",
    "closures",
    "haze",
    "fuzz",
    "subsurface",
    "thin_film",
    "lod",
    "meshlets",
    "triangles",
  ]) {
    expect(ids).toContain(id);
  }
});

test("the stats line shows draws, triangles and times, and waits for the GPU time", () => {
  const frame = { draws: 12, culled: 3, triangles: 45_678, cpu_ms: 1.234, gpu_ms: 2.5, memory_bytes: 268_435_456 };
  expect(statsLine(frame)).toBe("12 draws · 3 culled · 45.7k tris · CPU 1.2 ms · GPU 2.5 ms · 256 MB");
  expect(statsLine({ ...frame, gpu_ms: null, memory_bytes: null })).toBe(
    "12 draws · 3 culled · 45.7k tris · CPU 1.2 ms · GPU – ms",
  );
  expect(statsLine({ ...frame, triangles: 900 })).toContain("900 tris");
});
