// Dropping a model from the FileSystem dock into the viewport: import it if needed, ask the engine
// where it lands (`viewport.drop`), and instantiate it there as one undo step.
import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import type { View } from "../viewport/camera";
import type { ProjectFile } from "./tree";

/** The drop pixel in a viewport of `width`×`height` seen from `view`. */
export interface DropPoint {
  view: View;
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Places `file` at `at`; resolves the new root entity's id. */
export async function placeAsset(session: EditorSession, file: ProjectFile, at: DropPoint): Promise<string> {
  if (file.kind !== "model") throw new Error(`only models can be placed in the scene, not ${file.path}`);
  const asset = file.asset ?? (await importFile(session, file.path));
  const { view, x, y, width, height } = at;
  const landing = await session.call<{ position: number[] }>("viewport.drop", { view, x, y, width, height });
  const placed = await session.call<{ root: string }>("prefab.instantiate", {
    asset: `asset:${asset}`,
    translation: landing.position,
    actor: EDITOR_ACTOR,
  });
  return placed.root;
}

/** Imports a project file; resolves its asset id. */
export async function importFile(session: EditorSession, path: string): Promise<string> {
  const result = await session.call<{ id: string }>("asset.import", { path, actor: EDITOR_ACTOR });
  return result.id;
}
