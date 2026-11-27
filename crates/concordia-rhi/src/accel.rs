//! Acceleration structures for ray queries (M4 4.8c): triangles (a bottom level) and instances of
//! them (a top level), built on the device and read by shaders through the `raytracing` module's
//! bindless table.

use crate::error::RhiError;
use crate::handle::{AccelerationStructure, Buffer};

/// The most acceleration structures a device holds at once (`raytracing.slang`'s table).
pub const ACCEL_CAPACITY: u32 = 256;

/// Triangles from a vertex buffer (`float3` positions) and an optional `u32` index buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriangleGeometry {
    /// Positions (created with `BufferUsage::ACCEL_INPUT`).
    pub vertices: Buffer,
    /// Byte offset of the first position.
    pub vertex_offset: u64,
    /// Bytes between positions (at least 12).
    pub vertex_stride: u64,
    /// Positions in the buffer.
    pub vertex_count: u32,
    /// Corner indices (`u32`) and their byte offset (`None`: consecutive vertices make triangles).
    pub indices: Option<(Buffer, u64)>,
    /// Triangles.
    pub triangle_count: u32,
}

/// One placement of a bottom-level structure in a top level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccelInstance {
    /// The triangles placed.
    pub geometry: AccelerationStructure,
    /// Object → world, three rows of a 3 × 4 matrix.
    pub transform: [[f32; 4]; 3],
    /// What shaders see as the instance id (24 bits).
    pub id: u32,
    /// Ray mask bits it answers to.
    pub mask: u8,
}

/// What to build.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccelDesc<'a> {
    /// A bottom level over triangles.
    Triangles(&'a [TriangleGeometry]),
    /// A top level over instances of bottom levels.
    Instances(&'a [AccelInstance]),
}

/// Checks what the backends share: counts, strides, ids and finite transforms.
pub fn validate(desc: &AccelDesc<'_>) -> Result<(), RhiError> {
    let bad = |reason: String| {
        Err(RhiError::Invalid {
            what: "acceleration structure desc",
            reason,
        })
    };
    match desc {
        AccelDesc::Triangles(geometry) => {
            if geometry.is_empty() {
                return bad("no geometry".into());
            }
            for (k, g) in geometry.iter().enumerate() {
                if g.triangle_count == 0 || g.vertex_count == 0 {
                    return bad(format!("geometry {k}: no triangles or vertices"));
                }
                if g.vertex_stride < 12 {
                    return bad(format!(
                        "geometry {k}: vertex stride {} under 12",
                        g.vertex_stride
                    ));
                }
                if g.indices.is_none()
                    && u64::from(g.triangle_count) * 3 > u64::from(g.vertex_count)
                {
                    return bad(format!(
                        "geometry {k}: {} triangles need {} vertices, {} given",
                        g.triangle_count,
                        u64::from(g.triangle_count) * 3,
                        g.vertex_count
                    ));
                }
            }
        }
        AccelDesc::Instances(instances) => {
            for (k, i) in instances.iter().enumerate() {
                if i.transform.iter().flatten().any(|v| !v.is_finite()) {
                    return bad(format!("instance {k}: the transform is not finite"));
                }
                if i.id >= 1 << 24 {
                    return bad(format!("instance {k}: id {} is over 24 bits", i.id));
                }
            }
        }
    }
    Ok(())
}
