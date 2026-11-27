// One node in the graph panel: its op, typed pins as handles, editable constants on unlinked
// inputs, inline diagnostics and who last changed it (M4 4.11a, K6–K7).
import { Handle, Position, type NodeProps, type Node } from "@xyflow/react";
import { createContext, useContext, useState } from "react";

import { RoleBadge } from "../studio/RoleBadge";
import { handle, literalText, parseLiteral, type GraphEdit, type NodeData, type PinDef } from "./model";

/** What a node can do: the panel applies its edits. */
export interface GraphActions {
  apply: (edits: GraphEdit[]) => void;
  roleColor: (actor: string) => string | undefined;
}

export const GraphActionsContext = createContext<GraphActions>({ apply: () => {}, roleColor: () => undefined });

const pinStyle = { fontSize: 11, lineHeight: "22px", position: "relative" as const, padding: "0 10px" };

function ErrorMark({ node, pin, messages }: { node: string; pin: string; messages: string[] | undefined }) {
  if (messages === undefined || messages.length === 0) return null;
  return (
    <span
      data-testid={`pin-error-${node}-${pin}`}
      title={messages.join("\n")}
      style={{ color: "#e5484d", marginLeft: 4 }}
    >
      ●
    </span>
  );
}

/** An unlinked input's constant, applied on blur when it parses as the pin's type. */
function ConstantField({ node, pin, text }: { node: string; pin: PinDef; text: string }) {
  const { apply } = useContext(GraphActionsContext);
  const [value, setValue] = useState(text);
  const [bad, setBad] = useState(false);
  const commit = () => {
    if (value === text) return;
    const literal = parseLiteral(value, pin.ty);
    setBad(literal === undefined);
    if (literal !== undefined) apply([{ SetValue: { node, pin: pin.name, value: literal } }]);
  };
  return (
    <input
      aria-label={`${node} ${pin.name}`}
      aria-invalid={bad}
      className="nodrag"
      value={value}
      placeholder={pin.ty}
      onChange={(e) => setValue(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => (e.key === "Enter" ? commit() : undefined)}
      style={{ width: 70, marginLeft: 4, fontSize: 11 }}
    />
  );
}

export function GraphNode({ id, data }: NodeProps<Node<NodeData>>) {
  const { apply, roleColor } = useContext(GraphActionsContext);
  const def = data.def;
  const invalid = Object.keys(data.problems).length > 0 || def === undefined;
  const linked = (pin: string) => {
    const input = data.inputs[pin];
    return input !== undefined && "Link" in input;
  };
  return (
    <div
      data-testid={`graph-node-${id}`}
      aria-invalid={invalid}
      title={def?.docs}
      style={{
        minWidth: 180,
        background: "var(--panel, #1e1e22)",
        color: "var(--text, #ddd)",
        border: `2px solid ${invalid ? "#e5484d" : data.executed ? "#3dd68c" : "#555"}`,
        borderRadius: 6,
        boxShadow: data.executed ? "0 0 8px #3dd68c" : undefined,
      }}
    >
      <div
        style={{ display: "flex", alignItems: "center", gap: 6, padding: "2px 8px", borderBottom: "1px solid #444" }}
      >
        <strong style={{ fontSize: 12, flex: 1 }}>{data.op}</strong>
        <ErrorMark node={id} pin="" messages={data.problems[""] ?? (def === undefined ? ["unknown op"] : undefined)} />
        {data.author !== undefined && (
          <span data-testid={`author-${id}`} style={{ fontSize: 10 }}>
            <RoleBadge actor={data.author} color={roleColor(data.author)} />
          </span>
        )}
        <button
          aria-label={`delete ${id}`}
          className="nodrag"
          onClick={() => apply([{ RemoveNode: { id } }])}
          style={{ fontSize: 10 }}
        >
          ✕
        </button>
      </div>
      <div style={{ display: "flex", justifyContent: "space-between" }}>
        <div>
          {def?.exec_in.map((pin) => (
            <div key={`x${pin}`} style={pinStyle}>
              <Handle type="target" position={Position.Left} id={handle.execIn(pin)} style={{ borderRadius: 0 }} />▶{" "}
              {pin}
            </div>
          ))}
          {def?.inputs.map((pin) => (
            <div key={pin.name} style={pinStyle}>
              <Handle type="target" position={Position.Left} id={handle.input(pin.name)} />
              {pin.name}
              {!linked(pin.name) && <ConstantField node={id} pin={pin} text={literalText(data.inputs[pin.name])} />}
              <ErrorMark node={id} pin={pin.name} messages={data.problems[pin.name]} />
            </div>
          ))}
        </div>
        <div style={{ textAlign: "right" }}>
          {def?.exec_out.map((pin) => (
            <div key={`x${pin}`} style={pinStyle}>
              {pin} ▶
              <Handle type="source" position={Position.Right} id={handle.execOut(pin)} style={{ borderRadius: 0 }} />
            </div>
          ))}
          {def?.outputs.map((pin) => (
            <div key={pin.name} style={pinStyle} title={pin.ty}>
              {pin.name}
              <Handle type="source" position={Position.Right} id={handle.output(pin.name)} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
