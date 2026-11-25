//! The bindless tables (ADR-0088): set 0 a storage buffer of buffer addresses, set 1 sampled images
//! and the static samplers, set 2 storage images; one pipeline layout with 128 bytes of push constants.

use ash::vk;
use concordia_rhi::{MAX_PUSH_CONSTANTS, RhiError, StaticSampler};
use gpu_allocator::vulkan::Allocator;

use crate::convert::{self, error};
use crate::record::BINDLESS_CAPACITY;
use crate::resources::{self, Ctx, VkBuffer};

/// Every stage reads the tables and push constants.
pub(crate) const STAGES: vk::ShaderStageFlags = vk::ShaderStageFlags::from_raw(
    vk::ShaderStageFlags::VERTEX.as_raw()
        | vk::ShaderStageFlags::FRAGMENT.as_raw()
        | vk::ShaderStageFlags::COMPUTE.as_raw(),
);

/// Free slots in one table.
#[derive(Debug, Default)]
pub(crate) struct Indices {
    next: u32,
    free: Vec<u32>,
}

impl Indices {
    /// A free slot, or `None` when all [`BINDLESS_CAPACITY`] are taken.
    pub fn take(&mut self) -> Option<u32> {
        self.free.pop().or_else(|| {
            (self.next < BINDLESS_CAPACITY).then(|| {
                self.next += 1;
                self.next - 1
            })
        })
    }

    /// Returns a slot (once the GPU has finished with it).
    pub fn give_back(&mut self, index: u32) {
        debug_assert!(index < self.next && !self.free.contains(&index));
        self.free.push(index);
    }
}

/// The device's bindless tables.
pub(crate) struct Bindless {
    pub layouts: [vk::DescriptorSetLayout; 3],
    pub pool: vk::DescriptorPool,
    pub sets: [vk::DescriptorSet; 3],
    pub pipeline_layout: vk::PipelineLayout,
    pub samplers: Vec<vk::Sampler>,
    /// `BINDLESS_CAPACITY` device addresses (set 0, binding 0).
    pub addresses: VkBuffer,
    pub buffer_slots: Indices,
    pub texture_slots: Indices,
    /// Empty texture slots hold null descriptors (`VK_EXT_robustness2`), which read as zero as
    /// Metal's nil entries do; without them an unwritten or freed slot is undefined to read.
    pub nulls: bool,
}

impl Bindless {
    /// Creates the tables (`anisotropy`: the device's sampler anisotropy feature; `nulls`: it
    /// has `nullDescriptor`, so every texture slot starts null).
    pub fn new(
        device: &ash::Device,
        allocator: &mut Allocator,
        anisotropy: bool,
        nulls: bool,
    ) -> Result<Self, RhiError> {
        let mut made = Partial::new(device);
        let result = made.build(allocator, anisotropy);
        match result {
            Ok(mut tables) => {
                tables.nulls = nulls;
                tables.clear_all(device);
                Ok(tables)
            }
            Err(e) => {
                made.destroy();
                Err(e)
            }
        }
    }

    /// Points buffer slot `index` at `address` (0 clears it).
    pub fn write_address(&mut self, index: u32, address: vk::DeviceAddress) {
        if let Ok(bytes) = self.addresses.mapped() {
            let at = index as usize * 8;
            if let Some(slot) = bytes.get_mut(at..at + 8) {
                slot.copy_from_slice(&address.to_le_bytes());
            }
        }
    }

    /// Makes texture slot `index` null in both sets (when the tables hold nulls), after its texture
    /// is freed.
    pub fn clear_texture(&self, device: &ash::Device, index: u32) {
        self.write_texture(device, index, None, None);
    }

    /// Makes every texture slot null (when the tables hold nulls).
    fn clear_all(&self, device: &ash::Device) {
        if !self.nulls {
            return;
        }
        let sampled =
            vec![null_image(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL); BINDLESS_CAPACITY as usize];
        let storage = vec![null_image(vk::ImageLayout::GENERAL); BINDLESS_CAPACITY as usize];
        let writes = [
            image_write(self.sets[1], 0, vk::DescriptorType::SAMPLED_IMAGE, &sampled),
            image_write(self.sets[2], 0, vk::DescriptorType::STORAGE_IMAGE, &storage),
        ];
        // SAFETY: the sets are new and unused, nullDescriptor is enabled, and each write covers
        // the binding's BINDLESS_CAPACITY elements.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
    }

    /// Points texture slot `index` at `sampled` (set 1) and/or `storage` (set 2) views; a set
    /// given no view gets a null descriptor when the tables hold nulls.
    pub fn write_texture(
        &self,
        device: &ash::Device,
        index: u32,
        sampled: Option<(vk::ImageView, vk::ImageLayout)>,
        storage: Option<vk::ImageView>,
    ) {
        let or_null = |view: Option<(vk::ImageView, vk::ImageLayout)>, layout| match view {
            Some((view, layout)) => Some([vk::DescriptorImageInfo::default()
                .image_view(view)
                .image_layout(layout)]),
            None => self.nulls.then(|| [null_image(layout)]),
        };
        let sampled_info = or_null(sampled, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let storage_info = or_null(
            storage.map(|view| (view, vk::ImageLayout::GENERAL)),
            vk::ImageLayout::GENERAL,
        );
        let mut writes = Vec::new();
        if let Some(info) = &sampled_info {
            writes.push(image_write(
                self.sets[1],
                index,
                vk::DescriptorType::SAMPLED_IMAGE,
                info,
            ));
        }
        if let Some(info) = &storage_info {
            writes.push(image_write(
                self.sets[2],
                index,
                vk::DescriptorType::STORAGE_IMAGE,
                info,
            ));
        }
        if writes.is_empty() {
            return;
        }
        // SAFETY: the sets are update-after-bind and partially bound, and slot `index` is not used
        // by any pending submission (slots are reused only after retirement); the views are live,
        // or null with nullDescriptor enabled.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
    }

    /// Destroys the tables (the GPU must be idle).
    pub fn destroy(self, device: &ash::Device, allocator: &mut Allocator) {
        let mut made = Partial {
            device,
            layouts: self.layouts.to_vec(),
            pool: self.pool,
            pipeline_layout: self.pipeline_layout,
            samplers: self.samplers,
        };
        made.destroy();
        resources::free(
            &mut Ctx { device, allocator },
            resources::Dead::Buffer(self.addresses),
        );
    }
}

/// A null image descriptor (`nullDescriptor`): reads return zero.
fn null_image(layout: vk::ImageLayout) -> vk::DescriptorImageInfo {
    vk::DescriptorImageInfo::default().image_layout(layout)
}

/// A write of `info.len()` image descriptors from element `index`.
fn image_write<'a>(
    set: vk::DescriptorSet,
    index: u32,
    kind: vk::DescriptorType,
    info: &'a [vk::DescriptorImageInfo],
) -> vk::WriteDescriptorSet<'a> {
    vk::WriteDescriptorSet::default()
        .dst_set(set)
        .dst_binding(0)
        .dst_array_element(index)
        .descriptor_type(kind)
        .image_info(info)
}

/// Objects created so far, so a failed build can undo them.
struct Partial<'d> {
    device: &'d ash::Device,
    layouts: Vec<vk::DescriptorSetLayout>,
    pool: vk::DescriptorPool,
    pipeline_layout: vk::PipelineLayout,
    samplers: Vec<vk::Sampler>,
}

impl<'d> Partial<'d> {
    fn new(device: &'d ash::Device) -> Self {
        Self {
            device,
            layouts: Vec::new(),
            pool: vk::DescriptorPool::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            samplers: Vec::new(),
        }
    }

    fn build(&mut self, allocator: &mut Allocator, anisotropy: bool) -> Result<Bindless, RhiError> {
        for s in StaticSampler::ALL {
            // SAFETY: the sampler description is valid (anisotropy only when the feature is on).
            let sampler = unsafe {
                self.device
                    .create_sampler(&convert::sampler(s, anisotropy), None)
            }
            .map_err(|e| error("vkCreateSampler", e))?;
            self.samplers.push(sampler);
        }
        self.layouts.push(self.buffer_layout()?);
        self.layouts.push(self.texture_layout()?);
        self.layouts.push(self.storage_layout()?);
        self.pipeline_layout = self.create_pipeline_layout()?;
        self.pool = self.create_pool()?;
        let sets = self.allocate_sets()?;
        let mut ctx = Ctx {
            device: self.device,
            allocator,
        };
        let addresses = create_address_table(&mut ctx)?;
        self.point_set0_at(sets[0], &addresses);
        Ok(Bindless {
            layouts: [self.layouts[0], self.layouts[1], self.layouts[2]],
            pool: self.pool,
            sets,
            pipeline_layout: self.pipeline_layout,
            samplers: std::mem::take(&mut self.samplers),
            addresses,
            buffer_slots: Indices::default(),
            texture_slots: Indices::default(),
            nulls: false,
        })
    }

    fn set_layout(
        &self,
        bindings: &[vk::DescriptorSetLayoutBinding<'_>],
        flags: &[vk::DescriptorBindingFlags],
        update_after_bind: bool,
    ) -> Result<vk::DescriptorSetLayout, RhiError> {
        let mut binding_flags =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(flags);
        let layout_flags = match update_after_bind {
            true => vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL,
            false => vk::DescriptorSetLayoutCreateFlags::empty(),
        };
        let info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(layout_flags)
            .bindings(bindings)
            .push_next(&mut binding_flags);
        // SAFETY: `info` and the arrays it points to are valid for the call.
        unsafe { self.device.create_descriptor_set_layout(&info, None) }
            .map_err(|e| error("vkCreateDescriptorSetLayout", e))
    }

    fn buffer_layout(&self) -> Result<vk::DescriptorSetLayout, RhiError> {
        let binding = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(STAGES)];
        self.set_layout(&binding, &[vk::DescriptorBindingFlags::empty()], false)
    }

    fn texture_layout(&self) -> Result<vk::DescriptorSetLayout, RhiError> {
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(BINDLESS_CAPACITY)
                .stage_flags(STAGES),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .stage_flags(STAGES)
                .immutable_samplers(&self.samplers),
        ];
        self.set_layout(
            &bindings,
            &[table_flags(), vk::DescriptorBindingFlags::empty()],
            true,
        )
    }

    fn storage_layout(&self) -> Result<vk::DescriptorSetLayout, RhiError> {
        let binding = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
            .descriptor_count(BINDLESS_CAPACITY)
            .stage_flags(STAGES)];
        self.set_layout(&binding, &[table_flags()], true)
    }

    fn create_pipeline_layout(&self) -> Result<vk::PipelineLayout, RhiError> {
        let push = [vk::PushConstantRange::default()
            .stage_flags(STAGES)
            .offset(0)
            .size(MAX_PUSH_CONSTANTS as u32)];
        let info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&self.layouts)
            .push_constant_ranges(&push);
        // SAFETY: the set layouts are live; `info` is valid for the call.
        unsafe { self.device.create_pipeline_layout(&info, None) }
            .map_err(|e| error("vkCreatePipelineLayout", e))
    }

    fn create_pool(&self) -> Result<vk::DescriptorPool, RhiError> {
        let sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: BINDLESS_CAPACITY,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: StaticSampler::ALL.len() as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: BINDLESS_CAPACITY,
            },
        ];
        let info = vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
            .max_sets(3)
            .pool_sizes(&sizes);
        // SAFETY: `info` is valid for the call.
        unsafe { self.device.create_descriptor_pool(&info, None) }
            .map_err(|e| error("vkCreateDescriptorPool", e))
    }

    fn allocate_sets(&self) -> Result<[vk::DescriptorSet; 3], RhiError> {
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.pool)
            .set_layouts(&self.layouts);
        // SAFETY: the pool has room for exactly these three sets.
        let sets = unsafe { self.device.allocate_descriptor_sets(&info) }
            .map_err(|e| error("vkAllocateDescriptorSets", e))?;
        Ok([sets[0], sets[1], sets[2]])
    }

    fn point_set0_at(&self, set: vk::DescriptorSet, addresses: &VkBuffer) {
        let info = [vk::DescriptorBufferInfo::default()
            .buffer(addresses.raw)
            .offset(0)
            .range(vk::WHOLE_SIZE)];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&info);
        // SAFETY: the set is new and unused; the buffer is live.
        unsafe { self.device.update_descriptor_sets(&[write], &[]) };
    }

    fn destroy(&mut self) {
        // SAFETY: the objects were created from this device and are no longer in use.
        unsafe {
            if self.pool != vk::DescriptorPool::null() {
                self.device.destroy_descriptor_pool(self.pool, None);
            }
            if self.pipeline_layout != vk::PipelineLayout::null() {
                self.device
                    .destroy_pipeline_layout(self.pipeline_layout, None);
            }
            for layout in self.layouts.drain(..) {
                self.device.destroy_descriptor_set_layout(layout, None);
            }
            for sampler in self.samplers.drain(..) {
                self.device.destroy_sampler(sampler, None);
            }
        }
    }
}

fn table_flags() -> vk::DescriptorBindingFlags {
    vk::DescriptorBindingFlags::PARTIALLY_BOUND
        | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND
        | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING
}

fn create_address_table(ctx: &mut Ctx<'_>) -> Result<VkBuffer, RhiError> {
    use concordia_rhi::{BufferDesc, BufferUsage, MemoryLocation};
    let desc = BufferDesc {
        size: std::num::NonZeroU64::new(u64::from(BINDLESS_CAPACITY) * 8)
            .unwrap_or(std::num::NonZeroU64::MIN),
        usage: BufferUsage::STORAGE,
        memory: MemoryLocation::Upload,
        label: "bindless buffer addresses".into(),
    };
    let mut table = resources::create_buffer(ctx, &desc)?;
    table.mapped()?.fill(0);
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_dense_reused_and_bounded() {
        let mut slots = Indices::default();
        assert_eq!((slots.take(), slots.take()), (Some(0), Some(1)));
        slots.give_back(0);
        assert_eq!(slots.take(), Some(0));
        for _ in 2..BINDLESS_CAPACITY {
            assert!(slots.take().is_some());
        }
        assert_eq!(slots.take(), None, "all {BINDLESS_CAPACITY} taken");
    }
}
