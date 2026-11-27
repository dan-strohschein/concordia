// The graph panel (M4 4.11a, K1–K9): a visual editor over a `.graph.ron` file, and a pure Engine
// API client. It reads with `graph.read` and `graph.list_nodes`, re-reads on `edit` events (so an
// agent's edits show as they land), and every gesture is one `graph.apply` as `human:local`.
import { ReactFlow, Background, Controls, type Edge, type Node } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useCallback, useEffect, useMemo, useState } from "react";

import { Coalescer } from "../engine/coalesce";
import { EDITOR_ACTOR, type EditorSession } from "../engine/session";
import { GraphActionsContext, GraphNode } from "./GraphNode";
import {
  connectEdit,
  disconnectEdit,
  toFlow,
  type Diagnostic,
  type GraphEdit,
  type GraphJson,
  type Library,
} from "./model";

interface Reading {
  text: string;
  graph: GraphJson;
  diagnostics: Diagnostic[];
  authors: Record<string, string>;
}

const nodeTypes = { graph: GraphNode };

/** `graphs/enemy.graph.ron` → `enemy` (the module name). */
function moduleOf(path: string): string {
  return (path.split("/").at(-1) ?? path).replace(/\.graph\.ron$/, "");
}

export function GraphPanel({
  session,
  path,
  roleColor = () => undefined,
}: {
  session: EditorSession;
  path: string;
  roleColor?: (actor: string) => string | undefined;
}) {
  const [reading, setReading] = useState<Reading>();
  const [library, setLibrary] = useState<Library>({});
  const [error, setError] = useState<string>();
  const [showText, setShowText] = useState(false);
  const [query, setQuery] = useState("");
  const [traceEntity, setTraceEntity] = useState("");
  const [executed, setExecuted] = useState<ReadonlySet<string>>(new Set());

  useEffect(() => {
    let stopped = false;
    session
      .call<{ nodes: Library }>("graph.list_nodes", {})
      .then((r) => (stopped ? undefined : setLibrary(r.nodes)))
      .catch((e: unknown) => setError(String(e)));
    // Read now, then again whenever an edit lands (anyone's): no polling.
    const reader = new Coalescer(async () => {
      try {
        const r = await session.call<Reading>("graph.read", { path });
        if (!stopped) {
          setReading(r);
          setError(undefined);
        }
      } catch (e) {
        if (!stopped) setError(String(e));
      }
    });
    reader.request();
    const unsubscribe = session.onEvent((e) =>
      e.topic === "edit" || e.topic === "scene" ? reader.request() : undefined,
    );
    return () => {
      stopped = true;
      unsubscribe();
    };
  }, [session, path]);

  const apply = useCallback(
    (edits: GraphEdit[]) => {
      if (edits.length === 0) return;
      session.call("graph.apply", { path, edits, actor: EDITOR_ACTOR }).catch((e: unknown) => setError(String(e)));
    },
    [session, path],
  );

  const flow = useMemo(
    () =>
      reading
        ? toFlow(reading.graph, library, reading.diagnostics, reading.authors, executed)
        : { nodes: [], edges: [] },
    [reading, library, executed],
  );
  const overall = reading?.diagnostics.filter((d) => d.node_id == null) ?? [];
  const matches =
    query.trim() === ""
      ? []
      : Object.keys(library)
          .filter((op) => op.includes(query.trim()))
          .slice(0, 12);

  const trace = async () => {
    try {
      const r = await session.call<{ runs: { jobs: { executed: string[] }[] }[] }>("graph.trace", {
        module: moduleOf(path),
        entity: traceEntity,
        ticks: 1,
      });
      setExecuted(new Set(r.runs.flatMap((run) => run.jobs.flatMap((j) => j.executed))));
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <GraphActionsContext.Provider value={{ apply, roleColor }}>
      <div style={{ display: "flex", flexDirection: "column", height: "100%" }} aria-label="Graph">
        <div style={{ display: "flex", gap: 6, padding: 4, alignItems: "center", fontSize: 12 }}>
          <strong>{path}</strong>
          <span style={{ position: "relative" }}>
            <input
              role="searchbox"
              aria-label="add node"
              placeholder="add node…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            {matches.length > 0 && (
              <ul
                role="listbox"
                aria-label="nodes"
                style={{
                  position: "absolute",
                  zIndex: 10,
                  margin: 0,
                  padding: 2,
                  listStyle: "none",
                  background: "#222",
                }}
              >
                {matches.map((op) => (
                  <li
                    key={op}
                    role="option"
                    aria-selected={false}
                    title={library[op]?.docs}
                    onClick={() => {
                      apply([{ AddNode: { id: null, op, at: null } }]);
                      setQuery("");
                    }}
                    style={{ cursor: "pointer", padding: "1px 4px" }}
                  >
                    {op}
                  </li>
                ))}
              </ul>
            )}
          </span>
          <button aria-pressed={showText} onClick={() => setShowText((s) => !s)}>
            view as text
          </button>
          <input
            aria-label="trace entity"
            placeholder="entity id"
            value={traceEntity}
            onChange={(e) => setTraceEntity(e.target.value)}
          />
          <button
            onClick={() => void trace()}
            disabled={traceEntity.trim() === ""}
            title="Steps play one tick and shows what ran"
          >
            trace
          </button>
        </div>
        {(overall.length > 0 || error !== undefined) && (
          <div role="alert" style={{ color: "#e5484d", padding: "0 4px", fontSize: 12 }}>
            {error ?? overall.map((d) => d.message).join("; ")}
          </div>
        )}
        <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
          <div style={{ flex: 1, minWidth: 0 }}>
            <ReactFlow
              nodes={flow.nodes}
              edges={flow.edges}
              nodeTypes={nodeTypes}
              colorMode="dark"
              fitView
              onConnect={(c) => {
                const edit = connectEdit(c);
                if (edit) apply([edit]);
              }}
              onEdgesDelete={(edges: Edge[]) => apply(edges.flatMap((e) => disconnectEdit(e) ?? []))}
              onNodeDragStop={(_, node: Node) =>
                apply([{ SetPosition: { node: node.id, at: [node.position.x, node.position.y] } }])
              }
              deleteKeyCode={["Backspace", "Delete"]}
              onNodesDelete={(nodes: Node[]) => apply(nodes.map((n) => ({ RemoveNode: { id: n.id } })))}
            >
              <Background />
              <Controls />
            </ReactFlow>
          </div>
          {showText && (
            <pre aria-label="graph text" style={{ flex: 1, overflow: "auto", margin: 0, padding: 6, fontSize: 11 }}>
              {reading?.text ?? ""}
            </pre>
          )}
        </div>
      </div>
    </GraphActionsContext.Provider>
  );
}
