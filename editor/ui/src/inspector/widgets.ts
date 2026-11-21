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
  | { kind: "variant"; variants: Variant[] }
  | { kind: "map"; value: Widget }
  | { kind: "json" };

/** One variant of an enum: a bare name (`inner` undefined) or `{ name: value }`. */
export interface Variant {
  name: string;
  doc: string;
  inner?: Widget;
}

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

/** A `oneOf` entry as a variant: a string constant, or an object with exactly one required key. */
function variantOf(v: unknown, defs: Json): Variant | undefined {
  if (!isObject(v)) return undefined;
  const doc = typeof v["description"] === "string" ? v["description"] : "";
  if (typeof v["const"] === "string") return { name: v["const"], doc };
  const properties = isObject(v["properties"]) ? Object.entries(v["properties"]) : [];
  const required = Array.isArray(v["required"]) ? v["required"] : [];
  const [only] = properties;
  if (properties.length !== 1 || only === undefined || required.length !== 1 || required[0] !== only[0]) {
    return undefined;
  }
  return { name: only[0], doc, inner: widgetFor(only[1], defs) };
}

/** `oneOf` of string constants → enum; with struct variants → variant; anything else → raw JSON. */
function oneOf(variants: unknown[], defs: Json): Widget {
  const parsed = variants.map((v) => variantOf(v, defs));
  if (parsed.length === 0 || parsed.some((v) => v === undefined)) return JSON_EDITOR;
  const all = parsed as Variant[];
  return all.every((v) => v.inner === undefined)
    ? { kind: "enum", options: all.map((v) => v.name) }
    : { kind: "variant", variants: all };
}

/** `{type: object, additionalProperties: T}` without properties → a map of T by name. */
function map(s: Json, defs: Json): Widget {
  const value = s["additionalProperties"];
  return isObject(value) && s["properties"] === undefined
    ? { kind: "map", value: widgetFor(value, defs) }
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
  if (Array.isArray(s["oneOf"])) return oneOf(s["oneOf"], defs);
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
      return isObject(s["properties"]) ? { kind: "struct", fields: fieldsIn(s, defs) } : map(s, defs);
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

/** A starting value for a new entry or variant: required struct fields only (the engine fills the rest), numbers at their minimum. */
export function defaultValue(widget: Widget): unknown {
  switch (widget.kind) {
    case "number":
      return Math.max(widget.min ?? 0, Math.min(0, widget.max ?? 0));
    case "bool":
      return false;
    case "string":
    case "asset":
    case "entity":
      return "";
    case "enum":
      return widget.options[0] ?? "";
    case "vec3":
      return [0, 0, 0];
    case "quat":
      return [0, 0, 0, 1];
    case "color":
      return widget.alpha ? [1, 1, 1, 1] : [1, 1, 1];
    case "array":
      return [];
    case "struct":
      return Object.fromEntries(widget.fields.filter((f) => !f.optional).map((f) => [f.name, defaultValue(f.widget)]));
    case "variant":
      return widget.variants[0] === undefined ? null : variantValue(widget.variants[0]);
    case "map":
      return {};
    default:
      return null;
  }
}

/** A variant's default value: its name, or `{ name: default }`. */
export function variantValue(variant: Variant): unknown {
  return variant.inner === undefined ? variant.name : { [variant.name]: defaultValue(variant.inner) };
}
