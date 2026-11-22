//! The tiled, parallel build: each tile is built from the triangles and marks that overlap it
//! (plus Recast's border), independently, and tiles are kept in (z, x) order.

use std::num::NonZeroUsize;

use concordia_core::math::Vec3;
use recast_sys::{BoxMark, TileInput, build_tile};

use crate::data::NavMeshData;
use crate::error::NavError;
use crate::gather::NavInput;

/// Builds every tile of `input` on `threads` threads; the result is identical for any count.
pub fn build_tiles(input: &NavInput, threads: NonZeroUsize) -> Result<NavMeshData, NavError> {
    let grid = Grid::of(input);
    let buckets = grid.bucket(input);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.get())
        .build()
        .map_err(|e| NavError::Data(format!("no build threads: {e}")))?;
    let built: Vec<Option<BuiltTile>> = pool.install(|| {
        use rayon::prelude::*;
        grid.coords()
            .par_iter()
            .zip(buckets.par_iter())
            .map(|(&tile, tris)| grid.build(input, tile, tris))
            .collect::<Result<_, NavError>>()
    })?;
    let search = [2.0, input.settings.config.agent_height.max(2.0), 2.0];
    Ok(NavMeshData {
        origin: input.min.to_array(),
        tile_width: grid.width,
        grid: [grid.nx, grid.nz],
        search,
        costs: input.costs.clone(),
        tiles: built.into_iter().flatten().collect(),
    })
}

/// A built tile: its grid position and Detour data.
type BuiltTile = ([i32; 2], Vec<u8>);

/// The tile grid over a volume.
struct Grid {
    min: Vec3,
    max: Vec3,
    width: f32,
    border: f32,
    nx: u32,
    nz: u32,
}

impl Grid {
    fn of(input: &NavInput) -> Self {
        let c = &input.settings.config;
        let width = input.settings.tile_width();
        let size = input.max - input.min;
        let count = |extent: f32| ((extent / width).ceil() as u32).max(1);
        let border = ((c.agent_radius / c.cell_size).ceil() + 3.0) * c.cell_size;
        Self {
            min: input.min,
            max: input.max,
            width,
            border,
            nx: count(size.x),
            nz: count(size.z),
        }
    }

    /// Tiles in (z, x) order.
    fn coords(&self) -> Vec<[i32; 2]> {
        (0..self.nz)
            .flat_map(|z| (0..self.nx).map(move |x| [x as i32, z as i32]))
            .collect()
    }

    /// The range of tile indices along an axis that `lo..hi` (plus the border) touches.
    fn span(&self, lo: f32, hi: f32, origin: f32, n: u32) -> std::ops::RangeInclusive<u32> {
        let index = |v: f32| (((v - origin) / self.width).floor().max(0.0) as u32).min(n - 1);
        index(lo - self.border)..=index(hi + self.border)
    }

    /// Each tile's triangle indices, in input order.
    fn bucket(&self, input: &NavInput) -> Vec<Vec<[u32; 3]>> {
        let mut buckets = vec![Vec::new(); (self.nx * self.nz) as usize];
        for tri in &input.triangles {
            let p = tri.map(|i| Vec3::from_array(input.vertices[i as usize]));
            let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
            for z in self.span(lo.z, hi.z, self.min.z, self.nz) {
                for x in self.span(lo.x, hi.x, self.min.x, self.nx) {
                    buckets[(z * self.nx + x) as usize].push(*tri);
                }
            }
        }
        buckets
    }

    fn bounds(&self, [x, z]: [i32; 2]) -> (Vec3, Vec3) {
        let lo = Vec3::new(
            self.min.x + x as f32 * self.width,
            self.min.y,
            self.min.z + z as f32 * self.width,
        );
        (
            lo,
            Vec3::new(lo.x + self.width, self.max.y, lo.z + self.width),
        )
    }

    fn build(
        &self,
        input: &NavInput,
        tile: [i32; 2],
        triangles: &[[u32; 3]],
    ) -> Result<Option<BuiltTile>, NavError> {
        let (lo, hi) = self.bounds(tile);
        let reach = Vec3::new(self.border, 0.0, self.border);
        let marks: Vec<BoxMark> = input
            .marks
            .iter()
            .filter(|m| {
                Vec3::from_array(m.min).cmple(hi + reach).all()
                    && Vec3::from_array(m.max).cmpge(lo - reach).all()
            })
            .copied()
            .collect();
        // Each gathered triangle owns its three vertices, so a tile copies just its own.
        let vertices: Vec<[f32; 3]> = triangles
            .iter()
            .flat_map(|t| t.map(|i| input.vertices[i as usize]))
            .collect();
        let local: Vec<[u32; 3]> = (0..triangles.len() as u32)
            .map(|i| [3 * i, 3 * i + 1, 3 * i + 2])
            .collect();
        let tile_input = TileInput {
            vertices: &vertices,
            triangles: &local,
            marks: &marks,
            tile,
            min: lo.to_array(),
            max: hi.to_array(),
        };
        let data = build_tile(&input.settings.config, &tile_input)?;
        Ok(data.map(|d| (tile, d.bytes().to_vec())))
    }
}
