// The incremental Outliner tree (M1 phase 1.9e): patched per edit, always equal to a full rebuild.
import { OutlinerTree, buildTree, type EntityRow } from "../../src/outliner/tree";

const row = (id: string, name: string, parent?: string): EntityRow => ({
  id,
  components: { Name: name, ...(parent === undefined ? {} : { Parent: parent }) },
});

/** A small deterministic generator (mulberry32). */
function random(seed: number): () => number {
  let a = seed;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

test("patching matches a full rebuild across random edits", () => {
  for (let seed = 1; seed <= 40; seed++) {
    const next = random(seed);
    const pick = <T>(xs: T[]) => xs[Math.floor(next() * xs.length)];
    const rows = new Map<string, EntityRow>();
    for (let i = 0; i < 12; i++) rows.set(`e${i}`, row(`e${i}`, `N${Math.floor(next() * 5)}`));
    const tree = new OutlinerTree([...rows.values()]);
    for (let step = 0; step < 30; step++) {
      const ids = [...rows.keys()];
      const id = pick(ids) ?? "e0";
      const kind = next();
      const update = (r: EntityRow | undefined) => {
        if (r) rows.set(id, r);
        else rows.delete(id);
        tree.apply([{ id, row: r }]);
      };
      if (kind < 0.3)
        update(row(id, `N${Math.floor(next() * 5)}`, rows.get(id)?.components["Parent"] as string | undefined));
      else if (kind < 0.55) {
        // Parent to an older id only (no cycles; cycles are errors with order-dependent layout).
        const parent = pick(ids.filter((p) => Number(p.slice(1)) < Number(id.slice(1))));
        update(row(id, String(rows.get(id)?.components["Name"]), parent));
      } else if (kind < 0.7) update(row(id, String(rows.get(id)?.components["Name"])));
      else if (kind < 0.85) update(undefined);
      else {
        const fresh = `e${100 + step}`;
        rows.set(fresh, row(fresh, `N${Math.floor(next() * 5)}`, pick(ids)));
        tree.apply([{ id: fresh, row: rows.get(fresh) }]);
      }
      expect(tree.roots()).toEqual(buildTree([...rows.values()]));
    }
  }
});

test("a parent arriving after its children adopts them", () => {
  const tree = new OutlinerTree([row("b", "Child", "a")]);
  expect(tree.roots().map((n) => n.id)).toEqual(["b"]);
  tree.apply([{ id: "a", row: row("a", "Parent") }]);
  expect(tree.roots()).toEqual(buildTree([row("b", "Child", "a"), row("a", "Parent")]));
});
