//! A built navmesh and its queries. Queries borrow a Detour query object from a pool (sharded by
//! thread, so many threads rarely wait on one lock), so a navmesh can be queried from many threads
//! at once with the same results: every Detour call resets the object it is given.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use concordia_core::math::Vec3;
use recast_sys::{AreaCosts, DtNavMesh, DtQuery, NavMeshParams, PolyRef, TileData};

use crate::data::NavMeshData;
use crate::error::NavError;

/// Search nodes per query (Detour's most).
const MAX_NODES: u16 = u16::MAX;
/// Longest corridor a path query returns, in polygons.
const MAX_CORRIDOR: usize = 8192;

/// One polygon, for drawing and inspection.
#[derive(Debug, Clone, PartialEq)]
pub struct NavPoly {
    /// Vertices, counter-clockwise seen from above.
    pub vertices: Vec<Vec3>,
    /// Recast area id (63 walkable, 2..=62 cost areas).
    pub area: u8,
    /// Connected component, numbered in polygon order.
    pub island: u32,
}

/// Summary numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavStats {
    /// Built tiles.
    pub tiles: usize,
    /// Polygons.
    pub polygons: usize,
    /// Connected components.
    pub islands: usize,
    /// Bounds of every polygon (zero when empty).
    pub min: Vec3,
    /// Bounds of every polygon.
    pub max: Vec3,
}

/// A point on the navmesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavPoint {
    /// Position on the surface.
    pub position: Vec3,
    /// Its island.
    pub island: u32,
}

/// A path's corner points.
#[derive(Debug, Clone, PartialEq)]
pub struct NavPath {
    /// From the snapped start to the snapped end (or the closest reachable point), inclusive.
    pub waypoints: Vec<Vec3>,
    /// The end isn't reachable; the path ends at the closest reachable point.
    pub partial: bool,
    /// Total length in meters.
    pub length: f32,
    /// The polygon corridor (raw Detour references), from the start's polygon.
    pub corridor: Vec<u64>,
}

/// A surface move's end and the polygons it crossed.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceMove {
    /// The reached point (its height is the target's; see [`NavMesh::poly_height`]).
    pub position: Vec3,
    /// Polygons crossed (raw references), from the start's.
    pub visited: Vec<u64>,
}

/// A surface raycast's stop and the polygons it crossed.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceRay {
    /// The fraction along the ray where it hit an edge, or `None` if it reached the end.
    pub hit: Option<f32>,
    /// Polygons crossed (raw references), from the start's.
    pub polys: Vec<u64>,
}

/// A surface raycast's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavRay {
    /// Where the walk stopped at an edge, or `None` if it reached the end.
    pub hit: Option<Vec3>,
    /// The edge's normal (zero without a hit).
    pub normal: Vec3,
}

/// A navmesh with its polygons and islands, safe to query from any thread.
#[derive(Debug)]
pub struct NavMesh {
    data: NavMeshData,
    mesh: Arc<DtNavMesh>,
    costs: AreaCosts,
    polygons: Vec<NavPoly>,
    index: HashMap<PolyRef, usize>,
    islands: usize,
    queries: [Mutex<Vec<DtQuery>>; SHARDS],
}

/// Query pools per navmesh; a thread uses the one its id hashes to.
const SHARDS: usize = 16;

impl NavMesh {
    /// Loads built data into Detour.
    pub fn from_data(data: NavMeshData) -> Result<Self, NavError> {
        let tiles = data.grid[0].max(1).saturating_mul(data.grid[1].max(1));
        let params = NavMeshParams {
            origin: data.origin,
            tile_width: data.tile_width,
            tile_depth: data.tile_width,
            max_tiles: tiles,
            max_polys_per_tile: 1 << 16,
        };
        let mut mesh = DtNavMesh::new(&params)?;
        for (_, bytes) in &data.tiles {
            mesh.add_tile(&TileData::from_bytes(bytes.clone()))?;
        }
        let mut costs = AreaCosts::default();
        for (id, cost) in &data.costs {
            costs.set(*id, *cost)?;
        }
        let raw = mesh.polys();
        let index: HashMap<PolyRef, usize> = raw
            .iter()
            .enumerate()
            .map(|(i, p)| (p.reference, i))
            .collect();
        let (island_of, islands) = islands(&mesh, &raw, &index);
        let polygons = raw
            .iter()
            .zip(island_of)
            .map(|(p, island)| NavPoly {
                vertices: p.vertices.iter().map(|v| Vec3::from_array(*v)).collect(),
                area: p.area.id(),
                island,
            })
            .collect();
        Ok(Self {
            data,
            mesh: Arc::new(mesh),
            costs,
            polygons,
            index,
            islands,
            queries: std::array::from_fn(|_| Mutex::new(Vec::new())),
        })
    }

    /// The data it was loaded from.
    pub fn data(&self) -> &NavMeshData {
        &self.data
    }

    /// Summary numbers.
    pub fn stats(&self) -> NavStats {
        let points = || {
            self.polygons
                .iter()
                .flat_map(|p| p.vertices.iter().copied())
        };
        let (min, max) = match points().next() {
            Some(first) => points().fold((first, first), |(lo, hi), v| (lo.min(v), hi.max(v))),
            None => (Vec3::ZERO, Vec3::ZERO),
        };
        NavStats {
            tiles: self.data.tiles.len(),
            polygons: self.polygons.len(),
            islands: self.islands,
            min,
            max,
        }
    }

    /// Every polygon, tile by tile.
    pub fn polygons(&self) -> &[NavPoly] {
        &self.polygons
    }

    /// The nearest point on the navmesh within the search reach.
    pub fn nearest_point(&self, point: Vec3) -> Option<NavPoint> {
        self.nearest_within(point, Vec3::from_array(self.data.search))
            .ok()
            .flatten()
            .map(|(_, p)| p)
    }

    /// The island directly under (or over) `point`, if it lies on the navmesh.
    pub fn island_at(&self, point: Vec3) -> Option<u32> {
        let reach = Vec3::new(0.01, self.data.search[1], 0.01);
        let (_, found) = self.nearest_within(point, reach).ok().flatten()?;
        let (dx, dz) = (found.position.x - point.x, found.position.z - point.z);
        (dx.abs() <= 1e-3 && dz.abs() <= 1e-3).then_some(found.island)
    }

    /// The shortest path from `from` to `to`, both snapped to the navmesh.
    pub fn path(&self, from: Vec3, to: Vec3) -> Result<NavPath, NavError> {
        let reach = Vec3::from_array(self.data.search);
        let (start, a) = self
            .nearest_within(from, reach)?
            .ok_or(NavError::OffNavmesh(from))?;
        let (end, b) = self
            .nearest_within(to, reach)?
            .ok_or(NavError::OffNavmesh(to))?;
        self.with_query(|q| {
            let corridor = q.find_path(
                &self.costs,
                (start, a.position.to_array()),
                (end, b.position.to_array()),
                MAX_CORRIDOR,
            )?;
            // A partial corridor's end is clamped to its last polygon by Detour.
            let points = q.straight_path(
                a.position.to_array(),
                b.position.to_array(),
                &corridor.polys,
                corridor.polys.len() + 2,
            )?;
            let waypoints: Vec<Vec3> = points.into_iter().map(Vec3::from_array).collect();
            let length = waypoints.windows(2).map(|w| w[0].distance(w[1])).sum();
            Ok(NavPath {
                waypoints,
                partial: corridor.partial,
                length,
                corridor: corridor.polys.iter().map(|p| p.0.get()).collect(),
            })
        })
    }

    /// Walks the surface from `from` toward `to`, stopping at an edge.
    pub fn raycast(&self, from: Vec3, to: Vec3) -> Result<NavRay, NavError> {
        let reach = Vec3::from_array(self.data.search);
        let (poly, start) = self
            .nearest_within(from, reach)?
            .ok_or(NavError::OffNavmesh(from))?;
        let s = start.position;
        let end = Vec3::new(to.x, s.y, to.z);
        let ray =
            self.with_query(|q| Ok(q.raycast(&self.costs, poly, s.to_array(), end.to_array())?))?;
        Ok(NavRay {
            hit: ray.hit.map(|t| s.lerp(end, t)),
            normal: Vec3::from_array(ray.normal),
        })
    }

    /// The nearest polygon (a raw reference) and point within the search reach.
    pub fn nearest_poly(&self, point: Vec3) -> Option<(u64, NavPoint)> {
        self.nearest_within(point, Vec3::from_array(self.data.search))
            .ok()
            .flatten()
            .map(|(poly, p)| (poly.0.get(), p))
    }

    /// Corners along `corridor` from `from` to `to` (both included), at most `max`; an end
    /// outside the last polygon is clamped to it.
    pub fn straight_path(
        &self,
        from: Vec3,
        to: Vec3,
        corridor: &[u64],
        max: usize,
    ) -> Result<Vec<Vec3>, NavError> {
        let polys = corridor
            .iter()
            .map(|&r| poly_ref(r))
            .collect::<Result<Vec<_>, _>>()?;
        let points = self
            .with_query(|q| Ok(q.straight_path(from.to_array(), to.to_array(), &polys, max)?))?;
        Ok(points.into_iter().map(Vec3::from_array).collect())
    }

    /// Moves from `from` (on `poly`) toward `to` along the surface, sliding along walls and
    /// crossing at most `max_visited` polygons.
    pub fn move_along_surface(
        &self,
        poly: u64,
        from: Vec3,
        to: Vec3,
        max_visited: usize,
    ) -> Result<SurfaceMove, NavError> {
        let poly = poly_ref(poly)?;
        let moved = self.with_query(|q| {
            Ok(q.move_along_surface(
                &self.costs,
                poly,
                from.to_array(),
                to.to_array(),
                max_visited,
            )?)
        })?;
        Ok(SurfaceMove {
            position: Vec3::from_array(moved.position),
            visited: moved.visited.iter().map(|p| p.0.get()).collect(),
        })
    }

    /// The surface height of `poly` under `at`, or `None` when `at` is outside it.
    pub fn poly_height(&self, poly: u64, at: Vec3) -> Result<Option<f32>, NavError> {
        let poly = poly_ref(poly)?;
        self.with_query(|q| Ok(q.poly_height(poly, at.to_array())?))
    }

    /// Walks the surface from `from` (on `poly`) toward `to`, listing at most `max` polygons.
    pub fn raycast_path(
        &self,
        poly: u64,
        from: Vec3,
        to: Vec3,
        max: usize,
    ) -> Result<SurfaceRay, NavError> {
        let poly = poly_ref(poly)?;
        let ray = self.with_query(|q| {
            Ok(q.raycast_path(&self.costs, poly, from.to_array(), to.to_array(), max)?)
        })?;
        Ok(SurfaceRay {
            hit: ray.ray.hit,
            polys: ray.polys.iter().map(|p| p.0.get()).collect(),
        })
    }

    /// Up to `max` polygons around `poly` (its own first) within `radius` of `center`
    /// (`findLocalNeighbourhood`).
    pub fn local_neighbourhood(
        &self,
        poly: u64,
        center: Vec3,
        radius: f32,
        max: usize,
    ) -> Result<Vec<u64>, NavError> {
        let poly = poly_ref(poly)?;
        let found = self.with_query(|q| {
            Ok(q.local_neighbourhood(&self.costs, poly, center.to_array(), radius, max)?)
        })?;
        Ok(found.iter().map(|p| p.0.get()).collect())
    }

    /// Up to `max` of `poly`'s walls, the edges with no passable neighbour (`getPolyWallSegments`).
    pub fn wall_segments(&self, poly: u64, max: usize) -> Result<Vec<[Vec3; 2]>, NavError> {
        let poly = poly_ref(poly)?;
        let walls = self.with_query(|q| Ok(q.wall_segments(&self.costs, poly, max)?))?;
        Ok(walls
            .iter()
            .map(|[p, q]| [Vec3::from_array(*p), Vec3::from_array(*q)])
            .collect())
    }

    fn nearest_within(
        &self,
        point: Vec3,
        reach: Vec3,
    ) -> Result<Option<(PolyRef, NavPoint)>, NavError> {
        let found =
            self.with_query(|q| Ok(q.nearest(&self.costs, point.to_array(), reach.to_array())?))?;
        Ok(found.and_then(|(poly, p)| {
            let island = self.polygons.get(*self.index.get(&poly)?)?.island;
            Some((
                poly,
                NavPoint {
                    position: Vec3::from_array(p),
                    island,
                },
            ))
        }))
    }

    /// Runs `f` with a pooled query object.
    fn with_query<T>(
        &self,
        f: impl FnOnce(&mut DtQuery) -> Result<T, NavError>,
    ) -> Result<T, NavError> {
        let shard = &self.queries[shard_of(std::thread::current().id())];
        let pooled = shard.lock().ok().and_then(|mut q| q.pop());
        let mut query = match pooled {
            Some(q) => q,
            None => DtQuery::new(self.mesh.clone(), MAX_NODES)?,
        };
        let out = f(&mut query);
        if let Ok(mut pool) = shard.lock() {
            pool.push(query);
        }
        out
    }
}

/// The pool shard of thread `id` (which shard never affects results).
fn shard_of(id: std::thread::ThreadId) -> usize {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    (h.finish() as usize) % SHARDS
}

/// A raw reference as a `PolyRef` (zero is no polygon).
fn poly_ref(raw: u64) -> Result<PolyRef, NavError> {
    std::num::NonZeroU64::new(raw)
        .map(PolyRef)
        .ok_or(NavError::Recast(recast_sys::RecastError::NotFound))
}

/// Each polygon's island (numbered by first polygon) and the island count.
fn islands(
    mesh: &DtNavMesh,
    polys: &[recast_sys::Poly],
    index: &HashMap<PolyRef, usize>,
) -> (Vec<u32>, usize) {
    let mut parent: Vec<usize> = (0..polys.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (i, p) in polys.iter().enumerate() {
        for link in mesh.links(p.reference).unwrap_or_default() {
            if let Some(&j) = index.get(&link) {
                let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                parent[a.max(b)] = a.min(b);
            }
        }
    }
    let mut label: HashMap<usize, u32> = HashMap::new();
    let ids = (0..polys.len())
        .map(|i| {
            let r = root(&mut parent, i);
            let next = label.len() as u32;
            *label.entry(r).or_insert(next)
        })
        .collect();
    (ids, label.len())
}
