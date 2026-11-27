// The graph panel (M4 4.11a, K4–K9): a pure Engine API client over `graph.read`,
// `graph.list_nodes` and `graph.apply`.
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";

import { EngineClient } from "../../src/api/client";
import { EditorSession } from "../../src/engine/session";
import { GraphPanel } from "../../src/graph/GraphPanel";
import { connectEdit, toFlow, type GraphJson } from "../../src/graph/model";
import { FakeEngine, oneEntity } from "./support/engine";

// React Flow measures with these; jsdom has neither.
class FakeResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver = FakeResizeObserver as unknown as typeof ResizeObserver;
(globalThis as unknown as { DOMMatrixReadOnly: unknown }).DOMMatrixReadOnly = class {
  m22 = 1;
};

const id = (n: number) => `0193b0f2-0000-7000-8000-${n.toString(16).padStart(12, "0")}`;
const PATH = "graphs/rise.graph.ron";

/** "On tick, if dt > 0.01, move up by dt": four nodes, three data links, two exec links. */
const graph: GraphJson = {
  id: id(100),
  kind: "Event",
  version: 1,
  signature: { inputs: [], outputs: [], pure: false },
  variables: [],
  nodes: {
    [id(1)]: { op: "event.on_tick", inputs: {} },
    [id(2)]: { op: "math.gt", inputs: { a: { Link: [id(1), "dt"] }, b: { Value: { Float: 0.01 } } } },
    [id(3)]: { op: "branch", inputs: { cond: { Link: [id(2), "out"] } } },
    [id(4)]: { op: "set.Health.value", inputs: { value: { Link: [id(1), "dt"] } } },
  },
  exec: [
    [id(1), "out", id(3), "in"],
    [id(3), "true", id(4), "in"],
  ],
  layout: { [id(1)]: [0, 0] },
};

const pin = (name: string, ty: string, required = true) => ({ name, ty, required });
const library = {
  "event.on_tick": {
    op: "event.on_tick",
    category: "Events",
    docs: "Every tick.",
    pure: false,
    event: true,
    dynamic: false,
    exec_in: [],
    exec_out: ["out"],
    inputs: [],
    outputs: [pin("entity", "EntityId", false), pin("dt", "f32", false)],
  },
  "math.gt": {
    op: "math.gt",
    category: "Math",
    docs: "a > b.",
    pure: true,
    event: false,
    dynamic: false,
    exec_in: [],
    exec_out: [],
    inputs: [pin("a", "Scalar"), pin("b", "Scalar")],
    outputs: [pin("out", "bool", false)],
  },
  branch: {
    op: "branch",
    category: "Flow",
    docs: "If.",
    pure: false,
    event: false,
    dynamic: false,
    exec_in: ["in"],
    exec_out: ["true", "false"],
    inputs: [pin("cond", "bool")],
    outputs: [],
  },
  "set.Health.value": {
    op: "set.Health.value",
    category: "Components",
    docs: "Sets it.",
    pure: false,
    event: false,
    dynamic: false,
    exec_in: ["in"],
    exec_out: ["out"],
    inputs: [pin("entity", "EntityId", false), pin("value", "f32")],
    outputs: [],
  },
};

function engineWith(read: () => unknown) {
  return new FakeEngine({
    ...oneEntity({ translation: [0, 0, 0] }),
    "graph.list_nodes": () => ({ nodes: library }),
    "graph.read": read,
    "graph.apply": () => ({ ok: true, diagnostics: [], added: [], reloaded: false, reload_error: null }),
  });
}

const reading = (diagnostics: unknown[] = [], authors: Record<string, string> = {}) => ({
  text: "Graph(...)",
  graph,
  diagnostics,
  authors,
});

test("graph_panel_renders_nodes_and_edges_from_fixture", async () => {
  const flow = toFlow(graph, library, [], {});
  expect(flow.nodes.map((n) => n.data.op).sort()).toEqual(["branch", "event.on_tick", "math.gt", "set.Health.value"]);
  const data = flow.edges.filter((e) => !e.data?.exec);
  const exec = flow.edges.filter((e) => e.data?.exec);
  expect(data).toHaveLength(3);
  expect(exec).toHaveLength(2);
  expect(data.find((e) => e.target === id(2))).toMatchObject({
    source: id(1),
    sourceHandle: "out:dt",
    targetHandle: "in:a",
  });
  expect(exec.find((e) => e.target === id(4))).toMatchObject({
    source: id(3),
    sourceHandle: "exec-out:true",
    targetHandle: "exec-in:in",
  });
  // Nodes without a saved position are laid out, and none overlap.
  const positions = flow.nodes.map((n) => `${Math.round(n.position.x)},${Math.round(n.position.y)}`);
  expect(new Set(positions).size).toBe(4);
  expect(flow.nodes.find((n) => n.id === id(1))?.position).toEqual({ x: 0, y: 0 });

  const session = await EditorSession.open(new EngineClient(engineWith(() => reading())));
  render(<GraphPanel session={session} path={PATH} />);
  for (const op of ["event.on_tick", "math.gt", "branch", "set.Health.value"]) {
    expect(await screen.findByText(op)).toBeTruthy();
  }
  // Constants only where a constant fits: not on entity pins.
  expect(screen.getByLabelText(`${id(2)} b`)).toBeTruthy();
  expect(screen.queryByLabelText(`${id(4)} entity`)).toBeNull();
  // Long ops widen their node.
  const wide = flow.nodes.find((n) => n.id === id(4))?.width ?? 0;
  expect(wide).toBeGreaterThan(flow.nodes.find((n) => n.id === id(3))?.width ?? 0);
});

test("graph_panel_edit_sends_graph_apply_with_human_actor", async () => {
  // Connecting two pins is one Connect (data) or ConnectExec (control flow) edit.
  expect(connectEdit({ source: id(1), sourceHandle: "out:dt", target: id(4), targetHandle: "in:value" })).toEqual({
    Connect: { from: id(1), from_pin: "dt", to: id(4), to_pin: "value" },
  });
  expect(
    connectEdit({ source: id(3), sourceHandle: "exec-out:false", target: id(4), targetHandle: "exec-in:in" }),
  ).toEqual({
    ConnectExec: { from: id(3), from_pin: "false", to: id(4), to_pin: "in" },
  });
  expect(
    connectEdit({ source: id(3), sourceHandle: "exec-out:false", target: id(4), targetHandle: "in:value" }),
  ).toBeUndefined();

  const engine = engineWith(() => reading());
  const session = await EditorSession.open(new EngineClient(engine));
  render(<GraphPanel session={session} path={PATH} />);
  // Editing a constant: one graph.apply, as the editor's actor.
  const b = await screen.findByLabelText(`${id(2)} b`);
  fireEvent.change(b, { target: { value: "0.5" } });
  fireEvent.blur(b);
  await waitFor(() => expect(engine.called("graph.apply")).toHaveLength(1));
  expect(engine.called("graph.apply")[0]).toEqual({
    path: PATH,
    actor: "human:local",
    edits: [{ SetValue: { node: id(2), pin: "b", value: { Float: 0.5 } } }],
  });
  // Deleting a node.
  fireEvent.click(screen.getByRole("button", { name: `delete ${id(3)}` }));
  await waitFor(() => expect(engine.called("graph.apply")).toHaveLength(2));
  expect(engine.called("graph.apply")[1]?.["edits"]).toEqual([{ RemoveNode: { id: id(3) } }]);
  // Adding one from the palette.
  fireEvent.change(screen.getByRole("searchbox", { name: "add node" }), { target: { value: "math.g" } });
  fireEvent.click(await screen.findByRole("option", { name: /math\.gt/ }));
  await waitFor(() => expect(engine.called("graph.apply")).toHaveLength(3));
  const added = engine.called("graph.apply")[2]?.["edits"] as { AddNode: { op: string; id: string | null } }[];
  expect(added[0]?.AddNode.op).toBe("math.gt");
});

test("graph_panel_shows_diagnostics_on_offending_pin", async () => {
  const diagnostic = {
    code: "type_mismatch",
    node_id: id(3),
    pin: "cond",
    message: "`branch`'s `cond` expects bool but gets f32",
    suggestion: null,
  };
  const overall = {
    code: "kind_rule",
    node_id: null,
    pin: null,
    message: "an event graph needs an event node",
    suggestion: null,
  };
  const session = await EditorSession.open(new EngineClient(engineWith(() => reading([diagnostic, overall]))));
  render(<GraphPanel session={session} path={PATH} />);
  const node = await screen.findByTestId(`graph-node-${id(3)}`);
  expect(node.getAttribute("aria-invalid")).toBe("true");
  const marker = await screen.findByTestId(`pin-error-${id(3)}-cond`);
  expect(marker.getAttribute("title")).toContain("expects bool but gets f32");
  expect(screen.getByTestId(`graph-node-${id(2)}`).getAttribute("aria-invalid")).toBe("false");
  expect(screen.getByRole("alert").textContent).toContain("an event graph needs an event node");
});

test("graph_panel_rereads_on_edit_events_and_shows_who_changed_each_node", async () => {
  let authors: Record<string, string> = {};
  const engine = engineWith(() => reading([], authors));
  const session = await EditorSession.open(new EngineClient(engine));
  render(<GraphPanel session={session} path={PATH} />);
  await screen.findByText("branch");
  expect(engine.called("graph.read")).toHaveLength(1);
  authors = { [id(3)]: "agent:gameplay-programmer" };
  await act(async () => {
    engine.event({ topic: "edit", method: "graph.apply", actor: "agent:gameplay-programmer" });
    await new Promise((r) => setTimeout(r, 20));
  });
  expect(engine.called("graph.read")).toHaveLength(2);
  expect(await screen.findByTestId(`author-${id(3)}`)).toBeTruthy();
  expect(screen.getByTestId(`author-${id(3)}`).textContent).toContain("gameplay-programmer");
  // The canonical text is one click away.
  fireEvent.click(screen.getByRole("button", { name: "view as text" }));
  expect(screen.getByLabelText("graph text").textContent).toBe("Graph(...)");
});

test("graph_panel_constants_follow_the_graph_when_it_changes_underneath", async () => {
  let b = 0.01;
  const engine = engineWith(() => {
    const changed = structuredClone(graph);
    const node = changed.nodes[id(2)];
    if (node) node.inputs["b"] = { Value: { Float: b } };
    return { text: "Graph(...)", graph: changed, diagnostics: [], authors: {} };
  });
  const session = await EditorSession.open(new EngineClient(engine));
  render(<GraphPanel session={session} path={PATH} />);
  const field = (await screen.findByLabelText(`${id(2)} b`)) as HTMLInputElement;
  expect(field.value).toBe("0.01");
  // An undo (or an agent's SetValue) changes the constant; the field shows the new one.
  b = 0.75;
  await act(async () => {
    engine.event({ topic: "edit", method: "txn.undo" });
    await new Promise((r) => setTimeout(r, 20));
  });
  expect((screen.getByLabelText(`${id(2)} b`) as HTMLInputElement).value).toBe("0.75");
});
