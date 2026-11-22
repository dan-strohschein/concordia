//! Building one navmesh tile from triangles and area marks.

use crate::error::RecastError;
use crate::ffi;

/// Recast build settings, in world units unless noted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildConfig {
    /// Voxel size on the ground plane.
    pub cell_size: f32,
    /// Voxel height.
    pub cell_height: f32,
    /// Agent height (clearance).
    pub agent_height: f32,
    /// Agent radius (walls are eroded by it).
    pub agent_radius: f32,
    /// Tallest step an agent climbs.
    pub agent_max_climb: f32,
    /// Steepest walkable slope in degrees.
    pub agent_max_slope_deg: f32,
    /// Cells per tile side.
    pub tile_size: u32,
    /// Cells per side of the smallest region kept.
    pub region_min_size: u32,
    /// Cells per side under which regions are merged.
    pub region_merge_size: u32,
    /// Longest polygon edge along walls.
    pub edge_max_len: f32,
    /// How far simplified contours may stray from the voxels.
    pub edge_max_error: f32,
    /// Vertices per polygon (3..=6).
    pub verts_per_poly: u32,
    /// Detail mesh sample spacing (0 disables height samples).
    pub detail_sample_dist: f32,
    /// Detail mesh height error.
    pub detail_sample_max_error: f32,
}

impl BuildConfig {
    fn to_ffi(self) -> Result<ffi::cr_config, RecastError> {
        let int =
            |v: u32| i32::try_from(v).map_err(|_| RecastError::Input(format!("{v} is too large")));
        Ok(ffi::cr_config {
            cell_size: self.cell_size,
            cell_height: self.cell_height,
            agent_height: self.agent_height,
            agent_radius: self.agent_radius,
            agent_max_climb: self.agent_max_climb,
            agent_max_slope_deg: self.agent_max_slope_deg,
            tile_size: int(self.tile_size)?,
            region_min_size: int(self.region_min_size)?,
            region_merge_size: int(self.region_merge_size)?,
            edge_max_len: self.edge_max_len,
            edge_max_error: self.edge_max_error,
            verts_per_poly: int(self.verts_per_poly)?,
            detail_sample_dist: self.detail_sample_dist,
            detail_sample_max_error: self.detail_sample_max_error,
        })
    }
}

/// A Recast area id: blocked, walkable (cost 1) or one of the cost areas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Area(u8);

impl Area {
    /// Not walkable (removed after marking).
    pub const BLOCKED: Self = Self(ffi::CR_AREA_BLOCKED);
    /// Plain walkable ground.
    pub const WALKABLE: Self = Self(ffi::CR_AREA_WALKABLE);
    /// The first cost area's id.
    pub const FIRST_COST: u8 = 2;
    /// The last cost area's id.
    pub const LAST_COST: u8 = 62;

    /// Cost area `id` (2..=62).
    pub fn cost(id: u8) -> Option<Self> {
        (Self::FIRST_COST..=Self::LAST_COST)
            .contains(&id)
            .then_some(Self(id))
    }

    /// The raw id.
    pub fn id(self) -> u8 {
        self.0
    }
}

/// An axis-aligned box whose walkable cells take `area`; marks apply in order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxMark {
    /// Lower corner.
    pub min: [f32; 3],
    /// Upper corner.
    pub max: [f32; 3],
    /// The area it gives.
    pub area: Area,
}

/// One tile's input: the triangles overlapping it (counter-clockwise seen from above is walkable
/// up) and the marks.
#[derive(Debug, Clone, Copy)]
pub struct TileInput<'a> {
    /// Vertex positions.
    pub vertices: &'a [[f32; 3]],
    /// Vertex indices.
    pub triangles: &'a [[u32; 3]],
    /// Area marks, applied in order.
    pub marks: &'a [BoxMark],
    /// Grid position.
    pub tile: [i32; 2],
    /// Tile bounds (without the border).
    pub min: [f32; 3],
    /// Tile bounds (without the border).
    pub max: [f32; 3],
}

impl TileInput<'_> {
    fn validate(&self) -> Result<(), RecastError> {
        let finite = |v: &[f32; 3]| v.iter().all(|c| c.is_finite());
        if !self.vertices.iter().all(finite) {
            return Err(RecastError::Input("a vertex is not finite".into()));
        }
        let n = self.vertices.len();
        if let Some(t) = self
            .triangles
            .iter()
            .find(|t| t.iter().any(|&i| i as usize >= n))
        {
            return Err(RecastError::Input(format!(
                "triangle {t:?} indexes past {n} vertices"
            )));
        }
        if !(finite(&self.min) && finite(&self.max)) || (0..3).any(|i| self.min[i] >= self.max[i]) {
            return Err(RecastError::Input(
                "tile bounds are empty or not finite".into(),
            ));
        }
        if !self.marks.iter().all(|m| finite(&m.min) && finite(&m.max)) {
            return Err(RecastError::Input("a mark's bounds are not finite".into()));
        }
        Ok(())
    }
}

/// Detour tile data, as built (or as read back from a cache).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileData(Vec<u8>);

impl TileData {
    /// Wraps bytes from an earlier build; Detour checks them when the tile is added.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Builds one tile; `None` when it has no walkable polygons.
pub fn build_tile(
    config: &BuildConfig,
    input: &TileInput<'_>,
) -> Result<Option<TileData>, RecastError> {
    input.validate()?;
    let count = |n: usize| {
        i32::try_from(n).map_err(|_| RecastError::Input(format!("{n} items is too many")))
    };
    let marks: Vec<ffi::cr_box_mark> = input
        .marks
        .iter()
        .map(|m| ffi::cr_box_mark {
            min: m.min,
            max: m.max,
            area: m.area.0,
            pad: [0; 3],
        })
        .collect();
    let raw = ffi::cr_tile_input {
        verts: input.vertices.as_ptr().cast(),
        tris: input.triangles.as_ptr().cast(),
        marks: marks.as_ptr(),
        vert_count: count(input.vertices.len())?,
        tri_count: count(input.triangles.len())?,
        mark_count: count(marks.len())?,
        tile_x: input.tile[0],
        tile_y: input.tile[1],
        bmin: input.min,
        bmax: input.max,
    };
    let config = config.to_ffi()?;
    let (mut data, mut size) = (std::ptr::null_mut::<u8>(), 0i32);
    // SAFETY: every pointer is valid for the counts given ([f32; 3] and [u32; 3] are laid out as
    // 3 floats and 3 32-bit integers) and lives across the call; the shim writes the outputs.
    let code = unsafe { ffi::cr_build_tile(&config, &raw, &mut data, &mut size) };
    match code {
        ffi::CR_EMPTY => Ok(None),
        ffi::CR_OK if !data.is_null() && size > 0 => {
            // SAFETY: the shim returned `size` bytes at `data`, which we copy and then free once.
            let bytes = unsafe { std::slice::from_raw_parts(data, size as usize) }.to_vec();
            // SAFETY: `data` came from the shim and is freed exactly once.
            unsafe { ffi::cr_free(data.cast()) };
            Ok(Some(TileData(bytes)))
        }
        ffi::CR_OK => Err(RecastError::Query),
        code => Err(RecastError::from_code(code)),
    }
}
