// Add Component choices (M1 phase 1.14): the engine's addable components the entity lacks, searched.
import { componentChoices, type ComponentRow } from "../../src/inspector/components";

const rows: ComponentRow[] = [
  { name: "Transform", category: "Core", description: "Local transform.", addable: true },
  { name: "Parent", category: "Core", description: "Hierarchy link.", addable: false },
  { name: "PointLight", category: "Lighting", description: "An omni light.", addable: true },
  { name: "SpotLight", category: "Lighting", description: "A cone light.", addable: true },
  { name: "RigidBody", category: "Physics", description: "A simulated body.", addable: true },
];

test("lists addable components the entity lacks, grouped by category", () => {
  const names = componentChoices(rows, ["Transform"], "").map((r) => r.name);
  expect(names).toEqual(["PointLight", "SpotLight", "RigidBody"]);
});

test("search matches name or category, ignoring case", () => {
  expect(componentChoices(rows, [], "point").map((r) => r.name)).toEqual(["PointLight"]);
  expect(componentChoices(rows, [], "LIGHTING").map((r) => r.name)).toEqual(["PointLight", "SpotLight"]);
  expect(componentChoices(rows, [], "zzz")).toEqual([]);
});

test("rows without a category sort last", () => {
  const extra = [...rows, { name: "Custom", category: null, description: "", addable: true }];
  expect(componentChoices(extra, [], "").at(-1)?.name).toBe("Custom");
});
