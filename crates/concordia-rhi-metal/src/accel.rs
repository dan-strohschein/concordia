//! Acceleration structures on Metal (M4 4.8c): primitive structures over triangles and instance
//! structures over them (user-id instance descriptors), built synchronously, with a bindless table
//! of resource ids that `raytracing.slang`'s `g_accels` reads at buffer slot 4.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSArray;
use objc2_metal::{
    MTLAccelerationStructure, MTLAccelerationStructureCommandEncoder,
    MTLAccelerationStructureDescriptor, MTLAccelerationStructureGeometryDescriptor,
    MTLAccelerationStructureInstanceDescriptorType, MTLAccelerationStructureInstanceOptions,
    MTLAccelerationStructureTriangleGeometryDescriptor,
    MTLAccelerationStructureUserIDInstanceDescriptor, MTLAttributeFormat, MTLBuffer,
    MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLDevice, MTLIndexType,
    MTLInstanceAccelerationStructureDescriptor, MTLPackedFloat3, MTLPackedFloat4x3,
    MTLPrimitiveAccelerationStructureDescriptor, MTLResourceOptions,
};

use concordia_rhi::{AccelDesc, AccelInstance, AccelerationStructure, RhiError, TriangleGeometry};

use crate::bindless::resource_id;
use crate::device::Inner;

pub(crate) type MtlAccel = ProtocolObject<dyn MTLAccelerationStructure>;

/// A built structure: its object, bindless slot and (top levels) the bottom levels it holds.
#[derive(Debug)]
pub(crate) struct AccelEntry {
    pub(crate) raw: Retained<MtlAccel>,
    pub(crate) slot: u32,
    pub(crate) children: Vec<AccelerationStructure>,
}

/// The structure's table entry (its resource id).
pub(crate) fn accel_entry(accel: &MtlAccel) -> u64 {
    resource_id(accel.gpuResourceID())
}

/// Builds `desc` and stores it (validated first; instances must name live bottom levels).
pub(crate) fn build(
    inner: &mut Inner,
    desc: &AccelDesc<'_>,
    label: &str,
) -> Result<AccelerationStructure, RhiError> {
    concordia_rhi::validate_accel(desc)?;
    let (descriptor, children, _keep): (Retained<MTLAccelerationStructureDescriptor>, _, _) =
        match desc {
            AccelDesc::Triangles(geometry) => (triangles(inner, geometry)?, Vec::new(), None),
            AccelDesc::Instances(instances) => {
                let (d, children, buffer) = instances_desc(inner, instances)?;
                (d, children, Some(buffer))
            }
        };
    let raw = encode_build(inner, &descriptor, label)?;
    let slot = inner.accel_table.insert(accel_entry(&raw))?;
    let (index, generation) = inner.accels.insert(AccelEntry {
        raw,
        slot,
        children,
    });
    Ok(AccelerationStructure::from_raw(index, generation))
}

/// A primitive descriptor over `geometry` (buffers checked against their sizes).
fn triangles(
    inner: &Inner,
    geometry: &[TriangleGeometry],
) -> Result<Retained<MTLAccelerationStructureDescriptor>, RhiError> {
    let mut parts = Vec::with_capacity(geometry.len());
    for (k, g) in geometry.iter().enumerate() {
        let vertices = buffer_for(inner, g.vertices, "vertices")?;
        let needed = g.vertex_offset + g.vertex_stride * u64::from(g.vertex_count.max(1) - 1) + 12;
        if needed > vertices.length() as u64 {
            return Err(invalid(format!(
                "geometry {k}: {needed} bytes of vertices, the buffer holds {}",
                vertices.length()
            )));
        }
        let d = MTLAccelerationStructureTriangleGeometryDescriptor::descriptor();
        d.setVertexBuffer(Some(&vertices));
        // SAFETY: the offset and stride were checked against the buffer's length above.
        unsafe {
            d.setVertexBufferOffset(g.vertex_offset as usize);
            d.setVertexStride(g.vertex_stride as usize);
        }
        d.setVertexFormat(MTLAttributeFormat::Float3);
        d.setTriangleCount(g.triangle_count as usize);
        if let Some((indices, offset)) = g.indices {
            let ib = buffer_for(inner, indices, "indices")?;
            let needed = offset + u64::from(g.triangle_count) * 12;
            if needed > ib.length() as u64 {
                return Err(invalid(format!(
                    "geometry {k}: {needed} bytes of indices, the buffer holds {}",
                    ib.length()
                )));
            }
            d.setIndexBuffer(Some(&ib));
            // SAFETY: the offset was checked against the buffer's length.
            unsafe { d.setIndexBufferOffset(offset as usize) };
            d.setIndexType(MTLIndexType::UInt32);
        }
        d.setOpaque(true);
        parts.push(Retained::into_super(d));
    }
    let array: Retained<NSArray<MTLAccelerationStructureGeometryDescriptor>> =
        NSArray::from_retained_slice(&parts);
    let desc = MTLPrimitiveAccelerationStructureDescriptor::descriptor();
    desc.setGeometryDescriptors(Some(&array));
    Ok(Retained::into_super(desc))
}

/// An instance descriptor over `instances`, the bottom levels it holds and its instance buffer.
#[allow(clippy::type_complexity)]
fn instances_desc(
    inner: &Inner,
    instances: &[AccelInstance],
) -> Result<
    (
        Retained<MTLAccelerationStructureDescriptor>,
        Vec<AccelerationStructure>,
        Retained<ProtocolObject<dyn MTLBuffer>>,
    ),
    RhiError,
> {
    let mut children: Vec<AccelerationStructure> = Vec::new();
    let mut objects: Vec<Retained<MtlAccel>> = Vec::new();
    let mut rows = Vec::with_capacity(instances.len());
    for (k, i) in instances.iter().enumerate() {
        let entry = inner
            .accels
            .get(i.geometry.index(), i.geometry.generation())
            .ok_or_else(|| {
                invalid(format!(
                    "instance {k} names a destroyed or unknown structure"
                ))
            })?;
        if !entry.children.is_empty() {
            return Err(invalid(format!("instance {k} names a top level")));
        }
        let at = match children.iter().position(|c| *c == i.geometry) {
            Some(at) => at,
            None => {
                children.push(i.geometry);
                objects.push(entry.raw.clone());
                children.len() - 1
            }
        };
        let t = i.transform;
        let column = |c: usize| MTLPackedFloat3 {
            x: t[0][c],
            y: t[1][c],
            z: t[2][c],
        };
        rows.push(MTLAccelerationStructureUserIDInstanceDescriptor {
            transformationMatrix: MTLPackedFloat4x3 {
                columns: [column(0), column(1), column(2), column(3)],
            },
            options: MTLAccelerationStructureInstanceOptions::Opaque,
            mask: u32::from(i.mask),
            intersectionFunctionTableOffset: 0,
            accelerationStructureIndex: at as u32,
            userID: i.id,
        });
    }
    let bytes = std::mem::size_of_val(rows.as_slice()).max(16);
    let buffer = inner
        .device
        .newBufferWithLength_options(bytes, MTLResourceOptions::StorageModeShared)
        .ok_or_else(|| RhiError::OutOfMemory("instance descriptors".into()))?;
    // SAFETY: the shared buffer holds `bytes` ≥ the rows' size; nothing else uses it yet.
    unsafe {
        std::ptr::copy_nonoverlapping(
            rows.as_ptr(),
            buffer
                .contents()
                .as_ptr()
                .cast::<MTLAccelerationStructureUserIDInstanceDescriptor>(),
            rows.len(),
        );
    }
    let desc = MTLInstanceAccelerationStructureDescriptor::descriptor();
    let array = NSArray::from_retained_slice(&objects);
    desc.setInstancedAccelerationStructures(Some(&array));
    desc.setInstanceCount(rows.len());
    desc.setInstanceDescriptorBuffer(Some(&buffer));
    desc.setInstanceDescriptorType(MTLAccelerationStructureInstanceDescriptorType::UserID);
    Ok((Retained::into_super(desc), children, buffer))
}

/// Allocates and builds `descriptor`, waiting for the GPU.
fn encode_build(
    inner: &Inner,
    descriptor: &MTLAccelerationStructureDescriptor,
    label: &str,
) -> Result<Retained<MtlAccel>, RhiError> {
    let sizes = inner
        .device
        .accelerationStructureSizesWithDescriptor(descriptor);
    let raw = inner
        .device
        .newAccelerationStructureWithSize(sizes.accelerationStructureSize.max(1))
        .ok_or_else(|| RhiError::OutOfMemory(format!("acceleration structure {label:?}")))?;
    let scratch = inner
        .device
        .newBufferWithLength_options(
            sizes.buildScratchBufferSize.max(16),
            MTLResourceOptions::StorageModePrivate,
        )
        .ok_or_else(|| RhiError::OutOfMemory(format!("build scratch for {label:?}")))?;
    let cb = inner
        .queue
        .commandBuffer()
        .ok_or_else(|| RhiError::DeviceLost("no command buffer for a build".into()))?;
    let encoder = cb
        .accelerationStructureCommandEncoder()
        .ok_or_else(|| RhiError::DeviceLost("no acceleration-structure encoder".into()))?;
    encoder.buildAccelerationStructure_descriptor_scratchBuffer_scratchBufferOffset(
        &raw, descriptor, &scratch, 0,
    );
    encoder.endEncoding();
    cb.commit();
    cb.waitUntilCompleted();
    if let Some(error) = cb.error() {
        return Err(RhiError::DeviceLost(format!(
            "building {label:?}: {}",
            error.localizedDescription()
        )));
    }
    Ok(raw)
}

/// The bindless slot of `accel`.
pub(crate) fn index(inner: &Inner, accel: AccelerationStructure) -> Result<u32, RhiError> {
    inner
        .accels
        .get(accel.index(), accel.generation())
        .map(|e| e.slot)
        .ok_or_else(|| stale(accel))
}

/// Frees `accel` and its slot.
pub(crate) fn destroy(inner: &mut Inner, accel: AccelerationStructure) {
    if let Some(entry) = inner.accels.remove(accel.index(), accel.generation()) {
        inner.accel_table.release(entry.slot);
    }
}

fn buffer_for(
    inner: &Inner,
    buffer: concordia_rhi::Buffer,
    what: &str,
) -> Result<Retained<ProtocolObject<dyn MTLBuffer>>, RhiError> {
    inner
        .buffers
        .get(buffer.index(), buffer.generation())
        .map(|b| b.raw.clone())
        .ok_or_else(|| invalid(format!("the {what} buffer is destroyed or unknown")))
}

fn invalid(reason: String) -> RhiError {
    RhiError::Invalid {
        what: "acceleration structure desc",
        reason,
    }
}

fn stale(accel: AccelerationStructure) -> RhiError {
    RhiError::StaleHandle {
        kind: "acceleration structure",
        index: accel.index(),
        generation: accel.generation(),
    }
}
