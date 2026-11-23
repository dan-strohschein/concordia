//! Detour queries: nearest point, path corridors, straight paths and surface raycasts.

use std::ptr::NonNull;
use std::sync::Arc;

use crate::error::RecastError;
use crate::ffi;
use crate::navmesh::{DtNavMesh, PolyRef, from_raw, raw};

/// Travel cost per area id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AreaCosts([f32; ffi::CR_MAX_AREAS]);

impl Default for AreaCosts {
    fn default() -> Self {
        Self([1.0; ffi::CR_MAX_AREAS])
    }
}

impl AreaCosts {
    /// Sets area `id`'s cost (finite, at least 1).
    pub fn set(&mut self, id: u8, cost: f32) -> Result<(), RecastError> {
        if usize::from(id) >= ffi::CR_MAX_AREAS || !cost.is_finite() || cost < 1.0 {
            return Err(RecastError::Input(format!(
                "area {id} cost {cost} (needs a finite cost ≥ 1)"
            )));
        }
        self.0[usize::from(id)] = cost;
        Ok(())
    }

    fn filter(&self) -> ffi::cr_filter {
        ffi::cr_filter { area_cost: self.0 }
    }
}

/// A polygon corridor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corridor {
    /// Polygons from start to the last one reached.
    pub polys: Vec<PolyRef>,
    /// The end polygon was not reached; the corridor ends at the closest polygon instead.
    pub partial: bool,
}

/// A surface raycast's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayResult {
    /// Fraction along the ray where a wall was hit, or `None` when the end was reached.
    pub hit: Option<f32>,
    /// The wall's normal (zero when nothing was hit).
    pub normal: [f32; 3],
}

/// Where a surface move ended, and the polygons it crossed.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceMove {
    /// The reached point (its height is the end's; read the surface with `poly_height`).
    pub position: [f32; 3],
    /// Polygons crossed, from the start's to the one holding `position`.
    pub visited: Vec<PolyRef>,
}

/// A surface raycast and the polygons it crossed.
#[derive(Debug, Clone, PartialEq)]
pub struct RayPath {
    /// Where it stopped.
    pub ray: RayResult,
    /// Polygons crossed, from the start's.
    pub polys: Vec<PolyRef>,
}

/// A Detour query object (its node pool is reused between calls).
#[derive(Debug)]
pub struct DtQuery {
    ptr: NonNull<ffi::cr_query>,
    _mesh: Arc<DtNavMesh>,
}

// SAFETY: the query owns its node pool and is used through `&mut self` only; the navmesh it reads
// is `Sync` and kept alive by the `Arc`.
unsafe impl Send for DtQuery {}

fn check(code: i32) -> Result<(), RecastError> {
    match code {
        ffi::CR_OK => Ok(()),
        code => Err(RecastError::from_code(code)),
    }
}

fn finite(points: &[[f32; 3]]) -> Result<(), RecastError> {
    match points.iter().flatten().all(|c| c.is_finite()) {
        true => Ok(()),
        false => Err(RecastError::Input("a point is not finite".into())),
    }
}

impl DtQuery {
    /// A query over `mesh` with a pool of `max_nodes` search nodes (1..=65535).
    pub fn new(mesh: Arc<DtNavMesh>, max_nodes: u16) -> Result<Self, RecastError> {
        // SAFETY: the navmesh is live for the query's lifetime (the Arc); the shim validates the count.
        let ptr = unsafe { ffi::cr_query_create(mesh.as_ptr(), i32::from(max_nodes)) };
        NonNull::new(ptr)
            .map(|ptr| Self { ptr, _mesh: mesh })
            .ok_or_else(|| RecastError::Input(format!("{max_nodes} search nodes")))
    }

    /// The nearest polygon and point within `center ± half_extents`, if any.
    pub fn nearest(
        &mut self,
        costs: &AreaCosts,
        center: [f32; 3],
        half_extents: [f32; 3],
    ) -> Result<Option<(PolyRef, [f32; 3])>, RecastError> {
        finite(&[center, half_extents])?;
        let (mut reference, mut point) = (0u64, [0f32; 3]);
        let filter = costs.filter();
        // SAFETY: every pointer is valid for the shim's reads and writes during the call.
        let code = unsafe {
            ffi::cr_find_nearest(
                self.ptr.as_ptr(),
                &filter,
                center.as_ptr(),
                half_extents.as_ptr(),
                &mut reference,
                point.as_mut_ptr(),
            )
        };
        match code {
            ffi::CR_ERR_NOT_FOUND => Ok(None),
            code => check(code).map(|()| from_raw(reference).map(|r| (r, point))),
        }
    }

    /// The corridor from `start` to `end` (each a polygon and a point on it), at most `max` polygons.
    pub fn find_path(
        &mut self,
        costs: &AreaCosts,
        start: (PolyRef, [f32; 3]),
        end: (PolyRef, [f32; 3]),
        max: usize,
    ) -> Result<Corridor, RecastError> {
        finite(&[start.1, end.1])?;
        let mut path = vec![0u64; max.clamp(1, 1 << 16)];
        let (mut count, mut partial) = (0i32, 0u8);
        let filter = costs.filter();
        // SAFETY: `path` holds `cap` references; the other pointers are valid during the call.
        let code = unsafe {
            ffi::cr_find_path(
                self.ptr.as_ptr(),
                &filter,
                raw(start.0),
                raw(end.0),
                start.1.as_ptr(),
                end.1.as_ptr(),
                path.as_mut_ptr(),
                path.len() as i32,
                &mut count,
                &mut partial,
            )
        };
        check(code)?;
        path.truncate((count.max(0) as usize).min(path.len()));
        Ok(Corridor {
            polys: path.into_iter().filter_map(from_raw).collect(),
            partial: partial != 0,
        })
    }

    /// Corner points along `corridor` from `start` to `end` (both included), at most `max`.
    pub fn straight_path(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        corridor: &[PolyRef],
        max: usize,
    ) -> Result<Vec<[f32; 3]>, RecastError> {
        finite(&[start, end])?;
        let refs: Vec<u64> = corridor.iter().map(|&p| raw(p)).collect();
        let cap = max.clamp(2, 1 << 16);
        let mut points = vec![0f32; cap * 3];
        let mut count = 0i32;
        // SAFETY: `refs` holds `path_count` references and `points` `cap` triples.
        let code = unsafe {
            ffi::cr_straight_path(
                self.ptr.as_ptr(),
                start.as_ptr(),
                end.as_ptr(),
                refs.as_ptr(),
                refs.len() as i32,
                points.as_mut_ptr(),
                cap as i32,
                &mut count,
            )
        };
        check(code)?;
        let n = (count.max(0) as usize).min(cap);
        Ok((0..n)
            .map(|i| [points[i * 3], points[i * 3 + 1], points[i * 3 + 2]])
            .collect())
    }

    /// Walks the surface from `start` (on `poly`) toward `end`.
    pub fn raycast(
        &mut self,
        costs: &AreaCosts,
        poly: PolyRef,
        start: [f32; 3],
        end: [f32; 3],
    ) -> Result<RayResult, RecastError> {
        finite(&[start, end])?;
        let (mut t, mut normal) = (0f32, [0f32; 3]);
        let filter = costs.filter();
        // SAFETY: valid pointers during the call.
        let code = unsafe {
            ffi::cr_raycast(
                self.ptr.as_ptr(),
                &filter,
                raw(poly),
                start.as_ptr(),
                end.as_ptr(),
                &mut t,
                normal.as_mut_ptr(),
            )
        };
        check(code)?;
        Ok(RayResult {
            hit: (t <= 1.0).then_some(t),
            normal,
        })
    }
}

impl DtQuery {
    /// Moves from `start` (on `poly`) toward `end` along the surface, sliding along walls,
    /// crossing at most `max_visited` polygons (1..=65536).
    pub fn move_along_surface(
        &mut self,
        costs: &AreaCosts,
        poly: PolyRef,
        start: [f32; 3],
        end: [f32; 3],
        max_visited: usize,
    ) -> Result<SurfaceMove, RecastError> {
        finite(&[start, end])?;
        let cap = slots(max_visited, "visited")?;
        let mut visited = vec![0u64; cap];
        let (mut position, mut count) = ([0f32; 3], 0i32);
        let filter = costs.filter();
        // SAFETY: `visited` holds `cap` references; the other pointers are valid during the call.
        let code = unsafe {
            ffi::cr_move_along_surface(
                self.ptr.as_ptr(),
                &filter,
                raw(poly),
                start.as_ptr(),
                end.as_ptr(),
                position.as_mut_ptr(),
                visited.as_mut_ptr(),
                cap as i32,
                &mut count,
            )
        };
        check(code)?;
        Ok(SurfaceMove {
            position,
            visited: refs(visited, count),
        })
    }

    /// The surface height of `poly` under `pos`, or `None` when `pos` is outside it.
    pub fn poly_height(
        &mut self,
        poly: PolyRef,
        pos: [f32; 3],
    ) -> Result<Option<f32>, RecastError> {
        finite(&[pos])?;
        let mut height = 0f32;
        // SAFETY: valid pointers during the call.
        let code =
            unsafe { ffi::cr_poly_height(self.ptr.as_ptr(), raw(poly), pos.as_ptr(), &mut height) };
        match code {
            ffi::CR_EMPTY => Ok(None),
            code => check(code).map(|()| Some(height)),
        }
    }

    /// `raycast`, also listing up to `max` polygons the ray crossed (1..=65536).
    pub fn raycast_path(
        &mut self,
        costs: &AreaCosts,
        poly: PolyRef,
        start: [f32; 3],
        end: [f32; 3],
        max: usize,
    ) -> Result<RayPath, RecastError> {
        finite(&[start, end])?;
        let cap = slots(max, "path")?;
        let mut path = vec![0u64; cap];
        let (mut t, mut normal, mut count) = (0f32, [0f32; 3], 0i32);
        let filter = costs.filter();
        // SAFETY: `path` holds `cap` references; the other pointers are valid during the call.
        let code = unsafe {
            ffi::cr_raycast_path(
                self.ptr.as_ptr(),
                &filter,
                raw(poly),
                start.as_ptr(),
                end.as_ptr(),
                &mut t,
                normal.as_mut_ptr(),
                path.as_mut_ptr(),
                cap as i32,
                &mut count,
            )
        };
        check(code)?;
        Ok(RayPath {
            ray: RayResult {
                hit: (t <= 1.0).then_some(t),
                normal,
            },
            polys: refs(path, count),
        })
    }

    /// `findLocalNeighbourhood`: up to `max` polygons (1..=65536) around `poly`, its own first,
    /// reachable within `radius` of `center` without overlapping each other.
    pub fn local_neighbourhood(
        &mut self,
        costs: &AreaCosts,
        poly: PolyRef,
        center: [f32; 3],
        radius: f32,
        max: usize,
    ) -> Result<Vec<PolyRef>, RecastError> {
        finite(&[center])?;
        if !(radius.is_finite() && radius >= 0.0) {
            return Err(RecastError::Input(format!(
                "radius must be finite and ≥ 0, got {radius}"
            )));
        }
        let cap = slots(max, "polygon")?;
        let mut found = vec![0u64; cap];
        let mut count = 0i32;
        let filter = costs.filter();
        // SAFETY: `found` holds `cap` references; the other pointers are valid during the call.
        let code = unsafe {
            ffi::cr_local_neighbourhood(
                self.ptr.as_ptr(),
                &filter,
                raw(poly),
                center.as_ptr(),
                radius,
                found.as_mut_ptr(),
                cap as i32,
                &mut count,
            )
        };
        check(code)?;
        Ok(refs(found, count))
    }

    /// `getPolyWallSegments`: up to `max` (1..=65536) of `poly`'s walls, the edges with no
    /// passable neighbour, as point pairs in the polygon's winding order.
    pub fn wall_segments(
        &mut self,
        costs: &AreaCosts,
        poly: PolyRef,
        max: usize,
    ) -> Result<Vec<[[f32; 3]; 2]>, RecastError> {
        let cap = slots(max, "segment")?;
        let mut flat = vec![0f32; cap * 6];
        let mut count = 0i32;
        let filter = costs.filter();
        // SAFETY: `flat` holds `cap` segments of six floats; the other pointers are valid.
        let code = unsafe {
            ffi::cr_poly_wall_segments(
                self.ptr.as_ptr(),
                &filter,
                raw(poly),
                flat.as_mut_ptr(),
                cap as i32,
                &mut count,
            )
        };
        check(code)?;
        let n = (count.max(0) as usize).min(cap);
        Ok(flat.as_chunks::<6>().0[..n]
            .iter()
            .map(|s| [[s[0], s[1], s[2]], [s[3], s[4], s[5]]])
            .collect())
    }
}

/// A buffer size in 1..=65536.
fn slots(n: usize, what: &str) -> Result<usize, RecastError> {
    match n {
        1..=65536 => Ok(n),
        _ => Err(RecastError::Input(format!(
            "{n} {what} slots (needs 1..=65536)"
        ))),
    }
}

/// The first `count` references of `buffer`.
fn refs(mut buffer: Vec<u64>, count: i32) -> Vec<PolyRef> {
    buffer.truncate((count.max(0) as usize).min(buffer.len()));
    buffer.into_iter().filter_map(from_raw).collect()
}

impl Drop for DtQuery {
    fn drop(&mut self) {
        // SAFETY: the query is live and freed exactly once, before its navmesh (the Arc).
        unsafe { ffi::cr_query_free(self.ptr.as_ptr()) }
    }
}
