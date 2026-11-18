// The Outliner's data at scale: rows patched from edit events, the tree flattened for display,
// and the window of rows worth rendering.
import type { EntityRow, TreeNode } from "./tree";

/** One entity's new row, or `undefined` when it no longer exists. */
export interface RowUpdate {
  id: string;
  row: EntityRow | undefined;
}

/** A copy of `rows` with `updates` applied (insertion order kept for unchanged rows). */
export function withUpdates(
  rows: ReadonlyMap<string, EntityRow>,
  updates: readonly RowUpdate[],
): Map<string, EntityRow> {
  const next = new Map(rows);
  for (const { id, row } of updates) {
    if (row === undefined) next.delete(id);
    else next.set(id, row);
  }
  return next;
}

/** A tree node at its display depth. */
export interface FlatRow {
  node: TreeNode;
  depth: number;
}

/** The tree depth-first, as display rows. */
export function flatten(roots: readonly TreeNode[]): FlatRow[] {
  const out: FlatRow[] = [];
  const visit = (node: TreeNode, depth: number) => {
    out.push({ node, depth });
    node.children.forEach((c) => visit(c, depth + 1));
  };
  roots.forEach((n) => visit(n, 0));
  return out;
}

/** Which rows a scrolled list should render: `[start, end)`. */
export function rowWindow(p: {
  count: number;
  scrollTop: number;
  height: number;
  rowHeight: number;
  overscan: number;
}): {
  start: number;
  end: number;
} {
  if (!(p.rowHeight > 0)) throw new RangeError(`row height ${p.rowHeight} must be positive`);
  const first = Math.floor(Math.max(0, p.scrollTop) / p.rowHeight);
  const visible = Math.ceil(Math.max(0, p.height) / p.rowHeight);
  return {
    start: Math.max(0, Math.min(p.count, first - p.overscan)),
    end: Math.max(0, Math.min(p.count, first + visible + p.overscan)),
  };
}
