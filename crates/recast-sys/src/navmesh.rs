//! A Detour tiled navmesh: tiles added by grid position, then read-only.

use std::num::NonZeroU64;
use std::ptr::NonNull;

use crate::error::RecastError;
use crate::ffi;
use crate::tile::{Area, TileData};

/// A navmesh polygon reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolyRef(pub NonZeroU64);

impl PolyRef {
    fn new(raw: u64) -> Option<Self> {
        NonZeroU64::new(raw).map(Self)
    }

    fn raw(self) -> u64 {
        self.0.get()
    }
}

/// The tile grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavMeshParams {
    /// World position of tile (0, 0)'s lower corner.
    pub origin: [f32; 3],
    /// Tile width along x.
    pub tile_width: f32,
    /// Tile depth along z.
    pub tile_depth: f32,
    /// Most tiles.
    pub max_tiles: u32,
    /// Most polygons in one tile.
    pub max_polys_per_tile: u32,
}

/// A filled tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileInfo {
    /// Its slot (for [`DtNavMesh::poly`]).
    pub slot: u32,
    /// Grid position.
    pub tile: [i32; 2],
    /// Ground polygons.
    pub polys: u32,
}

/// One ground polygon.
#[derive(Debug, Clone, PartialEq)]
pub struct Poly {
    /// Its reference.
    pub reference: PolyRef,
    /// Its vertices, counter-clockwise seen from above.
    pub vertices: Vec<[f32; 3]>,
    /// Its area.
    pub area: Area,
}

/// A Detour navmesh (owned; freed on drop).
#[derive(Debug)]
pub struct DtNavMesh {
    ptr: NonNull<ffi::cr_navmesh>,
}

// SAFETY: the navmesh is only mutated through `&mut self` (adding tiles); Detour's read-only
// accessors and queries may run on any thread at once.
unsafe impl Send for DtNavMesh {}
// SAFETY: as above; shared access never mutates.
unsafe impl Sync for DtNavMesh {}

impl DtNavMesh {
    /// An empty navmesh over `params`' tile grid.
    pub fn new(params: &NavMeshParams) -> Result<Self, RecastError> {
        let int =
            |v: u32| i32::try_from(v).map_err(|_| RecastError::Input(format!("{v} is too large")));
        let (tiles, polys) = (int(params.max_tiles)?, int(params.max_polys_per_tile)?);
        // SAFETY: `origin` is 3 floats; the shim validates the rest.
        let ptr = unsafe {
            ffi::cr_navmesh_create(
                params.origin.as_ptr(),
                params.tile_width,
                params.tile_depth,
                tiles,
                polys,
            )
        };
        NonNull::new(ptr)
            .map(|ptr| Self { ptr })
            .ok_or_else(|| RecastError::Input(format!("navmesh parameters {params:?} are invalid")))
    }

    /// Adds a built tile at its grid position (the data is copied).
    pub fn add_tile(&mut self, tile: &TileData) -> Result<(), RecastError> {
        let bytes = tile.bytes();
        let size = i32::try_from(bytes.len())
            .map_err(|_| RecastError::Input("tile data too large".into()))?;
        // SAFETY: `bytes` is valid for `size` bytes; the shim copies it.
        match unsafe { ffi::cr_navmesh_add_tile(self.ptr.as_ptr(), bytes.as_ptr(), size) } {
            ffi::CR_OK => Ok(()),
            code => Err(RecastError::from_code(code)),
        }
    }

    pub(crate) fn as_ptr(&self) -> *const ffi::cr_navmesh {
        self.ptr.as_ptr()
    }

    /// Every filled tile, in slot order.
    pub fn tiles(&self) -> Vec<TileInfo> {
        // SAFETY: read-only access to a live navmesh.
        let slots = unsafe { ffi::cr_navmesh_tile_slots(self.as_ptr()) }.max(0);
        (0..slots)
            .filter_map(|slot| {
                let (mut x, mut y, mut polys) = (0, 0, 0);
                // SAFETY: as above; the outputs are valid integers.
                let code = unsafe {
                    ffi::cr_navmesh_tile_info(self.as_ptr(), slot, &mut x, &mut y, &mut polys)
                };
                (code == ffi::CR_OK).then(|| TileInfo {
                    slot: slot as u32,
                    tile: [x, y],
                    polys: polys.max(0) as u32,
                })
            })
            .collect()
    }

    /// Ground polygon `index` of the tile in `slot`.
    pub fn poly(&self, slot: u32, index: u32) -> Result<Poly, RecastError> {
        let (slot, index) = (slot as i32, index as i32);
        let mut out = ffi::cr_poly {
            r#ref: 0,
            verts: [0.0; 18],
            vert_count: 0,
            area: 0,
            pad: [0; 6],
        };
        // SAFETY: read-only access; `out` is a valid struct.
        let code = unsafe { ffi::cr_navmesh_poly(self.as_ptr(), slot, index, &mut out) };
        if code != ffi::CR_OK {
            return Err(RecastError::from_code(code));
        }
        let reference = PolyRef::new(out.r#ref).ok_or(RecastError::NotFound)?;
        let n = usize::from(out.vert_count).min(ffi::CR_MAX_VERTS_PER_POLY);
        let vertices = (0..n)
            .map(|i| [out.verts[i * 3], out.verts[i * 3 + 1], out.verts[i * 3 + 2]])
            .collect();
        let area = Area::cost(out.area).unwrap_or(if out.area == ffi::CR_AREA_WALKABLE {
            Area::WALKABLE
        } else {
            Area::BLOCKED
        });
        Ok(Poly {
            reference,
            vertices,
            area,
        })
    }

    /// Every ground polygon, tile by tile.
    pub fn polys(&self) -> Vec<Poly> {
        self.tiles()
            .iter()
            .flat_map(|t| (0..t.polys).filter_map(move |i| self.poly(t.slot, i).ok()))
            .collect()
    }

    /// The polygons linked to `poly` across its edges (within and between tiles).
    pub fn links(&self, poly: PolyRef) -> Result<Vec<PolyRef>, RecastError> {
        let mut out = [0u64; 64];
        // SAFETY: read-only access; `out` holds `cap` references.
        let n = unsafe {
            ffi::cr_navmesh_poly_links(
                self.as_ptr(),
                poly.raw(),
                out.as_mut_ptr(),
                out.len() as i32,
            )
        };
        if n < 0 {
            return Err(RecastError::from_code(n));
        }
        Ok(out[..(n as usize).min(out.len())]
            .iter()
            .filter_map(|&r| PolyRef::new(r))
            .collect())
    }
}

impl Drop for DtNavMesh {
    fn drop(&mut self) {
        // SAFETY: the navmesh is live and freed exactly once; queries hold an `Arc` to it.
        unsafe { ffi::cr_navmesh_free(self.ptr.as_ptr()) }
    }
}

pub(crate) fn raw(poly: PolyRef) -> u64 {
    poly.raw()
}

pub(crate) fn from_raw(raw: u64) -> Option<PolyRef> {
    PolyRef::new(raw)
}
