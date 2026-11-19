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

export const byLabelThenId = (a: TreeNode, b: TreeNode): number =>
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

/** One entity's new row, or `undefined` when it no longer exists. */
export interface TreeUpdate {
  id: string;
  row: EntityRow | undefined;
}

const parentId = (row: EntityRow): string | undefined => {
  const parent = row.components["Parent"];
  return typeof parent === "string" ? parent : undefined;
};

/** Whether following `Parent` links from any row loops. */
function hasCycle(rows: ReadonlyMap<string, EntityRow>): boolean {
  for (const start of rows.values()) {
    const seen = new Set([start.id]);
    for (let p = parentId(start); p !== undefined && rows.has(p); p = parentId(rows.get(p) as EntityRow)) {
      if (seen.has(p)) return true;
      seen.add(p);
    }
  }
  return false;
}

/** The Outliner tree, patched per edit instead of rebuilt; always equal to `buildTree(rows)`. */
export class OutlinerTree {
  private readonly rows = new Map<string, EntityRow>();
  private nodes = new Map<string, TreeNode>();
  private top: TreeNode[] = [];
  /** Where each entity hangs: its parent's id, or undefined at the top. */
  private at = new Map<string, string | undefined>();
  /** Entities shown at the top because their parent does not exist (yet), by that parent's id. */
  private waiting = new Map<string, Set<string>>();
  /** Cycles make the layout order-dependent; while any exist, every edit rebuilds. */
  private cyclic = false;

  constructor(rows: readonly EntityRow[]) {
    rows.forEach((r) => this.rows.set(r.id, r));
    this.rebuild();
  }

  /** The roots (sorted; the arrays are reused between edits). */
  roots(): TreeNode[] {
    return this.top;
  }

  /** Applies edits in order. */
  apply(updates: readonly TreeUpdate[]): void {
    for (const { id, row } of updates) {
      if (row === undefined) this.rows.delete(id);
      else this.rows.set(id, row);
      if (this.cyclic || !this.patch(id, row)) this.rebuild();
    }
  }

  private rebuild(): void {
    this.top = buildTree([...this.rows.values()]);
    this.nodes = new Map();
    this.at = new Map();
    this.waiting = new Map();
    const index = (list: TreeNode[], parent: string | undefined) =>
      list.forEach((n) => {
        this.nodes.set(n.id, n);
        this.at.set(n.id, parent);
        index(n.children, n.id);
      });
    index(this.top, undefined);
    for (const r of this.rows.values()) {
      const p = parentId(r);
      if (p !== undefined && !this.rows.has(p)) this.wait(p, r.id);
    }
    this.cyclic = hasCycle(this.rows);
  }

  /** Patches one entity; `false` when only a rebuild can (it would create a cycle). */
  private patch(id: string, row: EntityRow | undefined): boolean {
    const node = this.nodes.get(id);
    if (node) this.detach(id);
    if (row === undefined) {
      if (!node) return true;
      this.nodes.delete(id);
      this.at.delete(id);
      // Its children lose their parent and move to the top until it returns.
      for (const child of [...node.children]) {
        this.at.set(child.id, undefined);
        insertSorted(this.top, child);
        this.wait(id, child.id);
      }
      node.children.length = 0;
      return true;
    }
    const current = node ?? { id, label: "", children: [] };
    current.label = labelOf(row);
    this.nodes.set(id, current);
    if (this.attach(current, parentId(row)) === "cycle") return false;
    // A parent arriving adopts the entities waiting for it.
    for (const child of this.waiting.get(id) ?? []) {
      this.detach(child);
      const childNode = this.nodes.get(child);
      if (childNode && this.attach(childNode, id) === "cycle") return false;
    }
    this.waiting.delete(id);
    return true;
  }

  private attach(node: TreeNode, parent: string | undefined): "ok" | "cycle" {
    for (let p = parent; p !== undefined; p = this.at.get(p)) if (p === node.id) return "cycle";
    const target = parent === undefined ? undefined : this.nodes.get(parent);
    if (parent !== undefined && target === undefined) this.wait(parent, node.id);
    this.at.set(node.id, target ? parent : undefined);
    insertSorted(target ? target.children : this.top, node);
    return "ok";
  }

  private detach(id: string): void {
    const node = this.nodes.get(id);
    if (!node) return;
    const parent = this.at.get(id);
    const list = parent === undefined ? this.top : (this.nodes.get(parent)?.children ?? this.top);
    const i = list.indexOf(node);
    if (i >= 0) list.splice(i, 1);
    for (const set of this.waiting.values()) set.delete(id);
  }

  private wait(parent: string, child: string): void {
    const set = this.waiting.get(parent) ?? new Set<string>();
    set.add(child);
    this.waiting.set(parent, set);
  }
}

const labelOf = (row: EntityRow): string => nameOf(row);

/** Inserts `node` into the sorted `list` (binary search). */
function insertSorted(list: TreeNode[], node: TreeNode): void {
  let [lo, hi] = [0, list.length];
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (byLabelThenId(list[mid] as TreeNode, node) < 0) lo = mid + 1;
    else hi = mid;
  }
  list.splice(lo, 0, node);
}
