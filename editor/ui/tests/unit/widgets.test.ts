// Schema-driven inspector widgets (M1 phase 1.9), over real engine schemas.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { defaultValue, fieldsOf, label, widgetFor, type Widget } from "../../src/inspector/widgets";

const fixture = (name: string): Record<string, unknown> =>
  JSON.parse(readFileSync(join(__dirname, "../fixtures/schemas", `${name}.schema.json`), "utf8")) as Record<
    string,
    unknown
  >;

test("schema_to_widget_maps_vec3_number_enum_color_assetref", () => {
  const kinds = (name: string) => Object.fromEntries(fieldsOf(fixture(name)).map((f) => [f.name, f.widget]));
  const transform = kinds("Transform");
  expect(transform["translation"]?.kind).toBe("vec3");
  expect(transform["rotation"]?.kind).toBe("quat");
  const light = kinds("DirectionalLight");
  expect(light["color"]).toEqual({ kind: "color", alpha: false });
  expect(light["illuminance_lux"]).toMatchObject({ kind: "number", integer: false, min: 0, max: 200000 });
  expect(light["shadows"]?.kind).toBe("option");
  const camera = kinds("Camera");
  expect(camera["clear"]).toEqual({ kind: "color", alpha: true });
  expect(camera["tonemap"]).toEqual({ kind: "enum", options: ["AgX", "KhronosPbrNeutral", "AcesFitted", "Linear"] });
  // Struct-variant enums have their own widget since 2.3c (they fell back to raw JSON before).
  expect(camera["projection"]).toMatchObject({ kind: "variant" });
  expect(camera["priority"]).toMatchObject({ kind: "number", integer: true });
  const renderer = kinds("MeshRenderer");
  expect(renderer["mesh"]?.kind).toBe("asset");
  expect(renderer["cast_shadows"]).toEqual({ kind: "bool" });
  expect(renderer["mobility"]).toEqual({ kind: "enum", options: ["Auto", "Static", "Movable"] });
  expect(widgetFor(fixture("Parent"), {})).toEqual({ kind: "entity" });
  expect(
    Object.fromEntries(["Transform", "DirectionalLight", "Camera", "MeshRenderer"].map((n) => [n, kinds(n)])),
  ).toMatchSnapshot();
});

test("unknown_schema_shape_falls_back_to_json_editor", () => {
  for (const weird of [
    {},
    null,
    42,
    "x",
    { type: "array", prefixItems: [{ type: "number" }] },
    { $ref: "#/$defs/Missing" },
    { anyOf: [{ type: "string" }, { type: "number" }] },
  ]) {
    expect(widgetFor(weird, {})).toEqual({ kind: "json" });
  }
  expect(fieldsOf({ type: "object" })).toEqual([]);
});

test("option of a struct resolves through $defs", () => {
  const light = fixture("DirectionalLight");
  const shadows = fieldsOf(light).find((f) => f.name === "shadows")?.widget;
  expect(shadows?.kind).toBe("option");
  if (shadows?.kind !== "option") return;
  expect(shadows.inner.kind).toBe("struct");
  if (shadows.inner.kind !== "struct") return;
  expect(shadows.inner.fields.map((f) => f.name)).toContain("resolution");
});

test("labels follow Godot's capitalized words", () => {
  expect(label("illuminance_lux")).toBe("Illuminance Lux");
  expect(label("exposure_ev100")).toBe("Exposure Ev100");
  expect(label("x")).toBe("X");
});

test("numbers show seven significant digits, hiding f32 widening noise", async () => {
  const { formatNumber } = await import("../../src/inspector/fields");
  expect(formatNumber(0.05000000074505806)).toBe("0.05");
  expect(formatNumber(-1.575)).toBe("-1.575");
  expect(formatNumber(100000)).toBe("100000");
});

const defsOf = (root: Record<string, unknown>) => (root["$defs"] as Record<string, unknown> | undefined) ?? {};

test("struct_variant_enum_maps_to_a_variant_widget_with_each_variants_fields", () => {
  const material = fixture("Material");
  const fields = Object.fromEntries(fieldsOf(material).map((f) => [f.name, f.widget]));
  const nodes = fields["nodes"];
  expect(nodes?.kind).toBe("map");
  if (nodes?.kind !== "map") return;
  const node = nodes.value;
  expect(node.kind).toBe("variant");
  if (node.kind !== "variant") return;
  expect(node.variants.map((v) => v.name)).toEqual(["Slab", "Vertical", "Horizontal", "Coverage"]);
  const slab = node.variants[0]?.inner;
  expect(slab?.kind).toBe("struct");
  if (slab?.kind !== "struct") return;
  const slabFields = Object.fromEntries(slab.fields.map((f) => [f.name, f.widget]));
  expect(slabFields["base_color"]).toEqual({ kind: "color", alpha: true });
  expect(slabFields["roughness"]).toMatchObject({ kind: "number", min: 0, max: 1 });
  expect(slabFields["haze"]?.kind).toBe("option");
  const vertical = node.variants[1]?.inner;
  expect(vertical?.kind === "struct" && vertical.fields.map((f) => [f.name, f.widget.kind, f.optional])).toEqual([
    ["bottom", "string", false],
    ["top", "string", false],
  ]);
  expect(node.variants[1]?.doc).toContain("layered over");

  const alpha = fields["alpha_mode"];
  expect(alpha?.kind).toBe("variant");
  if (alpha?.kind !== "variant") return;
  expect(alpha.variants.map((v) => [v.name, v.inner?.kind])).toEqual([
    ["Opaque", undefined],
    ["Mask", "struct"],
    ["Blend", undefined],
  ]);
});

test("named_node_map_maps_to_a_map_widget_and_odd_maps_fall_back_to_json", () => {
  expect(widgetFor({ type: "object", additionalProperties: { type: "number" } }, {})).toEqual({
    kind: "map",
    value: { kind: "number", integer: false },
  });
  expect(widgetFor({ type: "object", additionalProperties: true }, {})).toEqual({ kind: "json" });
  expect(widgetFor({ type: "object" }, {})).toEqual({ kind: "json" });
  // A oneOf whose objects are not one-key variants is not an enum.
  expect(widgetFor({ oneOf: [{ type: "object", properties: { a: {}, b: {} }, required: ["a", "b"] }] }, {})).toEqual({
    kind: "json",
  });
  // All-unit variants stay a plain enum.
  expect(widgetFor({ oneOf: [{ const: "A" }, { const: "B" }] }, {})).toEqual({ kind: "enum", options: ["A", "B"] });
});

test("default values fill only required fields so the engine applies its own defaults", () => {
  const material = fixture("Material");
  const defs = defsOf(material);
  const node = widgetFor(defs["MaterialNode"], defs);
  if (node.kind !== "variant") throw new Error("variant");
  const [slab, vertical, horizontal] = node.variants;
  expect(defaultValue(node)).toEqual({ Slab: {} });
  expect(slab && defaultValue(slab.inner as Widget)).toEqual({});
  expect(vertical && defaultValue(vertical.inner as Widget)).toEqual({ top: "", bottom: "" });
  expect(horizontal && defaultValue(horizontal.inner as Widget)).toEqual({ a: "", b: "", mix: 0 });
  const haze = widgetFor(defs["Haze"], defs);
  expect(defaultValue(haze)).toEqual({ roughness: 0, weight: 0 });
  const thinFilm = widgetFor(defs["ThinFilm"], defs);
  // Numbers start at their minimum.
  expect(defaultValue(thinFilm)).toEqual({ thickness_nm: 0, ior: 1 });
  expect(defaultValue({ kind: "color", alpha: false })).toEqual([1, 1, 1]);
  expect(defaultValue({ kind: "map", value: { kind: "bool" } })).toEqual({});
  const alpha = widgetFor(defs["AlphaMode"], defs);
  expect(defaultValue(alpha)).toBe("Opaque");
});
