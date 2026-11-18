//! GPU resources: batched uploads and the mesh/texture caches.

use std::collections::BTreeMap;
use std::num::{NonZeroU32, NonZeroU64};

use concordia_assets::{AssetRef, MeshData, TextureData, TextureFormat};
use concordia_rhi::{
    Buffer, BufferDesc, BufferUsage, Command, CommandList, Device, Format, MemoryLocation,
    QueueKind, RhiError, SampleCount, Submit, Texture, TextureDesc, TextureUsage,
};

/// Copies recorded now and submitted together by [`Uploads::flush`].
pub(crate) struct Uploads {
    list: CommandList,
    staging: Vec<Buffer>,
}

impl Uploads {
    pub(crate) fn new() -> Self {
        Self {
            list: CommandList::new("uploads"),
            staging: Vec::new(),
        }
    }

    fn stage(&mut self, device: &dyn Device, bytes: &[u8]) -> Result<Buffer, RhiError> {
        let staging = device.create_buffer(&BufferDesc {
            size: nonzero(bytes.len()),
            usage: BufferUsage::COPY_SRC,
            memory: MemoryLocation::Upload,
            label: "staging".into(),
        })?;
        device.write_buffer(staging, 0, bytes)?;
        self.staging.push(staging);
        Ok(staging)
    }

    /// A GPU-only bindless buffer holding `bytes` (at least 16 bytes, so empty streams stay valid).
    pub(crate) fn buffer(
        &mut self,
        device: &dyn Device,
        bytes: &[u8],
        label: &str,
    ) -> Result<Buffer, RhiError> {
        let padded;
        let bytes = match bytes.len() {
            0..16 => {
                padded = [bytes, &[0u8; 16][bytes.len()..]].concat();
                &padded[..]
            }
            _ => bytes,
        };
        let usage = BufferUsage::STORAGE | BufferUsage::BINDLESS | BufferUsage::COPY_DST;
        let gpu = device.create_buffer(&BufferDesc {
            size: nonzero(bytes.len()),
            usage,
            memory: MemoryLocation::GpuOnly,
            label: label.into(),
        })?;
        let src = self.stage(device, bytes)?;
        self.list.push(Command::CopyBuffer {
            src,
            src_offset: 0,
            dst: gpu,
            dst_offset: 0,
            size: bytes.len() as u64,
        });
        Ok(gpu)
    }

    /// A sampled bindless texture with every mip of `data`.
    pub(crate) fn texture(
        &mut self,
        device: &dyn Device,
        data: &TextureData,
        label: &str,
    ) -> Result<Texture, RhiError> {
        let invalid = |why: String| RhiError::invalid("texture data", why);
        let width = NonZeroU32::new(data.width).ok_or_else(|| invalid("zero width".into()))?;
        let height = NonZeroU32::new(data.height).ok_or_else(|| invalid("zero height".into()))?;
        let mips =
            NonZeroU32::new(data.mips.len() as u32).ok_or_else(|| invalid("no mips".into()))?;
        let usage = TextureUsage::SAMPLED | TextureUsage::COPY_DST | TextureUsage::BINDLESS;
        let format = format_of(data.format);
        let texture = device.create_texture(&TextureDesc {
            width,
            height,
            mip_levels: mips,
            samples: SampleCount::X1,
            format,
            usage,
            label: label.into(),
        })?;
        for (mip, bytes) in data.mips.iter().enumerate() {
            let w = (data.width >> mip).max(1);
            let src = self.stage(device, bytes)?;
            let bytes_per_row = w * data.format.bytes_per_pixel() as u32;
            self.list.push(Command::CopyBufferToTexture {
                src,
                offset: 0,
                bytes_per_row,
                dst: texture,
                mip: mip as u32,
            });
        }
        Ok(texture)
    }

    /// Submits the copies, waits, and frees the staging buffers.
    pub(crate) fn flush(self, device: &dyn Device) -> Result<(), RhiError> {
        if !self.list.commands.is_empty() {
            device.submit(Submit {
                queue: QueueKind::Graphics,
                commands: &self.list,
                wait: &[],
                signal: &[],
            })?;
            device.wait_idle()?;
        }
        self.staging
            .into_iter()
            .for_each(|b| device.destroy_buffer(b));
        Ok(())
    }
}

fn nonzero(len: usize) -> NonZeroU64 {
    NonZeroU64::new(len as u64).unwrap_or(NonZeroU64::MIN)
}

/// The RHI format of an asset texture format.
pub(crate) fn format_of(format: TextureFormat) -> Format {
    match format {
        TextureFormat::Rgba8Srgb => Format::Rgba8UnormSrgb,
        TextureFormat::Rgba8Linear => Format::Rgba8Unorm,
        TextureFormat::Rgba16Float => Format::Rgba16Float,
    }
}

/// A mesh's streams on the GPU (bindless indices are what shaders use).
#[derive(Debug, Clone, Copy)]
pub(crate) struct GpuMesh {
    pub(crate) indices: u32,
    pub(crate) positions: u32,
    pub(crate) normals: u32,
    pub(crate) tangents: u32,
    pub(crate) uvs: u32,
    pub(crate) count: u32,
    buffers: [Buffer; 5],
}

impl GpuMesh {
    pub(crate) fn upload(
        device: &dyn Device,
        up: &mut Uploads,
        mesh: &MeshData,
        label: &str,
    ) -> Result<Self, RhiError> {
        let bytes = |floats: &[f32]| {
            floats
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect::<Vec<u8>>()
        };
        let indices = up.buffer(
            device,
            &mesh
                .indices
                .iter()
                .flat_map(|i| i.to_le_bytes())
                .collect::<Vec<_>>(),
            label,
        )?;
        let positions = up.buffer(device, &bytes(mesh.positions.as_flattened()), label)?;
        let normals = up.buffer(device, &bytes(mesh.normals.as_flattened()), label)?;
        let tangents = up.buffer(device, &bytes(mesh.tangents.as_flattened()), label)?;
        let uvs = up.buffer(device, &bytes(mesh.uv0.as_flattened()), label)?;
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        Ok(Self {
            indices: index(indices)?,
            positions: index(positions)?,
            normals: index(normals)?,
            tangents: index(tangents)?,
            uvs: index(uvs)?,
            count: mesh.indices.len() as u32,
            buffers: [indices, positions, normals, tangents, uvs],
        })
    }

    pub(crate) fn destroy(&self, device: &dyn Device) {
        self.buffers.iter().for_each(|b| device.destroy_buffer(*b));
    }
}

/// Meshes and textures uploaded once and reused across frames.
#[derive(Default)]
pub(crate) struct GpuAssets {
    pub(crate) meshes: BTreeMap<AssetRef, GpuMesh>,
    pub(crate) textures: BTreeMap<AssetRef, (Texture, u32)>,
}

impl GpuAssets {
    pub(crate) fn destroy(&mut self, device: &dyn Device) {
        self.meshes.values().for_each(|m| m.destroy(device));
        self.textures
            .values()
            .for_each(|(t, _)| device.destroy_texture(*t));
        self.meshes.clear();
        self.textures.clear();
    }
}
