// Schema-driven inspector widgets (M1 phase 1.9), over real engine schemas.
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { fieldsOf, label, widgetFor } from "../../src/inspector/widgets";

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
  expect(camera["projection"]?.kind).toBe("json");
  expect(camera["priority"]).toMatchObject({ kind: "number", integer: true });
  const renderer = kinds("MeshRenderer");
  expect(renderer["mesh"]?.kind).toBe("asset");
  expect(renderer["cast_shadows"]).toEqual({ kind: "bool" });
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
