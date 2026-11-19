// Property editors for each widget kind (Godot-style rows: label left, editor right). Every editor
// calls `onCommit` once per gesture (Enter, blur, toggle or pick); typing alone commits nothing.
import { useState } from "react";

import { eulerToQuat, hexToLinear, linearToHex, quatToEuler } from "./math";
import type { Field, Widget } from "./widgets";

interface EditorProps {
  label: string;
  value: unknown;
  widget: Widget;
  onCommit: (value: unknown) => void;
}

const AXES = ["x", "y", "z"] as const;
const AXIS_COLORS = ["#e05050", "#60c060", "#5080e0"];

/** A text box with a draft value: Enter or blur commits through `parse`, Escape reverts. */
function DraftInput(props: {
  label: string;
  shown: string;
  parse: (text: string) => unknown;
  onCommit: (value: unknown) => void;
  style?: React.CSSProperties;
}) {
  // The draft belongs to the value it was started from; a new value from the engine resets it.
  const [edit, setEdit] = useState<{ base: string; text: string }>();
  const draft = edit !== undefined && edit.base === props.shown ? edit.text : props.shown;
  const setDraft = (text: string) => setEdit({ base: props.shown, text });
  const revert = () => setEdit(undefined);
  const commit = () => {
    const parsed = props.parse(draft);
    revert();
    if (parsed !== undefined && draft !== props.shown) props.onCommit(parsed);
  };
  return (
    <input
      aria-label={props.label}
      value={draft}
      style={{ width: "100%", minWidth: 0, ...props.style }}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") revert();
      }}
    />
  );
}

const parseNumber =
  (widget: Extract<Widget, { kind: "number" }>) =>
  (text: string): number | undefined => {
    const n = Number(text);
    if (text.trim() === "" || !Number.isFinite(n) || (widget.integer && !Number.isInteger(n))) return undefined;
    if ((widget.min !== undefined && n < widget.min) || (widget.max !== undefined && n > widget.max)) return undefined;
    return n;
  };

/** f32 values arrive widened to f64 (0.05 → 0.0500000007…); show 7 significant digits. */
export const formatNumber = (n: number): string => String(Number(n.toPrecision(7)));

const anyNumber = (text: string) => {
  const n = Number(text);
  return text.trim() !== "" && Number.isFinite(n) ? n : undefined;
};

function Vector(props: { label: string; values: number[]; onCommit: (values: number[]) => void }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 2 }}>
      {AXES.map((axis, i) => (
        <DraftInput
          key={axis}
          label={`${props.label} ${axis}`}
          shown={formatNumber(props.values[i] ?? 0)}
          parse={anyNumber}
          style={{ borderLeft: `2px solid ${AXIS_COLORS[i]}` }}
          onCommit={(n) => props.onCommit(props.values.map((v, j) => (j === i ? (n as number) : v)))}
        />
      ))}
    </div>
  );
}

const asNumbers = (value: unknown, length: number): number[] =>
  Array.from({ length }, (_, i) => (Array.isArray(value) && typeof value[i] === "number" ? value[i] : 0));

/** The editor for one value. */
export function ValueEditor({ label, value, widget, onCommit }: EditorProps) {
  switch (widget.kind) {
    case "number":
      return (
        <DraftInput
          label={label}
          shown={typeof value === "number" ? formatNumber(value) : String(value ?? "")}
          parse={parseNumber(widget)}
          onCommit={onCommit}
        />
      );
    case "bool":
      return (
        <input
          type="checkbox"
          aria-label={label}
          checked={value === true}
          onChange={(e) => onCommit(e.target.checked)}
        />
      );
    case "string":
    case "asset":
    case "entity":
      return (
        <DraftInput label={label} shown={typeof value === "string" ? value : ""} parse={(t) => t} onCommit={onCommit} />
      );
    case "enum":
      return (
        <select aria-label={label} value={String(value)} onChange={(e) => onCommit(e.target.value)}>
          {widget.options.map((o) => (
            <option key={o}>{o}</option>
          ))}
        </select>
      );
    case "vec3":
      return <Vector label={label} values={asNumbers(value, 3)} onCommit={onCommit} />;
    case "quat":
      return (
        <Vector label={label} values={quatToEuler(asNumbers(value, 4))} onCommit={(e) => onCommit(eulerToQuat(e))} />
      );
    case "color":
      return (
        <ColorEditor
          label={label}
          value={asNumbers(value, widget.alpha ? 4 : 3)}
          alpha={widget.alpha}
          onCommit={onCommit}
        />
      );
    case "struct":
      return <Rows fields={widget.fields} value={value} onCommit={onCommit} />;
    case "option":
      return value === null || value === undefined ? (
        <JsonEditor label={label} value={null} onCommit={onCommit} />
      ) : (
        <div>
          <ValueEditor label={label} value={value} widget={widget.inner} onCommit={onCommit} />
          <button onClick={() => onCommit(null)}>Clear</button>
        </div>
      );
    default:
      return <JsonEditor label={label} value={value} onCommit={onCommit} />;
  }
}

function ColorEditor(props: { label: string; value: number[]; alpha: boolean; onCommit: (v: number[]) => void }) {
  const [r, g, b, a] = props.value;
  return (
    <div style={{ display: "flex", gap: 4 }}>
      <input
        type="color"
        aria-label={props.label}
        value={linearToHex([r ?? 0, g ?? 0, b ?? 0])}
        onChange={(e) =>
          props.onCommit(props.alpha ? [...hexToLinear(e.target.value), a ?? 1] : hexToLinear(e.target.value))
        }
      />
      {props.alpha && (
        <DraftInput
          label={`${props.label} alpha`}
          shown={String(a ?? 1)}
          parse={(t) => {
            const n = anyNumber(t);
            return n !== undefined && n >= 0 && n <= 1 ? n : undefined;
          }}
          onCommit={(n) => props.onCommit([r ?? 0, g ?? 0, b ?? 0, n as number])}
        />
      )}
    </div>
  );
}

/** Raw JSON for shapes without a dedicated editor (commits only valid JSON). */
function JsonEditor(props: { label: string; value: unknown; onCommit: (v: unknown) => void }) {
  return (
    <DraftInput
      label={props.label}
      shown={JSON.stringify(props.value)}
      parse={(t) => {
        try {
          return JSON.parse(t) as unknown;
        } catch {
          return undefined;
        }
      }}
      onCommit={props.onCommit}
      style={{ fontFamily: "monospace" }}
    />
  );
}

/** Property rows of a struct value. */
export function Rows(props: { fields: Field[]; value: unknown; onCommit: (value: unknown) => void }) {
  const record =
    typeof props.value === "object" && props.value !== null ? (props.value as Record<string, unknown>) : {};
  return (
    <div style={{ display: "grid", gridTemplateColumns: "40% 60%", rowGap: 2, alignItems: "center" }}>
      {props.fields.map((f) => (
        <Row
          key={f.name}
          field={f}
          value={record[f.name]}
          onCommit={(v) => props.onCommit({ ...record, [f.name]: v })}
        />
      ))}
    </div>
  );
}

function Row(props: { field: Field; value: unknown; onCommit: (value: unknown) => void }) {
  return (
    <>
      <span title={props.field.doc} style={{ opacity: 0.8, overflow: "hidden", textOverflow: "ellipsis" }}>
        {props.field.label}
      </span>
      <ValueEditor
        label={props.field.label}
        value={props.value}
        widget={props.field.widget}
        onCommit={props.onCommit}
      />
    </>
  );
}
