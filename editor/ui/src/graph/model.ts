// The graph panel's model (M4 4.11a): a `.graph.ron` graph as React Flow nodes and edges, and the
// `graph.apply` edits a user's gestures make. No engine logic lives here: types and checks are the
// engine's (`graph.read`, `graph.check`).
import dagre from "@dagrejs/dagre";
import type { Edge, Node } from "@xyflow/react";

import type { GraphEdit, Literal } from "../api/generated/MethodGraphApplyParams";
import type { Diagnostic } from "../api/generated/MethodGraphReadResult";

export type { Diagnostic, GraphEdit, Literal };

/** A typed data pin of a library node. */
export interface PinDef {
  name: string;
  ty: string;
  required: boolean;
}

/** A node the engine offers (`graph.list_nodes`). */
export interface NodeDef {
  op: string;
  category: string;
  docs: string;
  pure: boolean;
  event: boolean;
  dynamic: boolean;
  exec_in: string[];
  exec_out: string[];
  inputs: PinDef[];
  outputs: PinDef[];
}

export type Library = Record<string, NodeDef>;

/** A data input: a link to another node's output, or a constant. */
export type Input = { Link: [string, string] } | { Value: Literal };

/** A graph as `graph.read` returns it. */
export interface GraphJson {
  id: string;
  kind: string;
  nodes: Record<string, { op: string; inputs: Record<string, Input> }>;
  exec: [string, string, string, string][];
  layout: Record<string, [number, number]>;
  signature?: { inputs: { name: string; ty: string }[]; outputs: { name: string; ty: string }[]; pure?: boolean };
  [extra: string]: unknown;
}

/** What a node view shows. */
export interface NodeData extends Record<string, unknown> {
  op: string;
  def: NodeDef | undefined;
  inputs: Record<string, Input>;
  /** Pin name → messages (`""`: the node as a whole). */
  problems: Record<string, string[]>;
  author: string | undefined;
  executed: boolean;
}

export interface EdgeData extends Record<string, unknown> {
  exec: boolean;
}

/** A connection a user drew (React Flow's shape). */
export interface Connection {
  source: string | null;
  sourceHandle?: string | null;
  target: string | null;
  targetHandle?: string | null;
}

const WIDTH = 200;
const ROW = 22;

/** Handle ids: `in:<pin>`, `out:<pin>`, `exec-in:<pin>`, `exec-out:<pin>`. */
export const handle = {
  input: (pin: string) => `in:${pin}`,
  output: (pin: string) => `out:${pin}`,
  execIn: (pin: string) => `exec-in:${pin}`,
  execOut: (pin: string) => `exec-out:${pin}`,
};

function height(def: NodeDef | undefined): number {
  if (def === undefined) return 2 * ROW;
  const rows = Math.max(def.inputs.length + def.exec_in.length, def.outputs.length + def.exec_out.length);
  return (rows + 1.5) * ROW;
}

/** Positions for nodes without one in the layout (dagre, left to right). */
function positions(graph: GraphJson, library: Library, edges: Edge[]): Record<string, { x: number; y: number }> {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: "LR", nodesep: 30, ranksep: 70 });
  g.setDefaultEdgeLabel(() => ({}));
  for (const [nodeId, node] of Object.entries(graph.nodes)) {
    g.setNode(nodeId, { width: WIDTH, height: height(library[node.op]) });
  }
  for (const e of edges) g.setEdge(e.source, e.target);
  dagre.layout(g);
  const out: Record<string, { x: number; y: number }> = {};
  for (const nodeId of Object.keys(graph.nodes)) {
    const saved = graph.layout[nodeId];
    if (saved !== undefined) {
      out[nodeId] = { x: saved[0], y: saved[1] };
      continue;
    }
    const n = g.node(nodeId);
    out[nodeId] = { x: n.x - n.width / 2, y: n.y - n.height / 2 };
  }
  return out;
}

/** Diagnostics by node, then pin (`""` for the node as a whole). */
function problemsByNode(diagnostics: readonly Diagnostic[]): Record<string, Record<string, string[]>> {
  const out: Record<string, Record<string, string[]>> = {};
  for (const d of diagnostics) {
    if (d.node_id == null) continue;
    const pins = (out[d.node_id] ??= {});
    (pins[d.pin ?? ""] ??= []).push(d.suggestion ? `${d.message} (did you mean ${d.suggestion}?)` : d.message);
  }
  return out;
}

/** The graph as React Flow nodes and edges. */
export function toFlow(
  graph: GraphJson,
  library: Library,
  diagnostics: readonly Diagnostic[],
  authors: Record<string, string>,
  executed: ReadonlySet<string> = new Set(),
): { nodes: Node<NodeData>[]; edges: Edge<EdgeData>[] } {
  const edges: Edge<EdgeData>[] = [];
  for (const [to, node] of Object.entries(graph.nodes)) {
    for (const [pin, input] of Object.entries(node.inputs)) {
      if (!("Link" in input)) continue;
      const [from, fromPin] = input.Link;
      edges.push({
        id: `${from}:${fromPin}->${to}:${pin}`,
        source: from,
        sourceHandle: handle.output(fromPin),
        target: to,
        targetHandle: handle.input(pin),
        data: { exec: false },
      });
    }
  }
  for (const [from, fromPin, to, toPin] of graph.exec) {
    edges.push({
      id: `exec:${from}:${fromPin}->${to}:${toPin}`,
      source: from,
      sourceHandle: handle.execOut(fromPin),
      target: to,
      targetHandle: handle.execIn(toPin),
      data: { exec: true },
      style: { strokeWidth: 3 },
    });
  }
  const at = positions(graph, library, edges);
  const problems = problemsByNode(diagnostics);
  const nodes = Object.entries(graph.nodes).map(([nodeId, node]) => ({
    id: nodeId,
    type: "graph",
    position: at[nodeId] ?? { x: 0, y: 0 },
    // Known sizes: nodes paint before React Flow measures them (and in tests, which never do).
    width: WIDTH,
    height: height(library[node.op]),
    data: {
      op: node.op,
      def: library[node.op],
      inputs: node.inputs,
      problems: problems[nodeId] ?? {},
      author: authors[nodeId],
      executed: executed.has(nodeId),
    },
  }));
  return { nodes, edges };
}

/** The edit a drawn connection makes: data pins to data pins, exec to exec; else none. */
export function connectEdit(c: Connection): GraphEdit | undefined {
  const [fromKind, fromPin] = split(c.sourceHandle);
  const [toKind, toPin] = split(c.targetHandle);
  if (c.source == null || c.target == null || fromPin === undefined || toPin === undefined) return undefined;
  if (fromKind === "out" && toKind === "in") {
    return { Connect: { from: c.source, from_pin: fromPin, to: c.target, to_pin: toPin } };
  }
  if (fromKind === "exec-out" && toKind === "exec-in") {
    return { ConnectExec: { from: c.source, from_pin: fromPin, to: c.target, to_pin: toPin } };
  }
  return undefined;
}

/** The edit that removes an edge. */
export function disconnectEdit(e: Edge): GraphEdit | undefined {
  const [, fromPin] = split(e.sourceHandle);
  const [, toPin] = split(e.targetHandle);
  if (fromPin === undefined || toPin === undefined) return undefined;
  return e.data?.["exec"]
    ? { DisconnectExec: { from: e.source, from_pin: fromPin, to: e.target, to_pin: toPin } }
    : { Disconnect: { to: e.target, to_pin: toPin } };
}

function split(h: string | null | undefined): [string | undefined, string | undefined] {
  if (h == null) return [undefined, undefined];
  const at = h.lastIndexOf(":");
  return at < 0 ? [undefined, undefined] : [h.slice(0, at), h.slice(at + 1)];
}

/** A constant typed into a pin of type `ty` (`undefined`: not a value of that type). */
export function parseLiteral(text: string, ty: string): Literal | undefined {
  const t = text.trim();
  const number = (s: string) => (s === "" || Number.isNaN(Number(s)) ? undefined : Number(s));
  const vector = (n: number): Literal | undefined => {
    const parts = t
      .split(/[\s,]+/)
      .filter((p) => p !== "")
      .map(number);
    if (parts.length !== n || parts.some((p) => p === undefined)) return undefined;
    const v = parts as number[];
    if (n === 2) return { Vec2: [v[0] ?? 0, v[1] ?? 0] };
    if (n === 3) return { Vec3: [v[0] ?? 0, v[1] ?? 0, v[2] ?? 0] };
    return { Vec4: [v[0] ?? 0, v[1] ?? 0, v[2] ?? 0, v[3] ?? 0] };
  };
  switch (ty) {
    case "bool":
      return t === "true" ? { Bool: true } : t === "false" ? { Bool: false } : undefined;
    case "i32":
    case "i64": {
      const n = number(t);
      return n !== undefined && Number.isInteger(n) ? { Int: n } : undefined;
    }
    case "f32": {
      const n = number(t);
      return n === undefined ? undefined : { Float: n };
    }
    case "Numeric":
    case "Scalar": {
      const n = number(t);
      if (n === undefined) return ty === "Numeric" ? (vector(3) ?? vector(2) ?? vector(4)) : undefined;
      return t.includes(".") || t.includes("e") ? { Float: n } : { Int: n };
    }
    case "Vec2":
      return vector(2);
    case "Vec3":
      return vector(3);
    case "Vec4":
    case "Quat":
      return vector(4);
    default:
      return { Text: text };
  }
}

/** A constant as editable text. */
export function literalText(input: Input | undefined): string {
  if (input === undefined || !("Value" in input)) return "";
  const v = Object.values(input.Value)[0];
  return Array.isArray(v) ? v.join(", ") : String(v);
}
