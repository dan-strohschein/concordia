// Scene-tree edits, shared by the Scene dock's context menu and its keyboard shortcuts. Each is one
// Engine API call and resolves the entity to select next.
import { EDITOR_ACTOR, type EditorSession } from "../engine/session";

/** A template as `entity.templates` lists it. */
export interface Template {
  name: string;
  description: string;
  components: string[];
}

/** Spawns `template` under `parent` (top level when undefined); resolves the new id. */
export async function addEntity(session: EditorSession, template: string, parent?: string): Promise<string> {
  const params = { template, ...(parent === undefined ? {} : { parent }), actor: EDITOR_ACTOR };
  return (await session.call<{ id: string }>("entity.create", params)).id;
}

/** Copies `id` with its descendants; resolves the copy's id. */
export async function duplicateEntity(session: EditorSession, id: string): Promise<string | undefined> {
  return (await session.call<{ ids: string[] }>("entity.duplicate", { ids: [id], actor: EDITOR_ACTOR })).ids[0];
}

/** Removes `id` and its descendants (Godot's Delete). */
export async function deleteEntity(session: EditorSession, id: string): Promise<undefined> {
  await session.call("entity.despawn", { id, recursive: true, actor: EDITOR_ACTOR });
  return undefined;
}

/** The scene's templates, fetched once per session. */
export async function templates(session: EditorSession): Promise<Template[]> {
  return (await session.call<{ templates: Template[] }>("entity.templates")).templates;
}
