//! The `mesh` provider: primitives, sweeps, heightfields and signed-distance shapes, as GLB.

mod build;
mod canyon_tile;
mod ground;
pub(crate) mod noise;
mod rock;
mod sdf;
mod sweep;

use concordia_assets::MeshData;
use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::error::GenError;

/// A mesh to build (meters, +Y up, centred on the origin unless the shape says otherwise).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum MeshRequest {
    /// A box of `size`.
    Box {
        /// Width, height, depth.
        size: [f32; 3],
    },
    /// A UV sphere.
    Sphere {
        /// Radius.
        radius: f32,
        /// Steps around (≥ 3).
        segments: u32,
        /// Steps pole to pole (≥ 2).
        rings: u32,
    },
    /// A cylinder capped by hemispheres, along Y.
    Capsule {
        /// Radius.
        radius: f32,
        /// The cylinder part's height (the total is height + 2 radius).
        height: f32,
        /// Steps around (≥ 3).
        segments: u32,
        /// Steps per hemisphere (≥ 1).
        rings: u32,
    },
    /// A capped cylinder along Y.
    Cylinder {
        /// Radius.
        radius: f32,
        /// Height.
        height: f32,
        /// Steps around (≥ 3).
        segments: u32,
    },
    /// A torus around Y.
    Torus {
        /// Centre to tube centre.
        major: f32,
        /// Tube radius (< major).
        minor: f32,
        /// Steps around Y (≥ 3).
        segments: u32,
        /// Steps around the tube (≥ 3).
        sides: u32,
    },
    /// A closed counter-clockwise polygon in XY, extruded along +Z with caps.
    Extrude {
        /// The polygon (≥ 3 points, simple).
        profile: Vec<[f32; 2]>,
        /// Depth.
        depth: f32,
    },
    /// A profile of (radius, y) points revolved around Y; ends on the axis close the solid.
    Lathe {
        /// The profile (≥ 3 points, radius ≥ 0), bottom to top along the outside.
        profile: Vec<[f32; 2]>,
        /// Steps around (≥ 3).
        segments: u32,
    },
    /// A seeded rolling ground on XZ.
    Heightfield {
        /// Width and depth.
        size: [f32; 2],
        /// Cells along X and Z (≥ 1).
        cells: [u32; 2],
        /// Height range (± this).
        amplitude: f32,
        /// Bumps per meter.
        frequency: f32,
    },
    /// Shapes joined by a (smooth) union, meshed by surface nets.
    Sdf {
        /// The shapes (≥ 1).
        shapes: Vec<SdfShape>,
        /// Blend distance between shapes (0: a sharp union).
        smooth: f32,
        /// Grid cell size (at most a tenth of the shapes' extent).
        cell: f32,
    },
    /// A seeded rock (M4 4.4c): lumps smooth-unioned, displaced by 3D noise, with ledges and a
    /// flat base, meshed by surface nets.
    Rock {
        /// Width, height, depth of its lumps (the noise may reach a little past them).
        size: [f32; 3],
        /// Grid cells across the longest side (8–250).
        detail: u32,
        /// Noise depth as a fraction of the smallest half size (0–0.5).
        roughness: f32,
        /// Ledge spacing in metres (0: none).
        strata: f32,
        /// Lumps (1–8).
        lumps: u32,
    },
    /// One strip of the canyon (M4 4.4c, `crate::canyon`), seeded by the canyon's seed.
    CanyonTile {
        /// Which strip, from the canyon's mouth (z = 0) down −Z.
        tile: u32,
        /// Strips the canyon is cut into.
        of: u32,
        /// Cells across (X) and along (Z), each 1–4096.
        cells: [u32; 2],
    },
}

/// A signed-distance shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub enum SdfShape {
    /// A sphere.
    Sphere {
        /// Centre.
        center: [f32; 3],
        /// Radius.
        radius: f32,
    },
    /// A box.
    Box {
        /// Centre.
        center: [f32; 3],
        /// Half extents.
        half: [f32; 3],
    },
}

/// Builds the mesh for `request` (`seed` drives seeded shapes).
pub fn build(request: &MeshRequest, seed: u64) -> Result<MeshData, GenError> {
    match request {
        MeshRequest::Box { size } => {
            size.iter().try_for_each(|s| sweep::positive("size", *s))?;
            Ok(concordia_assets::shapes::cube(*size))
        }
        MeshRequest::Sphere {
            radius,
            segments,
            rings,
        } => {
            sweep::positive("radius", *radius)?;
            sweep::segments_ok("segments", *segments, 3)?;
            sweep::segments_ok("rings", *rings, 2)?;
            Ok(concordia_assets::shapes::sphere(*radius, *segments, *rings))
        }
        MeshRequest::Capsule {
            radius,
            height,
            segments,
            rings,
        } => sweep::capsule(*radius, *height, *segments, *rings),
        MeshRequest::Cylinder {
            radius,
            height,
            segments,
        } => sweep::cylinder(*radius, *height, *segments),
        MeshRequest::Torus {
            major,
            minor,
            segments,
            sides,
        } => sweep::torus(*major, *minor, *segments, *sides),
        MeshRequest::Extrude { profile, depth } => sweep::extrude(profile, *depth),
        MeshRequest::Lathe { profile, segments } => sweep::lathe(profile, *segments),
        MeshRequest::Heightfield {
            size,
            cells,
            amplitude,
            frequency,
        } => ground::heightfield(*size, *cells, *amplitude, *frequency, seed),
        MeshRequest::Sdf {
            shapes,
            smooth,
            cell,
        } => sdf::sdf(shapes, *smooth, *cell),
        MeshRequest::Rock {
            size,
            detail,
            roughness,
            strata,
            lumps,
        } => rock::rock(
            &rock::RockShape {
                size: *size,
                detail: *detail,
                roughness: *roughness,
                strata: *strata,
                lumps: *lumps,
            },
            seed,
        ),
        MeshRequest::CanyonTile { tile, of, cells } => {
            canyon_tile::tile(&crate::canyon::Canyon { seed }, *tile, *of, *cells)
        }
    }
}

/// The `mesh` provider: a [`MeshRequest`] as a `.glb`.
#[derive(Debug, Clone, Copy, Default)]
pub struct MeshProvider;

impl crate::provider::ContentProvider for MeshProvider {
    fn id(&self) -> &str {
        "mesh"
    }

    fn kind(&self) -> crate::provider::GenKind {
        crate::provider::GenKind::mesh
    }

    fn version(&self) -> u32 {
        1
    }

    fn extensions(&self) -> Vec<&str> {
        vec!["glb"]
    }

    fn request_schema(&self) -> serde_json::Value {
        concordia_reflect::schema_for(<MeshRequest as Reflect>::type_info())
    }

    fn generate(
        &self,
        request: &serde_json::Value,
        seed: u64,
        _: &crate::provider::GenContext<'_>,
    ) -> Result<crate::provider::Generated, GenError> {
        let request: MeshRequest = serde_json::from_value(request.clone())
            .map_err(|e| GenError::invalid("request", e.to_string()))?;
        let mesh = build(&request, seed)?;
        let bytes = crate::glb::write_glb(&mesh)?;
        Ok(crate::provider::Generated {
            bytes,
            info: serde_json::json!({
                "vertices": mesh.positions.len(),
                "triangles": mesh.indices.len() / 3,
                "bounds": {"min": mesh.aabb.0, "max": mesh.aabb.1},
            }),
            settings: import_settings(&request),
            inputs: Vec::new(),
        })
    }
}

/// Import settings a request's mesh needs: rocks and canyon strips always take the meshlet path
/// (M4 4.4c), whatever the `auto` rule becomes.
fn import_settings(request: &MeshRequest) -> Option<serde_json::Value> {
    matches!(
        request,
        MeshRequest::Rock { .. } | MeshRequest::CanyonTile { .. }
    )
    .then(|| serde_json::json!({ "meshlets": true }))
}
