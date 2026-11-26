//! GPU resources: batched uploads and the mesh/texture caches.

use std::collections::BTreeMap;
use std::num::{NonZeroU32, NonZeroU64};

use concordia_assets::{AssetRef, MeshData, TextureData, TextureFormat};
use concordia_core::math::Vec3;
use concordia_rhi::{
    Buffer, BufferDesc, BufferUsage, Command, CommandList, Device, Format, MemoryLocation,
    QueueKind, RhiError, SampleCount, Submit, Texture, TextureDesc, TextureUsage,
};

/// Buffers reused across frames (per-frame data, staging, readback), matched by usage and memory.
#[derive(Debug, Default)]
pub(crate) struct BufferPool {
    free: Vec<(Buffer, BufferDesc)>,
    out: BTreeMap<Buffer, BufferDesc>,
    /// Buffers created so far (for `RenderStats::allocations`).
    pub(crate) created: usize,
}

/// A pool shared by the renderer and its per-frame uploads.
pub(crate) type SharedPool = std::sync::Arc<std::sync::Mutex<BufferPool>>;

impl BufferPool {
    /// Free buffers kept for reuse; beyond this the oldest are destroyed.
    const KEEP: usize = 64;

    /// A buffer with `desc`'s usage and memory and at least its size (capacity is a power of two).
    pub(crate) fn take(
        &mut self,
        device: &dyn Device,
        desc: &BufferDesc,
    ) -> Result<Buffer, RhiError> {
        let fits = |d: &BufferDesc| {
            d.usage == desc.usage
                && d.memory == desc.memory
                && d.size >= desc.size
                && d.size.get() <= desc.size.get().next_power_of_two().max(256)
        };
        if let Some(i) = self.free.iter().position(|(_, d)| fits(d)) {
            let (buffer, d) = self.free.swap_remove(i);
            self.out.insert(buffer, d);
            return Ok(buffer);
        }
        let size =
            NonZeroU64::new(desc.size.get().next_power_of_two().max(256)).unwrap_or(desc.size);
        let d = BufferDesc {
            size,
            ..desc.clone()
        };
        let buffer = device.create_buffer(&d)?;
        self.created += 1;
        self.out.insert(buffer, d);
        Ok(buffer)
    }

    /// Takes back a buffer from [`BufferPool::take`] (the GPU must be done with it); `false` if it
    /// is not the pool's.
    pub(crate) fn give(&mut self, device: &dyn Device, buffer: Buffer) -> bool {
        let Some(desc) = self.out.remove(&buffer) else {
            return false;
        };
        self.free.push((buffer, desc));
        if self.free.len() > Self::KEEP {
            let (oldest, _) = self.free.remove(0);
            device.destroy_buffer(oldest);
        }
        true
    }

    /// Destroys every free buffer.
    pub(crate) fn clear(&mut self, device: &dyn Device) {
        self.free
            .drain(..)
            .for_each(|(b, _)| device.destroy_buffer(b));
    }
}

/// Copies recorded now and submitted together by [`Uploads::flush`], or recorded at the start of
/// a frame by [`Uploads::into_commands`].
pub(crate) struct Uploads {
    list: CommandList,
    staging: Vec<Buffer>,
    buffers: Vec<Buffer>,
    textures: Vec<Texture>,
    /// Where per-frame buffers come from (none: create and destroy them).
    pool: Option<SharedPool>,
}

impl Uploads {
    pub(crate) fn new() -> Self {
        Self {
            list: CommandList::new("uploads"),
            staging: Vec::new(),
            buffers: Vec::new(),
            textures: Vec::new(),
            pool: None,
        }
    }

    /// Uploads whose buffers come from (and, once their frame retires, go back to) `pool`.
    pub(crate) fn pooled(pool: SharedPool) -> Self {
        Self {
            pool: Some(pool),
            ..Self::new()
        }
    }

    /// A buffer for `desc`, from the pool when there is one.
    pub(crate) fn alloc(&self, device: &dyn Device, desc: &BufferDesc) -> Result<Buffer, RhiError> {
        match &self.pool {
            Some(pool) => pool
                .lock()
                .map_err(|_| RhiError::invalid("buffer pool", "poisoned"))?
                .take(device, desc),
            None => device.create_buffer(desc),
        }
    }

    fn stage(&mut self, device: &dyn Device, bytes: &[u8]) -> Result<Buffer, RhiError> {
        let staging = self.alloc(
            device,
            &BufferDesc {
                size: nonzero(bytes.len()),
                usage: BufferUsage::COPY_SRC,
                memory: MemoryLocation::Upload,
                label: "staging".into(),
            },
        )?;
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
        let gpu = self.alloc(
            device,
            &BufferDesc {
                size: nonzero(bytes.len()),
                usage,
                memory: MemoryLocation::GpuOnly,
                label: label.into(),
            },
        )?;
        self.buffers.push(gpu);
        self.write_into(device, gpu, bytes)?;
        Ok(gpu)
    }

    /// Copies `bytes` into the start of the existing buffer `dst` (it needs `COPY_DST` usage).
    pub(crate) fn write_into(
        &mut self,
        device: &dyn Device,
        dst: Buffer,
        bytes: &[u8],
    ) -> Result<(), RhiError> {
        if bytes.is_empty() {
            return Ok(());
        }
        let src = self.stage(device, bytes)?;
        self.list.push(Command::CopyBuffer {
            src,
            src_offset: 0,
            dst,
            dst_offset: 0,
            size: bytes.len() as u64,
        });
        Ok(())
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
        self.textures.push(texture);
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

    /// The copies followed by a barrier making them visible to shaders, and the staging buffers,
    /// which must live until the GPU has run the commands.
    pub(crate) fn into_commands(self) -> (CommandList, Vec<Buffer>) {
        let mut list = self.list;
        if !list.commands.is_empty() {
            list.push(upload_barrier(&self.buffers, &self.textures));
        }
        (list, self.staging)
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
        TextureFormat::R8 => Format::R8Unorm,
    }
}

/// No buffer: a stream the mesh lacks (shaders never read it).
pub(crate) const NO_BUFFER: u32 = u32::MAX;

/// A mesh's streams on the GPU (bindless indices are what shaders use).
#[derive(Debug, Clone, Copy)]
pub(crate) struct GpuMesh {
    pub(crate) indices: u32,
    pub(crate) positions: u32,
    pub(crate) normals: u32,
    pub(crate) tangents: u32,
    pub(crate) uvs: u32,
    /// Joint indices (`uint4` per vertex), or [`NO_BUFFER`].
    pub(crate) joints: u32,
    /// Joint weights (`float4` per vertex), or [`NO_BUFFER`].
    pub(crate) weights: u32,
    /// Joints a palette must hold to skin it (the largest index plus one); 0 when unskinned.
    pub(crate) joint_count: u32,
    pub(crate) count: u32,
    /// Object-space bounds (min, max).
    pub(crate) aabb: (Vec3, Vec3),
    /// The `MeshletBlob` v1 buffer's bindless index and counts, when the mesh has valid meshlets.
    pub(crate) meshlets: Option<(u32, crate::meshlet::MeshletInfo)>,
    buffers: [Option<Buffer>; 8],
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
        let skin = match skinnable(mesh) {
            true => Some((
                up.buffer(device, &joint_words(mesh), label)?,
                up.buffer(device, &bytes(mesh.weights.as_flattened()), label)?,
            )),
            false => None,
        };
        let index = |b: Buffer| device.buffer_index(b).map(|i| i.0);
        let blob = match meshlet_info(mesh, label) {
            Some(info) => Some((up.buffer(device, &mesh.meshlets, label)?, info)),
            None => None,
        };
        let joint_count = match skin {
            Some(_) => mesh
                .joints
                .iter()
                .flatten()
                .max()
                .map_or(0, |j| u32::from(*j) + 1),
            None => 0,
        };
        Ok(Self {
            indices: index(indices)?,
            positions: index(positions)?,
            normals: index(normals)?,
            tangents: index(tangents)?,
            uvs: index(uvs)?,
            joints: skin.map_or(Ok(NO_BUFFER), |(j, _)| index(j))?,
            weights: skin.map_or(Ok(NO_BUFFER), |(_, w)| index(w))?,
            joint_count,
            count: mesh.indices.len() as u32,
            aabb: (Vec3::from_array(mesh.aabb.0), Vec3::from_array(mesh.aabb.1)),
            meshlets: match blob {
                Some((b, info)) => Some((index(b)?, info)),
                None => None,
            },
            buffers: [
                Some(indices),
                Some(positions),
                Some(normals),
                Some(tangents),
                Some(uvs),
                skin.map(|(j, _)| j),
                skin.map(|(_, w)| w),
                blob.map(|(b, _)| b),
            ],
        })
    }

    pub(crate) fn destroy(&self, device: &dyn Device) {
        self.buffers().for_each(|b| device.destroy_buffer(b));
    }

    /// The mesh's buffers.
    pub(crate) fn buffers(&self) -> impl Iterator<Item = Buffer> + '_ {
        self.buffers.iter().flatten().copied()
    }
}

/// The counts of `mesh`'s meshlets; an invalid blob is logged and the mesh drawn on the M1 path.
fn meshlet_info(mesh: &MeshData, label: &str) -> Option<crate::meshlet::MeshletInfo> {
    match crate::meshlet::MeshletInfo::of(&mesh.meshlets) {
        Ok(info) => info,
        Err(error) => {
            tracing::warn!(target: "concordia::render", mesh = label, %error, "invalid meshlets ignored; drawn without them");
            None
        }
    }
}

/// A mesh with one joint and one weight row per vertex.
fn skinnable(mesh: &MeshData) -> bool {
    let n = mesh.positions.len();
    n > 0 && mesh.joints.len() == n && mesh.weights.len() == n
}

/// Joint indices widened to one `uint4` row per vertex.
fn joint_words(mesh: &MeshData) -> Vec<u8> {
    mesh.joints
        .iter()
        .flatten()
        .flat_map(|j| u32::from(*j).to_le_bytes())
        .collect()
}

/// Meshes and textures uploaded once and reused across frames.
#[derive(Default)]
pub(crate) struct GpuAssets {
    pub(crate) meshes: BTreeMap<AssetRef, GpuMesh>,
    /// Per skinned mesh, the bind-space box of each joint's vertices (`concordia_anim::joint_bounds`).
    pub(crate) joint_bounds: BTreeMap<AssetRef, concordia_anim::JointBounds>,
    pub(crate) textures: BTreeMap<AssetRef, (Texture, u32)>,
}

impl GpuAssets {
    pub(crate) fn destroy(&mut self, device: &dyn Device) {
        self.meshes.values().for_each(|m| m.destroy(device));
        self.textures
            .values()
            .for_each(|(t, _)| device.destroy_texture(*t));
        self.meshes.clear();
        self.joint_bounds.clear();
        self.textures.clear();
    }
}

/// Copy writes before shader reads, for freshly uploaded buffers and textures.
fn upload_barrier(buffers: &[Buffer], textures: &[Texture]) -> Command {
    use concordia_rhi::{Access, AccessFlags, BufferBarrier, Layout, Stages, TextureBarrier};
    let copied = |layout| Access {
        stages: Stages::COPY,
        access: AccessFlags::COPY_WRITE,
        layout,
    };
    let read = |layout| Access {
        stages: Stages::VERTEX | Stages::FRAGMENT | Stages::COMPUTE,
        access: AccessFlags::SHADER_READ,
        layout,
    };
    Command::Barrier {
        buffers: buffers
            .iter()
            .map(|&buffer| BufferBarrier {
                buffer,
                before: copied(Layout::General),
                after: read(Layout::General),
            })
            .collect(),
        textures: textures
            .iter()
            .map(|&texture| TextureBarrier {
                texture,
                before: copied(Layout::CopyDst),
                after: read(Layout::ShaderRead),
                aliasing: None,
            })
            .collect(),
    }
}
