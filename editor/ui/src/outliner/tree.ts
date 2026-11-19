// The Outliner's entity tree, built from `Parent` links in `query.run` rows.

/** A `query.run` row: an entity id and the components selected for it. */
export interface EntityRow {
  id: string;
  components: Record<string, unknown>;
}

/** A tree node: the entity, its display label (Name or id) and children. */
export interface TreeNode {
  id: string;
  label: string;
  children: TreeNode[];
}

const nameOf = (row: EntityRow): string => {
  const name = row.components["Name"];
  return typeof name === "string" && name.length > 0 ? name : row.id;
};

const byLabelThenId = (a: TreeNode, b: TreeNode): number =>
  a.label < b.label ? -1 : a.label > b.label ? 1 : a.id < b.id ? -1 : a.id > b.id ? 1 : 0;

/** Roots are entities without a (present, acyclic) parent; siblings sort by label, then id. */
export function buildTree(rows: readonly EntityRow[]): TreeNode[] {
  const byId = new Map(rows.map((r) => [r.id, r]));
  const parentOf = (row: EntityRow): string | undefined => {
    const parent = row.components["Parent"];
    return typeof parent === "string" && byId.has(parent) ? parent : undefined;
  };
  const inCycle = (row: EntityRow): boolean => {
    const seen = new Set([row.id]);
    for (let p = parentOf(row); p !== undefined;) {
      if (seen.has(p)) return true;
      seen.add(p);
      const next = byId.get(p);
      p = next === undefined ? undefined : parentOf(next);
    }
    return false;
  };
  const effectiveParent = (row: EntityRow): string | undefined => (inCycle(row) ? undefined : parentOf(row));
  const nodes = new Map(rows.map((r) => [r.id, { id: r.id, label: nameOf(r), children: [] as TreeNode[] }]));
  const roots: TreeNode[] = [];
  for (const row of rows) {
    const node = nodes.get(row.id);
    if (node === undefined) continue;
    const parent = effectiveParent(row);
    const target = parent === undefined ? undefined : nodes.get(parent);
    (target ? target.children : roots).push(node);
  }
  const sort = (list: TreeNode[]): TreeNode[] => {
    list.sort(byLabelThenId);
    list.forEach((n) => sort(n.children));
    return list;
  };
  return sort(roots);
}
