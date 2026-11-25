// Viewport tools (Godot's Q/W/E/R), the engine's view modes, and the render stats readout.

import type { ViewMode } from "../api/generated/MethodCaptureScreenshotParams";

/** The viewport's pointer tool. */
export type Tool = "select" | "move" | "rotate" | "scale";
/** An engine gizmo mode (`viewport.overlays` / `viewport.gizmo`). */
export type GizmoMode = "move" | "rotate" | "scale";

const KEYS: Record<string, Tool> = { q: "select", w: "move", e: "rotate", r: "scale" };

/** The tool a key press picks (Godot's Q, W, E, R), ignoring presses with ⌘ or Ctrl. */
export function toolForKey(e: { key: string; metaKey?: boolean; ctrlKey?: boolean }): Tool | undefined {
  if (e.metaKey === true || e.ctrlKey === true) return undefined;
  return KEYS[e.key.toLowerCase()];
}

/** The gizmo a tool shows, if any. */
export function gizmoMode(tool: Tool): GizmoMode | undefined {
  return tool === "select" ? undefined : tool;
}

/** The engine's view modes (`ViewMode`), as the View menu lists them. */
export const VIEW_MODES = [
  { id: "lit", label: "Lit" },
  { id: "unlit", label: "Unlit" },
  { id: "wireframe", label: "Wireframe" },
  { id: "normals", label: "Normals" },
  { id: "albedo", label: "Albedo" },
  { id: "roughness_metallic", label: "Roughness / Metallic" },
  { id: "overdraw", label: "Overdraw" },
  { id: "shadow_cascades", label: "Shadow Cascades" },
  { id: "light_clusters", label: "Lights per Cluster" },
  { id: "closures", label: "Material Closures" },
  { id: "haze", label: "Haze" },
  { id: "fuzz", label: "Fuzz" },
  { id: "subsurface", label: "Subsurface" },
  { id: "thin_film", label: "Thin Film" },
  { id: "lod", label: "LOD" },
  { id: "meshlets", label: "Meshlets" },
  { id: "triangles", label: "Triangles" },
  { id: "motion", label: "Motion Vectors" },
] as const;
export type ViewModeId = (typeof VIEW_MODES)[number]["id"];

/** True only while the menu lists exactly the engine's view modes (checked by `tsc`). */
type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : never) : never;
export const VIEW_MODES_MATCH_THE_ENGINE: Same<ViewModeId, ViewMode> = true;

/** The parts of `render.stats`' frame the readout shows. */
export interface FrameStats {
  draws: number;
  culled: number;
  triangles: number;
  cpu_ms: number;
  gpu_ms: number | null;
  memory_bytes: number | null;
}

const count = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n));

/** One short line: draws, culled, triangles, CPU and GPU time, and GPU memory when known. */
export function statsLine(f: FrameStats): string {
  const parts = [
    `${f.draws} draws`,
    `${f.culled} culled`,
    `${count(f.triangles)} tris`,
    `CPU ${f.cpu_ms.toFixed(1)} ms`,
    `GPU ${f.gpu_ms === null ? "–" : f.gpu_ms.toFixed(1)} ms`,
  ];
  if (f.memory_bytes !== null) parts.push(`${Math.round(f.memory_bytes / 2 ** 20)} MB`);
  return parts.join(" · ");
}
