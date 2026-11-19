// Component discovery for the Inspector: the engine's component index and the Add Component choices.
import { EDITOR_ACTOR, type EditorSession } from "../engine/session";

/** A row of `schema.get {}`'s `components` list. */
export interface ComponentRow {
  name: string;
  category: string | null;
  description: string;
  addable: boolean;
}

/** The registered components, as the engine lists them. */
export async function componentRows(session: EditorSession): Promise<ComponentRow[]> {
  const index = await session.call<{ schema: { components?: ComponentRow[] } }>("schema.get", {});
  return index.schema.components ?? [];
}

/** Addable components not in `present` whose name or category matches `query`, by category then name. */
export function componentChoices(
  rows: readonly ComponentRow[],
  present: readonly string[],
  query: string,
): ComponentRow[] {
  const q = query.trim().toLowerCase();
  const matches = (r: ComponentRow) => r.name.toLowerCase().includes(q) || (r.category ?? "").toLowerCase().includes(q);
  const key = (r: ComponentRow) => `${r.category === null ? "￿" : r.category}\u0000${r.name}`;
  return rows
    .filter((r) => r.addable && !present.includes(r.name) && matches(r))
    .sort((a, b) => key(a).localeCompare(key(b)));
}

/** Adds `component` with its default value (one undo step). */
export async function addComponent(session: EditorSession, entity: string, component: string): Promise<void> {
  await session.call("component.add", { entity, component, actor: EDITOR_ACTOR });
}

/** Removes `component` (one undo step). */
export async function removeComponent(session: EditorSession, entity: string, component: string): Promise<void> {
  await session.call("component.remove", { entity, component, actor: EDITOR_ACTOR });
}
