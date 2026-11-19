// The Outliner at scale: the tree flattened for display, and the window of rows worth rendering.
import type { TreeNode } from "./tree";

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

/** The scroll position that brings row `index` into view (moving as little as possible), or
 * `undefined` when it is already in view or there is no such row. */
export function scrollToShow(p: {
  index: number;
  scrollTop: number;
  height: number;
  rowHeight: number;
}): number | undefined {
  if (p.index < 0) return undefined;
  const top = p.index * p.rowHeight;
  if (top < p.scrollTop) return top;
  if (top + p.rowHeight > p.scrollTop + p.height) return top + p.rowHeight - p.height;
  return undefined;
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
