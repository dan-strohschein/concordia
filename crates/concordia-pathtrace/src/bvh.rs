//! A bounding-volume hierarchy over triangles, built on the CPU with binned SAH (Wald 2007) and
//! flattened depth-first, so the CPU traversal and the GPU's (4.1b3) read the same arrays.

use concordia_core::math::Vec3;

use crate::geometry::{Hit, Ray, Triangle};

/// Bins per split axis.
const BINS: usize = 16;
/// Leaves hold at most this many triangles.
const MAX_LEAF: usize = 4;
/// Conservative slab test: tmax grows by 2·γ(3) (Ize, "Robust BVH Ray Traversal", JCGT 2013).
const ROBUST: f32 = 1.0 + 2.0 * (3.0 * f32::EPSILON * 0.5) / (1.0 - 3.0 * f32::EPSILON * 0.5);

/// One node: bounds and either two children (`count == 0`: the left child is the next node, the
/// right one is `first`) or a leaf (`count` triangles from `first` in `order`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    /// Bounds' minimum corner.
    pub min: Vec3,
    /// Bounds' maximum corner.
    pub max: Vec3,
    /// Right child (inner node) or first index into `order` (leaf).
    pub first: u32,
    /// Triangles in the leaf (0 for inner nodes).
    pub count: u32,
}

/// A hierarchy over boxes (triangles, or instances): flattened nodes and the leaf order.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    nodes: Vec<Node>,
    order: Vec<u32>,
}

impl Tree {
    /// Builds a hierarchy over primitives with these (min, max) bounds.
    pub fn build(bounds: &[(Vec3, Vec3)]) -> Self {
        let mut tree = Self {
            nodes: Vec::with_capacity(bounds.len().max(1) * 2),
            order: (0..bounds.len() as u32).collect(),
        };
        if !bounds.is_empty() {
            let builder = Builder {
                centroids: bounds.iter().map(|(lo, hi)| (*lo + *hi) * 0.5).collect(),
                bounds: bounds.to_vec(),
            };
            builder.node(&mut tree.nodes, &mut tree.order, 0, bounds.len());
        }
        tree
    }

    /// The flattened nodes (root first).
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Primitive indices in leaf order.
    pub fn order(&self) -> &[u32] {
        &self.order
    }

    /// Visits the primitives whose boxes `ray` crosses, nearest box first. `leaf` may shorten
    /// the ray (`t_max`) and returns true to stop.
    pub fn walk(&self, ray: &mut Ray, mut leaf: impl FnMut(u32, &mut Ray) -> bool) {
        if self.nodes.is_empty() {
            return;
        }
        let inv = Vec3::ONE / ray.dir;
        let mut stack = [0u32; 64];
        let mut top = 1;
        while top > 0 {
            top -= 1;
            let index = stack[top];
            let node = &self.nodes[index as usize];
            if slab(node, ray, inv).is_none() {
                continue;
            }
            if node.count > 0 {
                let leaves = &self.order[node.first as usize..(node.first + node.count) as usize];
                for &prim in leaves {
                    if leaf(prim, ray) {
                        return;
                    }
                }
                continue;
            }
            let (left, right) = (index + 1, node.first);
            let near = |child: u32| slab(&self.nodes[child as usize], ray, inv);
            // Push the farther child first so the nearer one is visited next.
            let order = match (near(left), near(right)) {
                (Some(a), Some(b)) if a <= b => [Some(right), Some(left)],
                (Some(_), Some(_)) => [Some(left), Some(right)],
                (Some(_), None) => [None, Some(left)],
                (None, Some(_)) => [None, Some(right)],
                (None, None) => [None, None],
            };
            for child in order.into_iter().flatten() {
                debug_assert!(top < stack.len(), "tree deeper than its stack");
                if top < stack.len() {
                    stack[top] = child;
                    top += 1;
                }
            }
        }
    }
}

/// A hierarchy over a triangle list (hits name indices into that list).
#[derive(Debug, Clone, Default)]
pub struct Bvh {
    triangles: Vec<Triangle>,
    tree: Tree,
}

impl Bvh {
    /// Builds a hierarchy over `triangles`.
    pub fn build(triangles: &[Triangle]) -> Self {
        let bounds: Vec<_> = triangles.iter().map(Triangle::bounds).collect();
        Self {
            triangles: triangles.to_vec(),
            tree: Tree::build(&bounds),
        }
    }

    /// The nearest hit along `ray`.
    pub fn intersect(&self, ray: &Ray) -> Option<Hit> {
        let mut nearest = None;
        // Each hit shortens the ray, so every later one is nearer.
        self.tree.walk(&mut ray.clone(), |prim, ray| {
            if let Some(hit) = self.triangles[prim as usize].intersect(ray, prim) {
                ray.t_max = hit.t;
                nearest = Some(hit);
            }
            false
        });
        nearest
    }

    /// True if anything blocks `ray` (shadow rays; stops at the first hit).
    pub fn occluded(&self, ray: &Ray) -> bool {
        let mut blocked = false;
        self.tree.walk(&mut ray.clone(), |prim, ray| {
            blocked = self.triangles[prim as usize].intersect(ray, prim).is_some();
            blocked
        });
        blocked
    }

    /// The triangles, in the caller's order.
    pub fn triangles(&self) -> &[Triangle] {
        &self.triangles
    }

    /// The mesh's bounds (min, max); empty meshes have inverted bounds.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.tree.nodes().first().map_or(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |n| (n.min, n.max),
        )
    }

    /// The flattened nodes (root first).
    pub fn nodes(&self) -> &[Node] {
        self.tree.nodes()
    }

    /// Triangle indices in leaf order.
    pub fn order(&self) -> &[u32] {
        self.tree.order()
    }
}

/// The entry distance into `node`'s box, if the ray crosses it within its range.
fn slab(node: &Node, ray: &Ray, inv: Vec3) -> Option<f32> {
    let t0 = (node.min - ray.origin) * inv;
    let t1 = (node.max - ray.origin) * inv;
    // min/max ignore NaN (0 · ∞ on a slab boundary), so such an axis never culls.
    let near = t0.min(t1).max_element().max(ray.t_min);
    let exit = t0.max(t1).min_element() * ROBUST;
    (near <= exit.min(ray.t_max)).then_some(near)
}

/// Builds nodes over `order[start..end]`.
struct Builder {
    centroids: Vec<Vec3>,
    bounds: Vec<(Vec3, Vec3)>,
}

impl Builder {
    fn node(&self, nodes: &mut Vec<Node>, order: &mut [u32], start: usize, end: usize) -> u32 {
        let index = nodes.len() as u32;
        let (min, max) = self.span(&order[start..end]);
        nodes.push(Node {
            min,
            max,
            first: start as u32,
            count: (end - start) as u32,
        });
        if end - start <= MAX_LEAF {
            return index;
        }
        let Some(mid) = self.split(order, start, end) else {
            return index;
        };
        self.node(nodes, order, start, mid);
        let right = self.node(nodes, order, mid, end);
        nodes[index as usize].first = right;
        nodes[index as usize].count = 0;
        index
    }

    fn span(&self, prims: &[u32]) -> (Vec3, Vec3) {
        prims.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), &p| {
                let (a, b) = self.bounds[p as usize];
                (lo.min(a), hi.max(b))
            },
        )
    }

    /// Partitions `order[start..end]` at the cheapest binned SAH split; None when no split beats a
    /// leaf (or all centroids coincide).
    fn split(&self, order: &mut [u32], start: usize, end: usize) -> Option<usize> {
        let prims = &order[start..end];
        let (cmin, cmax) = prims.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), &p| {
                (
                    lo.min(self.centroids[p as usize]),
                    hi.max(self.centroids[p as usize]),
                )
            },
        );
        let extent = cmax - cmin;
        let axis = if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        };
        if extent[axis] <= 0.0 {
            return None;
        }
        let bin_of = |p: u32| {
            let f = (self.centroids[p as usize][axis] - cmin[axis]) / extent[axis];
            ((f * BINS as f32) as usize).min(BINS - 1)
        };
        let empty = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        let mut bins = [(empty, 0usize); BINS];
        for &p in prims {
            let b = &mut bins[bin_of(p)];
            let (lo, hi) = self.bounds[p as usize];
            b.0 = (b.0.0.min(lo), b.0.1.max(hi));
            b.1 += 1;
        }
        let area = |(lo, hi): (Vec3, Vec3)| {
            let d = (hi - lo).max(Vec3::ZERO);
            d.x * d.y + d.y * d.z + d.z * d.x
        };
        let merge = |a: (Vec3, Vec3), b: (Vec3, Vec3)| (a.0.min(b.0), a.1.max(b.1));
        let (mut best_cost, mut best) = (area(self.span(prims)) * prims.len() as f32, None);
        for split in 1..BINS {
            let (l, nl) = bins[..split]
                .iter()
                .fold((empty, 0), |(b, n), (bb, c)| (merge(b, *bb), n + c));
            let (r, nr) = bins[split..]
                .iter()
                .fold((empty, 0), |(b, n), (bb, c)| (merge(b, *bb), n + c));
            if nl == 0 || nr == 0 {
                continue;
            }
            let cost = area(l) * nl as f32 + area(r) * nr as f32;
            if cost < best_cost {
                (best_cost, best) = (cost, Some(split));
            }
        }
        let split = best?;
        let slice = &mut order[start..end];
        slice.sort_by_key(|&p| (bin_of(p) >= split, p));
        let left = slice.iter().filter(|&&p| bin_of(p) < split).count();
        Some(start + left)
    }
}
