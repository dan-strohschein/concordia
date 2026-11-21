// The Inspector dock: one collapsible section per component, generated from the engine's schemas.
import { useCallback, useEffect, useMemo, useState } from "react";

import { Coalescer } from "../engine/coalesce";

import type { EditorSession } from "../engine/session";
import { AddComponent } from "./AddComponent";
import { removeComponent } from "./components";
import { ValueEditor } from "./fields";
import { MaterialInspector } from "./MaterialInspector";
import { label, widgetFor, type Widget } from "./widgets";

interface Section {
  name: string;
  value: unknown;
  widget: Widget;
}

async function load(session: EditorSession, entity: string): Promise<Section[]> {
  const data = await session.entity(entity);
  const names = Object.keys(data.components).sort();
  return Promise.all(
    names.map(async (name) => {
      const schema = (await session.schema(name)) as Record<string, unknown>;
      const defs = (schema["$defs"] as Record<string, unknown> | undefined) ?? {};
      return { name, value: data.components[name], widget: widgetFor(schema, defs) };
    }),
  );
}

/** Shows the selected material file if there is one, otherwise the selected entity's components. */
export function Inspector(props: {
  session: EditorSession;
  entity: string | undefined;
  material?: string | undefined;
}) {
  return props.material === undefined ? (
    <EntityInspector session={props.session} entity={props.entity} />
  ) : (
    <MaterialInspector key={props.material} session={props.session} path={props.material} />
  );
}

function EntityInspector({ session, entity }: { session: EditorSession; entity: string | undefined }) {
  const [sections, setSections] = useState<Section[]>([]);
  const [error, setError] = useState<string>();
  // Coalesced: a drag's stream of change events costs two fetches, not one per event.
  const loader = useMemo(
    () =>
      new Coalescer(async () => {
        if (entity === undefined) return;
        await load(session, entity).then(setSections, (e: unknown) => setError(String(e)));
      }),
    [session, entity],
  );
  const refresh = useCallback(() => loader.request(), [loader]);
  useEffect(refresh, [refresh]);
  useEffect(
    () =>
      session.onEvent((event) => {
        const touched = Array.isArray(event["entities"]) && entity !== undefined && event["entities"].includes(entity);
        if (touched || event.topic === "scene") refresh();
      }),
    [session, entity, refresh],
  );
  if (entity === undefined) return <p style={{ opacity: 0.6, padding: 8 }}>Select an entity.</p>;
  const commit = (name: string) => (value: unknown) => {
    session.setComponent(entity, name, value).then(refresh, (e: unknown) => setError(String(e)));
  };
  const remove = (name: string) => (e: React.MouseEvent) => {
    e.preventDefault(); // a button in <summary> would otherwise toggle the section
    removeComponent(session, entity, name).catch((err: unknown) => setError(String(err)));
  };
  return (
    <div style={{ padding: 4 }}>
      {error && (
        <p role="alert" style={{ color: "#f88" }}>
          {error}
        </p>
      )}
      {sections.map((s) => (
        <details key={s.name} open role="group" aria-label={s.name} style={{ marginBottom: 6 }}>
          <summary style={{ background: "#333", padding: "2px 6px", cursor: "pointer" }}>
            <span>{s.name}</span>
            <button
              type="button"
              aria-label={`Remove ${s.name}`}
              title="Remove Component"
              onClick={remove(s.name)}
              style={{ float: "right" }}
            >
              ×
            </button>
          </summary>
          <div style={{ padding: "4px 6px" }}>
            <ValueEditor label={label(s.name)} value={s.value} widget={s.widget} onCommit={commit(s.name)} />
          </div>
        </details>
      ))}
      <AddComponent session={session} entity={entity} present={sections.map((s) => s.name)} onError={setError} />
    </div>
  );
}
