//! The bindless tables: GPU addresses of buffers and resource ids of textures, written into
//! shared buffers that shaders read as argument buffers (Metal 3).

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions, MTLTexture};

use concordia_rhi::RhiError;

use crate::BINDLESS_CAPACITY;

pub(crate) type MtlBuffer = ProtocolObject<dyn MTLBuffer>;
pub(crate) type MtlTexture = ProtocolObject<dyn MTLTexture>;

/// One table of 8-byte entries plus its free slots.
#[derive(Debug)]
pub(crate) struct Table {
    raw: Retained<MtlBuffer>,
    free: Vec<u32>,
}

impl Table {
    pub(crate) fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        label: &str,
    ) -> Result<Self, RhiError> {
        let bytes = BINDLESS_CAPACITY as usize * 8;
        let raw = device
            .newBufferWithLength_options(bytes, MTLResourceOptions::StorageModeShared)
            .ok_or_else(|| RhiError::OutOfMemory(format!("bindless {label} table")))?;
        Ok(Self {
            raw,
            free: (0..BINDLESS_CAPACITY).rev().collect(),
        })
    }

    pub(crate) fn raw(&self) -> &MtlBuffer {
        &self.raw
    }

    /// Takes a slot and writes `value` (a GPU address or resource id) into it.
    pub(crate) fn insert(&mut self, value: u64) -> Result<u32, RhiError> {
        let slot = self.free.pop().ok_or_else(|| {
            RhiError::OutOfMemory(format!("bindless slot (capacity {BINDLESS_CAPACITY})"))
        })?;
        self.write(slot, value);
        Ok(slot)
    }

    pub(crate) fn release(&mut self, slot: u32) {
        self.write(slot, 0);
        self.free.push(slot);
    }

    fn write(&self, slot: u32, value: u64) {
        debug_assert!(slot < BINDLESS_CAPACITY);
        let base = self.raw.contents().as_ptr().cast::<u64>();
        // SAFETY: the buffer holds BINDLESS_CAPACITY u64 entries and slot is below it; the
        // buffer is shared (CPU-visible) and only written between submissions.
        unsafe { base.add(slot as usize).write(value) };
    }
}

/// A buffer's GPU address for the buffer table.
pub(crate) fn buffer_entry(buffer: &MtlBuffer) -> u64 {
    buffer.gpuAddress()
}

/// A texture's resource id for the texture table.
pub(crate) fn texture_entry(texture: &MtlTexture) -> u64 {
    let id = texture.gpuResourceID();
    // SAFETY: MTLResourceID is #[repr(C)] around a single u64 (Apple: "an opaque 64-bit handle").
    unsafe { std::mem::transmute::<objc2_metal::MTLResourceID, u64>(id) }
}
