// The Outliner's tree from Parent links (M1 phase 1.9).
import { buildTree, type EntityRow } from "../../src/outliner/tree";

const row = (id: string, name?: string, parent?: string): EntityRow => ({
  id,
  components: { ...(name === undefined ? {} : { Name: name }), ...(parent === undefined ? {} : { Parent: parent }) },
});

test("outliner_builds_tree_from_parent_links_and_sorts_stably", () => {
  const rows = [row("e5", "Lamp", "e1"), row("e1", "World"), row("e3", "Box"), row("e4", "Arm", "e1"), row("e2")];
  const tree = buildTree(rows);
  expect(tree.map((n) => n.label)).toEqual(["Box", "World", "e2"]);
  expect(tree[1]?.children.map((n) => n.label)).toEqual(["Arm", "Lamp"]);
  expect(buildTree([...rows].reverse())).toEqual(tree);
});

test("missing parents and cycles become roots instead of disappearing", () => {
  const tree = buildTree([row("a", "A", "ghost"), row("b", "B", "c"), row("c", "C", "b")]);
  const all = (nodes: ReturnType<typeof buildTree>): string[] => nodes.flatMap((n) => [n.id, ...all(n.children)]);
  expect(all(tree).sort()).toEqual(["a", "b", "c"]);
  expect(tree.some((n) => n.id === "a")).toBe(true);
});

test("same names sort by id so the order never flickers", () => {
  const tree = buildTree([row("z", "Box"), row("a", "Box")]);
  expect(tree.map((n) => n.id)).toEqual(["a", "z"]);
});
