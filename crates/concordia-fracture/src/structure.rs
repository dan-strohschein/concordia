//! What holds pieces together (M8 8.2): the connection graph, cluster levels, mass and damage
//! thresholds. Everything is computed in `f64` from the pieces, in a fixed order.

use std::collections::BTreeMap;

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::{Fracture, FractureError, Piece, Slot};

/// The authored part of a breakable's structure (in `.breakable.ron`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct StructureSpec {
    /// Density in kg/m³ (stone is about 2400).
    pub density: f32,
    /// Bond strength per square metre of contact between two pieces.
    pub strength: f32,
    /// Cluster levels above the pieces, lowest first.
    pub levels: Vec<ClusterLevel>,
    /// Per-edge strengths that replace the computed ones (cells, either order).
    pub edges: Vec<EdgeOverride>,
}

impl Default for StructureSpec {
    fn default() -> Self {
        Self {
            density: 2400.0,
            strength: 1.0e5,
            levels: Vec::new(),
            edges: Vec::new(),
        }
    }
}

/// One cluster level: how the level below groups, and the damage that breaks its clusters apart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ClusterLevel {
    /// How the nodes of the level below group.
    pub grouping: Grouping,
    /// Damage a cluster of this level takes before it breaks.
    pub damage_threshold: f32,
}

/// How a level groups the level below.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum Grouping {
    /// Seedless k-means over the nodes' centres of mass.
    KMeans {
        /// The number of clusters (fewer if there are fewer nodes).
        clusters: u32,
    },
    /// Groups by hand: cells for the first level, the level below's cluster indices above it.
    Manual {
        /// Each group's members; together they name every node once.
        groups: Vec<Vec<u32>>,
    },
}

/// A strength that replaces an edge's computed one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct EdgeOverride {
    /// One cell.
    pub a: u32,
    /// The other.
    pub b: u32,
    /// The edge's strength.
    pub strength: f32,
}

/// A piece's mass properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PieceMass {
    /// Its cell.
    pub cell: u32,
    /// Its mass (kg).
    pub mass: f64,
    /// Its centre of mass.
    pub centre: [f64; 3],
}

/// A bond between two nodes of one level (cells for pieces, cluster indices above), `a < b`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    /// One end.
    pub a: u32,
    /// The other end.
    pub b: u32,
    /// The contact area (m²).
    pub area: f64,
    /// The bond's strength.
    pub strength: f64,
}

/// A cluster of one level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cluster {
    /// Its members: piece indices for the first level, the level below's cluster indices above.
    pub members: Vec<u32>,
    /// Every piece in it (piece indices), ascending.
    pub pieces: Vec<u32>,
    /// Its mass (kg).
    pub mass: f64,
    /// Its centre of mass.
    pub centre: [f64; 3],
    /// The damage it takes before it breaks.
    pub damage_threshold: f32,
}

/// One level's clusters and the bonds between them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Level {
    /// Its clusters, ordered by their lowest piece.
    pub clusters: Vec<Cluster>,
    /// Bonds between clusters: the sum of the piece bonds that cross.
    pub edges: Vec<Edge>,
}

/// A fractured mesh's structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Structure {
    /// Mass per piece, in piece order.
    pub pieces: Vec<PieceMass>,
    /// Bonds between touching pieces, by cell, sorted.
    pub edges: Vec<Edge>,
    /// Cluster levels, lowest first.
    pub levels: Vec<Level>,
}

/// The structure of `fracture` under `spec`.
pub fn structure(fracture: &Fracture, spec: &StructureSpec) -> Result<Structure, FractureError> {
    validate(spec)?;
    let pieces: Vec<PieceMass> = fracture
        .pieces
        .iter()
        .map(|p| PieceMass {
            cell: p.cell,
            mass: p.volume * f64::from(spec.density),
            centre: p.centre_of_mass().to_array().map(f64::from),
        })
        .collect();
    let mut edges = contacts(&fracture.pieces, f64::from(spec.strength));
    apply_overrides(&mut edges, &spec.edges)?;
    let index: BTreeMap<u32, u32> = fracture
        .pieces
        .iter()
        .enumerate()
        .map(|(i, p)| (p.cell, i as u32))
        .collect();
    // Piece bonds by piece index, for grouping.
    let bonds: Vec<Edge> = edges
        .iter()
        .map(|e| Edge {
            a: index[&e.a],
            b: index[&e.b],
            ..e.clone()
        })
        .collect();
    let mut levels: Vec<Level> = Vec::new();
    for (depth, level) in spec.levels.iter().enumerate() {
        let nodes: Vec<Node> = match levels.last() {
            None => pieces
                .iter()
                .enumerate()
                .map(|(i, p)| Node {
                    name: p.cell,
                    pieces: vec![i as u32],
                    mass: p.mass,
                    centre: p.centre,
                })
                .collect(),
            Some(below) => below
                .clusters
                .iter()
                .enumerate()
                .map(|(i, c)| Node {
                    name: i as u32,
                    pieces: c.pieces.clone(),
                    mass: c.mass,
                    centre: c.centre,
                })
                .collect(),
        };
        let groups = match &level.grouping {
            Grouping::KMeans { clusters } => kmeans(&nodes, *clusters as usize),
            Grouping::Manual { groups } => manual(&nodes, groups, depth)?,
        };
        levels.push(build_level(&nodes, groups, level.damage_threshold, &bonds));
    }
    Ok(Structure {
        pieces,
        edges,
        levels,
    })
}

fn invalid<T>(why: impl Into<String>) -> Result<T, FractureError> {
    Err(FractureError::InvalidParameter(why.into()))
}

fn validate(spec: &StructureSpec) -> Result<(), FractureError> {
    if !(spec.density.is_finite() && spec.density > 0.0) {
        return invalid("`density` must be a positive number");
    }
    if !(spec.strength.is_finite() && spec.strength > 0.0) {
        return invalid("`strength` must be a positive number");
    }
    for (i, level) in spec.levels.iter().enumerate() {
        if !(level.damage_threshold.is_finite() && level.damage_threshold >= 0.0) {
            return invalid(format!("level {i}: `damage_threshold` must be 0 or more"));
        }
        if level.grouping == (Grouping::KMeans { clusters: 0 }) {
            return invalid(format!("level {i}: k-means needs at least one cluster"));
        }
    }
    for o in &spec.edges {
        if !(o.strength.is_finite() && o.strength >= 0.0) || o.a == o.b {
            return invalid(format!(
                "edge {}–{}: two different cells and a strength of 0 or more",
                o.a, o.b
            ));
        }
    }
    Ok(())
}

/// Bonds between pieces whose interior faces meet face to face, sorted by cell.
fn contacts(pieces: &[Piece], strength: f64) -> Vec<Edge> {
    let faces: Vec<Vec<Face>> = pieces.iter().map(interior_faces).collect();
    let boxes: Vec<([f64; 3], [f64; 3])> = pieces.iter().map(aabb).collect();
    let mut edges = Vec::new();
    for i in 0..pieces.len() {
        for j in i + 1..pieces.len() {
            if !touching_boxes(boxes[i], boxes[j]) {
                continue;
            }
            let area: f64 = faces[i]
                .iter()
                .flat_map(|f| faces[j].iter().map(move |g| overlap(f, g)))
                .sum();
            if area > AREA_EPS {
                let (a, b) = (
                    pieces[i].cell.min(pieces[j].cell),
                    pieces[i].cell.max(pieces[j].cell),
                );
                edges.push(Edge {
                    a,
                    b,
                    area,
                    strength: area * strength,
                });
            }
        }
    }
    edges.sort_by_key(|e| (e.a, e.b));
    edges
}

const PLANE_EPS: f64 = 1e-4;
const AREA_EPS: f64 = 1e-8;

/// An interior triangle: its corners, unit normal and offset along it.
struct Face {
    corners: [[f64; 3]; 3],
    normal: [f64; 3],
    offset: f64,
}

fn interior_faces(p: &Piece) -> Vec<Face> {
    p.triangles
        .iter()
        .zip(&p.materials)
        .filter(|(_, s)| **s == Slot::Interior)
        .filter_map(|(t, _)| {
            let corners = t.map(|i| p.positions[i as usize].map(f64::from));
            let n = cross(sub(corners[1], corners[0]), sub(corners[2], corners[0]));
            let len = dot(n, n).sqrt();
            (len > 1e-12).then(|| {
                let normal = n.map(|v| v / len);
                Face {
                    corners,
                    normal,
                    offset: dot(normal, corners[0]),
                }
            })
        })
        .collect()
}

fn aabb(p: &Piece) -> ([f64; 3], [f64; 3]) {
    p.positions.iter().fold(
        ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
        |(lo, hi), v| {
            (
                [0, 1, 2].map(|a| lo[a].min(f64::from(v[a]))),
                [0, 1, 2].map(|a| hi[a].max(f64::from(v[a]))),
            )
        },
    )
}

fn touching_boxes(a: ([f64; 3], [f64; 3]), b: ([f64; 3], [f64; 3])) -> bool {
    (0..3).all(|k| a.0[k] <= b.1[k] + PLANE_EPS && b.0[k] <= a.1[k] + PLANE_EPS)
}

/// The area two faces share, if they lie on one plane facing each other.
fn overlap(f: &Face, g: &Face) -> f64 {
    if dot(f.normal, g.normal) > -0.9999 || (f.offset + g.offset).abs() > PLANE_EPS {
        return 0.0;
    }
    let (u, v) = tangents(f.normal);
    let flat = |c: &[[f64; 3]; 3]| -> Vec<[f64; 2]> {
        c.iter().map(|p| [dot(*p, u), dot(*p, v)]).collect()
    };
    let subject = ccw(flat(&f.corners));
    let clip = ccw(flat(&g.corners));
    area(&clip_polygon(subject, &clip))
}

fn tangents(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let up = if n[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = cross(up, n);
    let l = dot(u, u).sqrt();
    let u = u.map(|x| x / l);
    (u, cross(n, u))
}

fn ccw(mut p: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    if area_signed(&p) < 0.0 {
        p.reverse();
    }
    p
}

/// Sutherland–Hodgman: `subject` clipped by the convex, counter-clockwise `clip`.
fn clip_polygon(mut subject: Vec<[f64; 2]>, clip: &[[f64; 2]]) -> Vec<[f64; 2]> {
    for k in 0..clip.len() {
        let (a, b) = (clip[k], clip[(k + 1) % clip.len()]);
        let side = |p: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        let input = std::mem::take(&mut subject);
        for i in 0..input.len() {
            let (p, q) = (input[i], input[(i + 1) % input.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0.0 {
                subject.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let t = sp / (sp - sq);
                subject.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
            }
        }
        if subject.is_empty() {
            break;
        }
    }
    subject
}

fn area_signed(p: &[[f64; 2]]) -> f64 {
    (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        * 0.5
}

fn area(p: &[[f64; 2]]) -> f64 {
    if p.len() < 3 {
        0.0
    } else {
        area_signed(p).abs()
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn apply_overrides(edges: &mut [Edge], overrides: &[EdgeOverride]) -> Result<(), FractureError> {
    for o in overrides {
        let (a, b) = (o.a.min(o.b), o.a.max(o.b));
        match edges.iter_mut().find(|e| (e.a, e.b) == (a, b)) {
            Some(e) => e.strength = f64::from(o.strength),
            None => {
                return invalid(format!(
                    "cells {a} and {b} don't touch, so they have no edge"
                ));
            }
        }
    }
    Ok(())
}

/// A node of the level below.
struct Node {
    /// How a manual grouping names it (cell or cluster index).
    name: u32,
    pieces: Vec<u32>,
    mass: f64,
    centre: [f64; 3],
}

fn distance2(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = sub(a, b);
    dot(d, d)
}

/// Mass-weighted k-means with farthest-point starts (no randomness); groups of node indices.
fn kmeans(nodes: &[Node], k: usize) -> Vec<Vec<u32>> {
    let k = k.min(nodes.len());
    if k == 0 {
        return Vec::new();
    }
    let mut centres = vec![nodes[0].centre];
    while centres.len() < k {
        let far = (0..nodes.len())
            .max_by(|&i, &j| {
                let d = |n: usize| {
                    centres
                        .iter()
                        .map(|c| distance2(nodes[n].centre, *c))
                        .fold(f64::INFINITY, f64::min)
                };
                // Ties go to the lower index.
                d(i).total_cmp(&d(j)).then(j.cmp(&i))
            })
            .unwrap_or(0);
        centres.push(nodes[far].centre);
    }
    let mut assignment = vec![usize::MAX; nodes.len()];
    for _ in 0..64 {
        let next: Vec<usize> = nodes
            .iter()
            .map(|n| {
                (0..centres.len())
                    .min_by(|&a, &b| {
                        distance2(n.centre, centres[a])
                            .total_cmp(&distance2(n.centre, centres[b]))
                            .then(a.cmp(&b))
                    })
                    .unwrap_or(0)
            })
            .collect();
        if next == assignment {
            break;
        }
        assignment = next;
        for (c, centre) in centres.iter_mut().enumerate() {
            let members: Vec<&Node> = nodes
                .iter()
                .zip(&assignment)
                .filter(|(_, a)| **a == c)
                .map(|(n, _)| n)
                .collect();
            if let Some(mean) = weighted_centre(members.iter().map(|n| (n.mass, n.centre))) {
                *centre = mean;
            }
        }
    }
    (0..centres.len())
        .map(|c| {
            (0..nodes.len())
                .filter(|&i| assignment[i] == c)
                .map(|i| i as u32)
                .collect::<Vec<u32>>()
        })
        .filter(|g| !g.is_empty())
        .collect()
}

/// A hand-made grouping, checked to name every node once; groups of node indices.
fn manual(
    nodes: &[Node],
    groups: &[Vec<u32>],
    depth: usize,
) -> Result<Vec<Vec<u32>>, FractureError> {
    let by_name: BTreeMap<u32, u32> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name, i as u32))
        .collect();
    let what = if depth == 0 { "cell" } else { "cluster" };
    let mut seen = vec![false; nodes.len()];
    let mut out = Vec::new();
    for group in groups {
        let mut members = Vec::new();
        for name in group {
            let Some(&i) = by_name.get(name) else {
                return invalid(format!("level {depth}: no piece for {what} {name}"));
            };
            if std::mem::replace(&mut seen[i as usize], true) {
                return invalid(format!("level {depth}: {what} {name} is in two groups"));
            }
            members.push(i);
        }
        if !members.is_empty() {
            out.push(members);
        }
    }
    if let Some(missing) = seen.iter().position(|s| !s) {
        return invalid(format!(
            "level {depth}: {what} {} is in no group",
            nodes[missing].name
        ));
    }
    Ok(out)
}

fn weighted_centre(items: impl Iterator<Item = (f64, [f64; 3])>) -> Option<[f64; 3]> {
    let (mut mass, mut moment) = (0.0, [0.0; 3]);
    for (m, c) in items {
        mass += m;
        for k in 0..3 {
            moment[k] += m * c[k];
        }
    }
    (mass > 0.0).then(|| moment.map(|v| v / mass))
}

fn build_level(nodes: &[Node], groups: Vec<Vec<u32>>, threshold: f32, bonds: &[Edge]) -> Level {
    let mut clusters: Vec<Cluster> = groups
        .into_iter()
        .map(|members| {
            let mut pieces: Vec<u32> = members
                .iter()
                .flat_map(|&m| nodes[m as usize].pieces.iter().copied())
                .collect();
            pieces.sort_unstable();
            let parts = || members.iter().map(|&m| &nodes[m as usize]);
            Cluster {
                mass: parts().map(|n| n.mass).sum(),
                centre: weighted_centre(parts().map(|n| (n.mass, n.centre))).unwrap_or_default(),
                members,
                pieces,
                damage_threshold: threshold,
            }
        })
        .collect();
    clusters.sort_by_key(|c| c.pieces.first().copied());
    let mut owner = BTreeMap::new();
    for (i, c) in clusters.iter().enumerate() {
        for &p in &c.pieces {
            owner.insert(p, i as u32);
        }
    }
    let mut joined: BTreeMap<(u32, u32), (f64, f64)> = BTreeMap::new();
    for e in bonds {
        let (ca, cb) = (owner[&e.a], owner[&e.b]);
        if ca != cb {
            let entry = joined.entry((ca.min(cb), ca.max(cb))).or_default();
            entry.0 += e.area;
            entry.1 += e.strength;
        }
    }
    Level {
        clusters,
        edges: joined
            .into_iter()
            .map(|((a, b), (area, strength))| Edge {
                a,
                b,
                area,
                strength,
            })
            .collect(),
    }
}
