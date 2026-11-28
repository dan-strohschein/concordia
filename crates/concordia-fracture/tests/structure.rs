//! The structure that holds pieces together (M8 8.2): the connection graph, cluster levels and
//! mass. Sources: a 2 × 1 × 1 slab (volume 2) and an L-shaped prism (volume 3).

use concordia_fracture::{
    Breakable, ClusterLevel, EdgeOverride, FractureError, Grouping, Mesh, Pattern, StructureSpec,
    fracture, structure,
};

fn extrude(polygon: &[[f32; 2]], h: f32) -> Mesh {
    let n = polygon.len() as u32;
    let mut positions: Vec<[f32; 3]> = polygon.iter().map(|p| [p[0], p[1], 0.0]).collect();
    positions.extend(polygon.iter().map(|p| [p[0], p[1], h]));
    let mut triangles = Vec::new();
    for i in 1..n - 1 {
        triangles.push([0, i + 1, i]);
        triangles.push([n, n + i, n + i + 1]);
    }
    for i in 0..n {
        let j = (i + 1) % n;
        triangles.push([i, j, j + n]);
        triangles.push([i, j + n, i + n]);
    }
    Mesh {
        positions,
        triangles,
    }
}

fn slab() -> Mesh {
    extrude(&[[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]], 1.0)
}

fn ell() -> Mesh {
    let l = [
        [0.0, 0.0],
        [2.0, 0.0],
        [2.0, 1.0],
        [1.0, 1.0],
        [1.0, 2.0],
        [0.0, 2.0],
    ];
    extrude(&l, 1.0)
}

fn spec() -> StructureSpec {
    StructureSpec {
        density: 1000.0,
        strength: 10.0,
        levels: Vec::new(),
        edges: Vec::new(),
    }
}

/// The edges as (cell a, cell b, area), a < b, sorted.
fn edges(s: &concordia_fracture::Structure) -> Vec<(u32, u32, f64)> {
    s.edges.iter().map(|e| (e.a, e.b, e.area)).collect()
}

#[test]
fn touching_pieces_are_connected_and_separate_ones_are_not() {
    let slices = fracture(
        &slab(),
        &Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 4,
        },
        0,
    )
    .unwrap();
    let s = structure(&slices, &spec()).unwrap();
    let got = edges(&s);
    assert_eq!(
        got.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(),
        [(0, 1), (1, 2), (2, 3)]
    );
    assert!(got.iter().all(|e| (e.2 - 1.0).abs() < 1e-5), "{got:?}");

    // A running bond: two rows of 1 × 0.5 bricks, the upper offset by half a brick. Each lower
    // brick touches its neighbour and two upper ones; the upper row's three touch in a line.
    let bricks = fracture(
        &slab(),
        &Pattern::Bricks {
            size: [1.0, 0.5, 1.0],
        },
        0,
    )
    .unwrap();
    let s = structure(&bricks, &spec()).unwrap();
    let got = edges(&s);
    assert_eq!(got.len(), 7, "{got:?}");
    assert!(got.iter().all(|e| (e.2 - 0.5).abs() < 1e-5), "{got:?}");
    // The lower-left brick and the upper-right half brick don't touch.
    let cell_at = |x: f32, y: f32| {
        bricks
            .pieces
            .iter()
            .find(|p| {
                let c = p.centre_of_mass();
                (c.x - x).abs() < 0.3 && (c.y - y).abs() < 0.1
            })
            .unwrap()
            .cell
    };
    let (lower_left, upper_right) = (cell_at(0.5, 0.25), cell_at(1.75, 0.75));
    let (a, b) = (lower_left.min(upper_right), lower_left.max(upper_right));
    assert!(!got.iter().any(|e| (e.0, e.1) == (a, b)), "{got:?}");
}

#[test]
fn edge_strength_scales_with_contact_area() {
    let cut = |normal| {
        let f = fracture(&slab(), &Pattern::Slices { normal, count: 2 }, 0).unwrap();
        structure(&f, &spec()).unwrap().edges[0].clone()
    };
    // Across x the cut face is 1 × 1; across z it is 2 × 1.
    let (across_x, across_z) = (cut([1.0, 0.0, 0.0]), cut([0.0, 0.0, 1.0]));
    assert!((across_x.area - 1.0).abs() < 1e-5 && (across_z.area - 2.0).abs() < 1e-5);
    assert!((across_x.strength - 10.0).abs() < 1e-4, "{across_x:?}");
    assert!((across_z.strength - 20.0).abs() < 1e-4, "{across_z:?}");
}

#[test]
fn an_edge_override_replaces_its_strength() {
    let f = fracture(
        &slab(),
        &Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 3,
        },
        0,
    )
    .unwrap();
    let mut s = spec();
    s.edges = vec![EdgeOverride {
        a: 2,
        b: 1,
        strength: 99.0,
    }];
    let got = structure(&f, &s).unwrap();
    let strength = |a, b| {
        got.edges
            .iter()
            .find(|e| (e.a, e.b) == (a, b))
            .unwrap()
            .strength
    };
    assert_eq!(strength(1, 2), 99.0, "either order names the edge");
    // The other cut keeps area (1 × 1) × strength.
    assert!((strength(0, 1) - 10.0).abs() < 1e-4);
}

#[test]
fn cluster_mass_is_the_sum_of_its_pieces() {
    let f = fracture(&ell(), &Pattern::Uniform { cells: 12 }, 4).unwrap();
    let mut s = spec();
    s.density = 2000.0;
    s.levels = vec![
        ClusterLevel {
            grouping: Grouping::KMeans { clusters: 3 },
            damage_threshold: 50.0,
        },
        ClusterLevel {
            grouping: Grouping::KMeans { clusters: 1 },
            damage_threshold: 200.0,
        },
    ];
    let got = structure(&f, &s).unwrap();
    assert_eq!(got.pieces.len(), f.pieces.len());
    for (mass, piece) in got.pieces.iter().zip(&f.pieces) {
        assert!((mass.mass - 2000.0 * piece.volume).abs() < 1e-3);
    }
    assert_eq!(got.levels.len(), 2);
    let level1 = &got.levels[0];
    assert!(!level1.clusters.is_empty() && level1.clusters.len() <= 3);
    // Every piece is in exactly one level-1 cluster, whose mass is its members' sum.
    let mut seen = vec![0; f.pieces.len()];
    for c in &level1.clusters {
        let sum: f64 = c.pieces.iter().map(|&p| got.pieces[p as usize].mass).sum();
        assert!((c.mass - sum).abs() < 1e-6, "{} vs {sum}", c.mass);
        assert_eq!(c.damage_threshold, 50.0);
        for &p in &c.pieces {
            seen[p as usize] += 1;
        }
    }
    assert!(seen.iter().all(|&n| n == 1), "{seen:?}");
    let top = &got.levels[1].clusters;
    assert_eq!(top.len(), 1);
    assert!((top[0].mass - 6000.0).abs() < 1e-2, "{}", top[0].mass);
    assert_eq!(top[0].members.len(), level1.clusters.len());
    // Its centre is the mass-weighted mean (the L's centroid: (5/6, 5/6, 1/2)).
    let c = top[0].centre;
    assert!((c[0] - 5.0 / 6.0).abs() < 1e-4 && (c[1] - 5.0 / 6.0).abs() < 1e-4);
    assert!((c[2] - 0.5).abs() < 1e-4, "{c:?}");
    assert_eq!(structure(&f, &s).unwrap(), got, "deterministic");
}

#[test]
fn manual_grouping_names_cells_and_level_edges_join_clusters() {
    let f = fracture(
        &slab(),
        &Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 4,
        },
        0,
    )
    .unwrap();
    let mut s = spec();
    s.levels = vec![ClusterLevel {
        grouping: Grouping::Manual {
            groups: vec![vec![0, 1], vec![2, 3]],
        },
        damage_threshold: 5.0,
    }];
    let got = structure(&f, &s).unwrap();
    let level = &got.levels[0];
    assert_eq!(level.clusters.len(), 2);
    assert_eq!(level.clusters[0].pieces, [0, 1]);
    // One cut joins the two halves: the edge between cells 1 and 2.
    assert_eq!(level.edges.len(), 1);
    let e = &level.edges[0];
    assert_eq!((e.a, e.b), (0, 1));
    assert!((e.area - 1.0).abs() < 1e-5 && (e.strength - 10.0).abs() < 1e-4);
}

#[test]
fn the_structure_spec_round_trips_through_breakable_ron_canonically() {
    let recipe = Breakable {
        source: "asset:01a220ac-4979-71b7-8418-4d7a8c055055#mesh/mesh/0".into(),
        pattern: Pattern::Uniform { cells: 8 },
        seed: 2,
        metres_per_tile: 0.5,
        interior_material: None,
        structure: StructureSpec {
            density: 2400.0,
            strength: 1.5e5,
            levels: vec![
                ClusterLevel {
                    grouping: Grouping::Manual {
                        groups: vec![vec![0, 1, 2], vec![3, 4, 5, 6, 7]],
                    },
                    damage_threshold: 1000.0,
                },
                ClusterLevel {
                    grouping: Grouping::KMeans { clusters: 1 },
                    damage_threshold: 5000.0,
                },
            ],
            edges: vec![EdgeOverride {
                a: 0,
                b: 3,
                strength: 1.0,
            }],
        },
    };
    let text = concordia_scene::write_document(&recipe).unwrap();
    let back: Breakable = concordia_scene::read_document(&text).unwrap();
    assert_eq!(back, recipe);
    assert_eq!(concordia_scene::write_document(&back).unwrap(), text);
}

#[test]
fn bad_specs_are_errors() {
    let f = fracture(
        &slab(),
        &Pattern::Slices {
            normal: [1.0, 0.0, 0.0],
            count: 3,
        },
        0,
    )
    .unwrap();
    let level = |grouping| ClusterLevel {
        grouping,
        damage_threshold: 1.0,
    };
    let bad = [
        StructureSpec {
            density: 0.0,
            ..spec()
        },
        StructureSpec {
            strength: f32::NAN,
            ..spec()
        },
        StructureSpec {
            levels: vec![level(Grouping::KMeans { clusters: 0 })],
            ..spec()
        },
        StructureSpec {
            levels: vec![level(Grouping::Manual {
                groups: vec![vec![0, 1]],
            })],
            ..spec()
        },
        StructureSpec {
            levels: vec![level(Grouping::Manual {
                groups: vec![vec![0, 1], vec![1, 2]],
            })],
            ..spec()
        },
        StructureSpec {
            levels: vec![ClusterLevel {
                grouping: Grouping::KMeans { clusters: 1 },
                damage_threshold: -1.0,
            }],
            ..spec()
        },
        StructureSpec {
            edges: vec![EdgeOverride {
                a: 0,
                b: 2,
                strength: 1.0,
            }],
            ..spec()
        },
    ];
    for s in bad {
        assert!(
            matches!(structure(&f, &s), Err(FractureError::InvalidParameter(_))),
            "{s:?}"
        );
    }
}
