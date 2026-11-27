//! The bindless tables: GPU addresses of buffers and resource ids of textures, written into
//! shared buffers that shaders read as argument buffers (Metal 3).

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLBuffer, MTLDevice, MTLResourceOptions, MTLSamplerAddressMode, MTLSamplerDescriptor,
    MTLSamplerMinMagFilter, MTLSamplerMipFilter, MTLSamplerState, MTLTexture,
};

use concordia_rhi::StaticSampler;

use concordia_rhi::RhiError;

use crate::BINDLESS_CAPACITY;

pub(crate) type MtlBuffer = ProtocolObject<dyn MTLBuffer>;
pub(crate) type MtlTexture = ProtocolObject<dyn MTLTexture>;
pub(crate) type MtlSampler = ProtocolObject<dyn MTLSamplerState>;

/// One table of 8-byte entries plus its free slots; `fixed` entries follow the slots (static samplers).
#[derive(Debug)]
pub(crate) struct Table {
    raw: Retained<MtlBuffer>,
    free: Vec<u32>,
    capacity: u32,
}

impl Table {
    pub(crate) fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        label: &str,
        fixed: usize,
    ) -> Result<Self, RhiError> {
        Self::with_capacity(device, label, BINDLESS_CAPACITY, fixed)
    }

    /// A table of `capacity` slots (the acceleration-structure table is smaller, M4 4.8c).
    pub(crate) fn with_capacity(
        device: &ProtocolObject<dyn MTLDevice>,
        label: &str,
        capacity: u32,
        fixed: usize,
    ) -> Result<Self, RhiError> {
        let bytes = (capacity as usize + fixed) * 8;
        let raw = device
            .newBufferWithLength_options(bytes, MTLResourceOptions::StorageModeShared)
            .ok_or_else(|| RhiError::OutOfMemory(format!("bindless {label} table")))?;
        Ok(Self {
            raw,
            capacity,
            free: (0..capacity).rev().collect(),
        })
    }

    pub(crate) fn raw(&self) -> &MtlBuffer {
        &self.raw
    }

    /// Takes a slot and writes `value` (a GPU address or resource id) into it.
    pub(crate) fn insert(&mut self, value: u64) -> Result<u32, RhiError> {
        let slot = self.free.pop().ok_or_else(|| {
            RhiError::OutOfMemory(format!("bindless slot (capacity {})", self.capacity))
        })?;
        self.write(slot, value);
        Ok(slot)
    }

    pub(crate) fn release(&mut self, slot: u32) {
        self.write(slot, 0);
        self.free.push(slot);
    }

    /// Writes fixed entry `index` (after the slots), e.g. a static sampler's resource id.
    pub(crate) fn write_fixed(&self, index: usize, value: u64) {
        debug_assert!((self.capacity as usize + index) * 8 < self.raw.length());
        self.write_at(self.capacity as usize + index, value);
    }

    fn write(&self, slot: u32, value: u64) {
        debug_assert!(slot < self.capacity);
        self.write_at(slot as usize, value);
    }

    fn write_at(&self, entry: usize, value: u64) {
        let base = self.raw.contents().as_ptr().cast::<u64>();
        // SAFETY: callers keep `entry` inside the buffer (asserted above); the buffer is shared
        // (CPU-visible) and only written between submissions.
        unsafe { base.add(entry).write(value) };
    }
}

/// A buffer's GPU address for the buffer table.
pub(crate) fn buffer_entry(buffer: &MtlBuffer) -> u64 {
    buffer.gpuAddress()
}

/// A texture's resource id for the texture table.
pub(crate) fn texture_entry(texture: &MtlTexture) -> u64 {
    resource_id(texture.gpuResourceID())
}

/// Creates the static samplers (argument-buffer capable) and writes them after the texture slots.
pub(crate) fn install_static_samplers(
    device: &ProtocolObject<dyn MTLDevice>,
    textures: &Table,
) -> Result<Vec<Retained<MtlSampler>>, RhiError> {
    StaticSampler::ALL
        .iter()
        .map(|&kind| {
            let sampler = new_sampler(device, kind)?;
            textures.write_fixed(kind.index() as usize, resource_id(sampler.gpuResourceID()));
            Ok(sampler)
        })
        .collect()
}

fn new_sampler(
    device: &ProtocolObject<dyn MTLDevice>,
    kind: StaticSampler,
) -> Result<Retained<MtlSampler>, RhiError> {
    let desc = MTLSamplerDescriptor::new();
    let (filter, mip) = match kind.is_linear() {
        true => (MTLSamplerMinMagFilter::Linear, MTLSamplerMipFilter::Linear),
        false => (
            MTLSamplerMinMagFilter::Nearest,
            MTLSamplerMipFilter::Nearest,
        ),
    };
    let address = match kind.repeats() {
        true => MTLSamplerAddressMode::Repeat,
        false => MTLSamplerAddressMode::ClampToEdge,
    };
    desc.setMinFilter(filter);
    desc.setMagFilter(filter);
    desc.setMipFilter(mip);
    desc.setSAddressMode(address);
    desc.setTAddressMode(address);
    desc.setRAddressMode(address);
    if kind == StaticSampler::LinearRepeat {
        desc.setMaxAnisotropy(8);
    }
    desc.setSupportArgumentBuffers(true);
    device
        .newSamplerStateWithDescriptor(&desc)
        .ok_or_else(|| RhiError::Unsupported(format!("could not create static sampler {kind:?}")))
}

/// An `MTLResourceID` as the 8-byte table entry.
pub(crate) fn resource_id(id: objc2_metal::MTLResourceID) -> u64 {
    // SAFETY: MTLResourceID is #[repr(C)] around a single u64 (Apple: "an opaque 64-bit handle").
    unsafe { std::mem::transmute::<objc2_metal::MTLResourceID, u64>(id) }
}
