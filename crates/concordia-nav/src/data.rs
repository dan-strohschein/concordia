//! A navmesh's serialized form: the grid, the area costs and every tile's Detour data, little-endian.

use crate::error::NavError;

const MAGIC: &[u8; 4] = b"CNAV";
const VERSION: u32 = 1;

/// A built navmesh as bytes-to-be: canonical, so equal inputs give equal bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct NavMeshData {
    /// World position of tile (0, 0)'s lower corner.
    pub origin: [f32; 3],
    /// Tile side in meters.
    pub tile_width: f32,
    /// Tiles along x and z.
    pub grid: [u32; 2],
    /// Search reach for snapping points (half extents).
    pub search: [f32; 3],
    /// Travel cost of each cost area id.
    pub costs: Vec<(u8, f32)>,
    /// Built tiles in (z, x) order: grid position and Detour data.
    pub tiles: Vec<([i32; 2], Vec<u8>)>,
}

impl NavMeshData {
    /// The canonical bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend(VERSION.to_le_bytes());
        for f in self.origin.iter().chain([&self.tile_width]) {
            out.extend(f.to_le_bytes());
        }
        out.extend(self.grid.iter().flat_map(|n| n.to_le_bytes()));
        out.extend(self.search.iter().flat_map(|f| f.to_le_bytes()));
        out.extend((self.costs.len() as u32).to_le_bytes());
        for (id, cost) in &self.costs {
            out.push(*id);
            out.extend(cost.to_le_bytes());
        }
        out.extend((self.tiles.len() as u32).to_le_bytes());
        for ([x, z], bytes) in &self.tiles {
            out.extend(x.to_le_bytes());
            out.extend(z.to_le_bytes());
            out.extend((bytes.len() as u32).to_le_bytes());
            out.extend(bytes);
        }
        out
    }

    /// Reads [`to_bytes`](Self::to_bytes) output.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NavError> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(4)? != MAGIC {
            return Err(NavError::Data("not a Concordia navmesh (bad magic)".into()));
        }
        let version = r.u32()?;
        if version != VERSION {
            return Err(NavError::Data(format!(
                "format version {version}, expected {VERSION}"
            )));
        }
        let origin = [r.f32()?, r.f32()?, r.f32()?];
        let tile_width = r.f32()?;
        let grid = [r.u32()?, r.u32()?];
        let search = [r.f32()?, r.f32()?, r.f32()?];
        let costs = (0..r.count(5)?)
            .map(|_| Ok((r.take(1)?[0], r.f32()?)))
            .collect::<Result<_, NavError>>()?;
        let tiles = (0..r.count(12)?)
            .map(|_| {
                let (x, z) = (r.u32()? as i32, r.u32()? as i32);
                let n = r.u32()? as usize;
                Ok(([x, z], r.take(n)?.to_vec()))
            })
            .collect::<Result<_, NavError>>()?;
        if r.at != bytes.len() {
            return Err(NavError::Data(format!(
                "{} trailing bytes",
                bytes.len() - r.at
            )));
        }
        Ok(Self {
            origin,
            tile_width,
            grid,
            search,
            costs,
            tiles,
        })
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], NavError> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.ok_or_else(|| NavError::Data(format!("truncated at byte {}", self.at)))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, NavError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32, NavError> {
        self.u32().map(f32::from_bits)
    }

    /// A count whose items take at least `min_size` bytes each (so a bad count can't allocate).
    fn count(&mut self, min_size: usize) -> Result<usize, NavError> {
        let n = self.u32()? as usize;
        match n.saturating_mul(min_size) <= self.bytes.len() - self.at {
            true => Ok(n),
            false => Err(NavError::Data(format!("count {n} exceeds the data"))),
        }
    }
}
