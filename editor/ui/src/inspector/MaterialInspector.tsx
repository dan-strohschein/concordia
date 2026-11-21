// The Inspector for a `.material.ron` (M2 phase 2.3c): the material's fields from the engine's
// `Material` schema, saved with `material.set`. An edit the engine refuses stays as a draft with
// the reason until the material is valid again (or is reverted), so multi-step changes work.
// Render it with `key={path}` so another material starts without the previous draft.
import { useCallback, useEffect, useState } from "react";

import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import { ValueEditor } from "./fields";
import { widgetFor, type Widget } from "./widgets";

interface Loaded {
  material: unknown;
  migrated: boolean;
  widget: Widget;
}

async function load(session: EditorSession, path: string): Promise<Loaded> {
  const [read, schema] = await Promise.all([
    session.call<{ material: unknown; migrated: boolean }>("material.get", { path }),
    session.schema("Material") as Promise<Record<string, unknown>>,
  ]);
  const defs = (schema["$defs"] as Record<string, unknown> | undefined) ?? {};
  return { material: read.material, migrated: read.migrated, widget: widgetFor(schema, defs) };
}

/** The engine's reason, without the JSON-RPC wrapping. */
const reason = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export function MaterialInspector({ session, path }: { session: EditorSession; path: string }) {
  const [loaded, setLoaded] = useState<Loaded>();
  const [draft, setDraft] = useState<unknown>();
  const [error, setError] = useState<string>();
  const refresh = useCallback(() => {
    load(session, path).then(setLoaded, (e: unknown) => setError(reason(e)));
  }, [session, path]);
  useEffect(refresh, [refresh]);
  useEffect(
    () =>
      session.onEvent((event) => {
        if (event.topic === "asset" && event["path"] === path) refresh();
      }),
    [session, path, refresh],
  );
  if (loaded === undefined) {
    return <p style={{ opacity: 0.6, padding: 8 }}>{error ?? "Loading…"}</p>;
  }
  const commit = (material: unknown) => {
    setDraft(material);
    session.call("material.set", { path, material, actor: EDITOR_ACTOR }).then(
      () => {
        setLoaded({ ...loaded, material, migrated: false });
        setDraft(undefined);
        setError(undefined);
      },
      (e: unknown) => setError(reason(e)),
    );
  };
  const revert = () => {
    setDraft(undefined);
    setError(undefined);
  };
  return (
    <div style={{ padding: 4 }}>
      <div style={{ background: "#333", padding: "2px 6px", display: "flex", gap: 6 }}>
        <span aria-hidden>🎨</span>
        <span style={{ flex: 1 }}>{path}</span>
        {draft !== undefined && <button onClick={revert}>Revert</button>}
      </div>
      {loaded.migrated && (
        <p style={{ opacity: 0.7, margin: "4px 6px" }}>An M1 material: saving rewrites it as a layered material.</p>
      )}
      {error !== undefined && (
        <p role="alert" style={{ color: "#f88", margin: "4px 6px" }}>
          {error}
        </p>
      )}
      <div style={{ padding: "4px 6px" }}>
        <ValueEditor label="Material" value={draft ?? loaded.material} widget={loaded.widget} onCommit={commit} />
      </div>
    </div>
  );
}
