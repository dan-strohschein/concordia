// Maps engine JSON Schemas (with `x-concordia-widget` hints) to inspector widgets. Unknown shapes
// fall back to a raw JSON editor; nothing here throws on unexpected input.

type Json = Record<string, unknown>;

/** What the inspector shows for one value. */
export type Widget =
  | { kind: "number"; integer: boolean; min?: number; max?: number }
  | { kind: "bool" }
  | { kind: "string" }
  | { kind: "enum"; options: string[] }
  | { kind: "vec3" }
  | { kind: "quat" }
  | { kind: "color"; alpha: boolean }
  | { kind: "asset" }
  | { kind: "entity" }
  | { kind: "array"; item: Widget }
  | { kind: "struct"; fields: Field[] }
  | { kind: "option"; inner: Widget }
  | { kind: "json" };

/** One property row: wire name, display label, docs and widget. */
export interface Field {
  name: string;
  label: string;
  doc: string;
  widget: Widget;
  optional: boolean;
}

const JSON_EDITOR: Widget = { kind: "json" };

function isObject(value: unknown): value is Json {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** "illuminance_lux" → "Illuminance Lux" (Godot's capitalized property names). */
export function label(name: string): string {
  return name
    .split("_")
    .filter((w) => w.length > 0)
    .map((w) => w[0]?.toUpperCase() + w.slice(1))
    .join(" ");
}

/** Follows `#/$defs/Name` references (bounded, so cycles cannot hang). */
function resolve(schema: unknown, defs: Json): Json | undefined {
  let current = schema;
  for (let i = 0; i < 16 && isObject(current); i++) {
    const ref = current["$ref"];
    if (typeof ref !== "string") return current;
    current = defs[ref.replace(/^#\/\$defs\//, "")];
  }
  return undefined;
}

function hinted(s: Json): Widget | undefined {
  switch (s["x-concordia-widget"]) {
    case "vec3":
      return { kind: "vec3" };
    case "quat":
      return { kind: "quat" };
    case "color":
      return { kind: "color", alpha: s["maxItems"] === 4 };
    default:
      return undefined;
  }
}

function number(s: Json, integer: boolean): Widget {
  const w: Widget = { kind: "number", integer };
  if (typeof s["minimum"] === "number") w.min = s["minimum"];
  if (typeof s["maximum"] === "number") w.max = s["maximum"];
  return w;
}

function string(s: Json): Widget {
  if (s["format"] === "uuid") return { kind: "entity" };
  if (typeof s["pattern"] === "string" && s["pattern"].startsWith("^(asset:")) return { kind: "asset" };
  return { kind: "string" };
}

/** `oneOf` of string constants → enum; anything else (struct variants) → raw JSON. */
function oneOf(variants: unknown[]): Widget {
  const options = variants.map((v) => (isObject(v) && typeof v["const"] === "string" ? v["const"] : undefined));
  return options.every((o) => o !== undefined) && options.length > 0
    ? { kind: "enum", options: options as string[] }
    : JSON_EDITOR;
}

/** `anyOf: [null, T]` → optional T. */
function anyOf(variants: unknown[], defs: Json): Widget {
  const nonNull = variants.filter((v) => !(isObject(v) && v["type"] === "null"));
  const only = nonNull[0];
  return variants.length === 2 && nonNull.length === 1 && only !== undefined
    ? { kind: "option", inner: widgetFor(only, defs) }
    : JSON_EDITOR;
}

/** The widget for a schema; `defs` holds the root's `$defs`. */
export function widgetFor(schema: unknown, defs: Json): Widget {
  const s = resolve(schema, defs);
  if (s === undefined) return JSON_EDITOR;
  const hint = hinted(s);
  if (hint) return hint;
  if (Array.isArray(s["oneOf"])) return oneOf(s["oneOf"]);
  if (Array.isArray(s["anyOf"])) return anyOf(s["anyOf"], defs);
  switch (s["type"]) {
    case "boolean":
      return { kind: "bool" };
    case "number":
      return number(s, false);
    case "integer":
      return number(s, true);
    case "string":
      return string(s);
    case "object":
      return isObject(s["properties"]) ? { kind: "struct", fields: fieldsIn(s, defs) } : JSON_EDITOR;
    case "array":
      return isObject(s["items"]) && s["prefixItems"] === undefined && s["maxItems"] === undefined
        ? { kind: "array", item: widgetFor(s["items"], defs) }
        : JSON_EDITOR;
    default:
      return JSON_EDITOR;
  }
}

function fieldsIn(s: Json, defs: Json): Field[] {
  const properties = isObject(s["properties"]) ? s["properties"] : {};
  const required = Array.isArray(s["required"]) ? s["required"] : [];
  return Object.entries(properties).map(([name, property]) => ({
    name,
    label: label(name),
    doc: isObject(property) && typeof property["description"] === "string" ? property["description"] : "",
    widget: widgetFor(property, defs),
    optional: !required.includes(name),
  }));
}

/** The property rows of a component's root schema (empty when it is not a struct). */
export function fieldsOf(root: unknown): Field[] {
  if (!isObject(root)) return [];
  const defs = isObject(root["$defs"]) ? root["$defs"] : {};
  const widget = widgetFor(root, defs);
  return widget.kind === "struct" ? widget.fields : [];
}
