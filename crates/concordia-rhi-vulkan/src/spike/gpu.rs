//! A small Vulkan device for D4's spike, with the extensions the backend does not enable yet
//! (`VK_EXT_mesh_shader`; `VK_KHR_acceleration_structure` and `VK_KHR_ray_query`). It reuses the
//! backend's instance and validation log. References: the Vulkan 1.3 specification (mesh shading,
//! acceleration structures, ray queries) and the Khronos Vulkan-Samples' `mesh_shading` and
//! `ray_queries` samples (Apache-2.0), read for call order only.

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char};

use ash::vk;
use concordia_testkit::vulkan_required;

use crate::instance::Instance;
use crate::{Validation, ValidationLog};

/// The mesh spike's image is `SIZE`×`SIZE` RGBA8.
pub(super) const SIZE: u32 = 32;

/// What a spike case needs from the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Want {
    /// `VK_EXT_mesh_shader`.
    Mesh,
    /// `VK_KHR_ray_query` and the acceleration structures it traces.
    RayQuery,
}

impl Want {
    fn extensions(self) -> &'static [&'static CStr] {
        match self {
            Self::Mesh => &[ash::ext::mesh_shader::NAME],
            Self::RayQuery => &[
                ash::khr::acceleration_structure::NAME,
                ash::khr::ray_query::NAME,
                ash::khr::deferred_host_operations::NAME,
            ],
        }
    }
}

/// A host-visible buffer with its device address, mapped for its lifetime.
struct Buffer {
    raw: vk::Buffer,
    ptr: *mut u8,
    address: vk::DeviceAddress,
}

/// An object destroyed with the device, newest first.
enum Dead {
    Buffer(vk::Buffer, vk::DeviceMemory),
    Image(vk::Image, vk::DeviceMemory),
    View(vk::ImageView),
    Module(vk::ShaderModule),
    Pipeline(vk::Pipeline),
    Layout(vk::PipelineLayout),
    SetLayout(vk::DescriptorSetLayout),
    Pool(vk::DescriptorPool),
    Accel(vk::AccelerationStructureKHR),
}

/// The spike's device: one graphics-and-compute queue, under validation when available.
pub(super) struct Gpu {
    log: ValidationLog,
    physical: vk::PhysicalDevice,
    device: ash::Device,
    queue: vk::Queue,
    pool: vk::CommandPool,
    memory: vk::PhysicalDeviceMemoryProperties,
    task: bool,
    mesh: Option<ash::ext::mesh_shader::Device>,
    accel: Option<ash::khr::acceleration_structure::Device>,
    dead: RefCell<Vec<Dead>>,
    /// Dropped last: the messenger reports until the device is gone.
    instance: Instance,
}

/// `None` without Vulkan or the extensions, unless `CONCORDIA_REQUIRE_VULKAN=1` (then a panic).
fn unavailable<T>(reason: impl std::fmt::Display) -> Option<T> {
    assert!(
        !vulkan_required(),
        "CONCORDIA_REQUIRE_VULKAN=1 but D4's spike cannot run: {reason}"
    );
    None
}

impl Gpu {
    /// A device for `want`, or `None` (see [`unavailable`]).
    pub fn open(want: Want) -> Option<Self> {
        let log = ValidationLog::new();
        let validation = match vulkan_required() {
            true => Validation::Required,
            false => Validation::IfAvailable,
        };
        let instance = match Instance::new(validation, log.clone()) {
            Ok(instance) => instance,
            Err(e) => return unavailable(e),
        };
        let candidates = match instance.candidates() {
            Ok(c) => c,
            Err(e) => return unavailable(e),
        };
        let Some(candidate) = candidates
            .into_iter()
            .find(|c| c.record.graphics_compute_queue && c.record.api_version >= [1, 3, 0])
        else {
            return unavailable("no Vulkan 1.3 device with a graphics-and-compute queue");
        };
        let missing = missing_extensions(&instance, candidate.physical, want.extensions());
        if !missing.is_empty() {
            return unavailable(format!(
                "{} lacks {missing:?}",
                candidate.record.device_name
            ));
        }
        let task = match want {
            Want::Mesh => match mesh_features(&instance, candidate.physical) {
                (true, task) => task,
                (false, _) => return unavailable("VK_EXT_mesh_shader without meshShader"),
            },
            Want::RayQuery => match ray_query_features(&instance, candidate.physical) {
                true => false,
                false => return unavailable("no accelerationStructure or rayQuery feature"),
            },
        };
        let device = create_device(&instance, candidate.physical, candidate.family, want, task)?;
        // SAFETY: the device is live and has the queue family `family` with one queue.
        let queue = unsafe { device.get_device_queue(candidate.family, 0) };
        let info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(candidate.family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        // SAFETY: the device is live; `info` is valid for the call.
        let pool = unsafe { device.create_command_pool(&info, None) }.expect("command pool");
        // SAFETY: the physical device came from this instance.
        let memory = unsafe {
            instance
                .raw
                .get_physical_device_memory_properties(candidate.physical)
        };
        let mesh = (want == Want::Mesh)
            .then(|| ash::ext::mesh_shader::Device::new(&instance.raw, &device));
        let accel = (want == Want::RayQuery)
            .then(|| ash::khr::acceleration_structure::Device::new(&instance.raw, &device));
        Some(Self {
            log,
            physical: candidate.physical,
            device,
            queue,
            pool,
            memory,
            task,
            mesh,
            accel,
            dead: RefCell::new(Vec::new()),
            instance,
        })
    }

    /// True when the device has `taskShader` (amplification shaders).
    pub fn task_shaders(&self) -> bool {
        self.task
    }

    /// Destroys the device and fails on any validation error reported meanwhile.
    pub fn assert_clean(self) {
        let log = self.log.clone();
        let validating = self.instance.validating();
        drop(self);
        let errors = log.take();
        assert!(
            errors.is_empty(),
            "validation errors:\n{}",
            errors.join("\n")
        );
        if !validating {
            eprintln!("D4 spike: ran without the validation layer (not installed)");
        }
    }

    /// Draws one mesh-shader triangle (launched by `task` if given) and reads the image back.
    pub fn draw_mesh(
        &self,
        task: Option<(&[u32], &str)>,
        ms: &[u32],
        ms_entry: &str,
        fs: &[u32],
        fs_entry: &str,
    ) -> Vec<u8> {
        let format = vk::Format::R8G8B8A8_UNORM;
        let (image, view) = self.target(format);
        let readback = self.buffer(
            u64::from(SIZE * SIZE * 4),
            vk::BufferUsageFlags::TRANSFER_DST,
        );
        let mut stages = Vec::new();
        if let Some((words, entry)) = task {
            stages.push((vk::ShaderStageFlags::TASK_EXT, words, entry));
        }
        stages.push((vk::ShaderStageFlags::MESH_EXT, ms, ms_entry));
        stages.push((vk::ShaderStageFlags::FRAGMENT, fs, fs_entry));
        let layout = self.layout(&[]);
        let pipeline = self.mesh_pipeline(&stages, layout, format);
        let Some(mesh) = &self.mesh else {
            panic!("the device was opened without VK_EXT_mesh_shader");
        };
        self.run(|cb| {
            self.image_barrier(
                cb,
                image,
                (
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                ),
                (vk::PipelineStageFlags2::NONE, vk::AccessFlags2::NONE),
                (
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                ),
            );
            let attachments = [vk::RenderingAttachmentInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue::default())];
            let rendering = vk::RenderingInfo::default()
                .render_area(vk::Rect2D::default().extent(extent()))
                .layer_count(1)
                .color_attachments(&attachments);
            // SAFETY: `cb` is recording; the pipeline, view and image are live, and the image is
            // in COLOR_ATTACHMENT_OPTIMAL for the pass.
            unsafe {
                self.device.cmd_begin_rendering(cb, &rendering);
                self.device
                    .cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, pipeline);
                mesh.cmd_draw_mesh_tasks(cb, 1, 1, 1);
                self.device.cmd_end_rendering(cb);
            }
            self.image_barrier(
                cb,
                image,
                (
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                ),
                (
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                ),
                (
                    vk::PipelineStageFlags2::COPY,
                    vk::AccessFlags2::TRANSFER_READ,
                ),
            );
            let copy = vk::BufferImageCopy::default()
                .image_subresource(color_layers())
                .image_extent(extent().into());
            // SAFETY: `cb` is recording; the image is in TRANSFER_SRC_OPTIMAL and the buffer holds
            // SIZE×SIZE RGBA8 texels.
            unsafe {
                self.device.cmd_copy_image_to_buffer(
                    cb,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    readback.raw,
                    &[copy],
                );
            }
            self.to_host(
                cb,
                vk::PipelineStageFlags2::COPY,
                vk::AccessFlags2::TRANSFER_WRITE,
            );
        });
        // SAFETY: the buffer is mapped, coherent and SIZE×SIZE×4 bytes; the GPU has finished.
        unsafe { std::slice::from_raw_parts(readback.ptr, (SIZE * SIZE * 4) as usize) }.to_vec()
    }

    /// Builds a one-triangle BLAS and TLAS, runs `cs` (4 threads) against them, and reads back
    /// its 8 words.
    pub fn trace(&self, cs: &[u32], entry: &str) -> Vec<u32> {
        let Some(accel) = &self.accel else {
            panic!("the device was opened without VK_KHR_acceleration_structure");
        };
        let input = vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR;
        let vertices = self.buffer(36, input);
        let triangle: [f32; 9] = [-1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0];
        // SAFETY: the buffer is mapped, coherent and 36 bytes long.
        unsafe { std::ptr::copy_nonoverlapping(triangle.as_ptr().cast(), vertices.ptr, 36) };
        let triangles = vk::AccelerationStructureGeometryTrianglesDataKHR::default()
            .vertex_format(vk::Format::R32G32B32_SFLOAT)
            .vertex_data(vk::DeviceOrHostAddressConstKHR {
                device_address: vertices.address,
            })
            .vertex_stride(12)
            .max_vertex(2)
            .index_type(vk::IndexType::NONE_KHR);
        let bottom = [vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::TRIANGLES)
            .geometry(vk::AccelerationStructureGeometryDataKHR { triangles })
            .flags(vk::GeometryFlagsKHR::OPAQUE)];
        let blas =
            self.acceleration_structure(vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL, &bottom);
        let info = vk::AccelerationStructureDeviceAddressInfoKHR::default()
            .acceleration_structure(blas.raw);
        // SAFETY: the BLAS is live.
        let blas_address = unsafe { accel.get_acceleration_structure_device_address(&info) };
        let instances = self.buffer(
            std::mem::size_of::<vk::AccelerationStructureInstanceKHR>() as u64,
            input,
        );
        let instance = vk::AccelerationStructureInstanceKHR {
            transform: vk::TransformMatrixKHR {
                matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            },
            instance_custom_index_and_mask: vk::Packed24_8::new(0, 0xff),
            instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                0,
                vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw() as u8,
            ),
            acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                device_handle: blas_address,
            },
        };
        // SAFETY: the buffer is mapped, coherent, sized for one instance and suitably aligned
        // (mappings are at least 64-byte aligned).
        unsafe {
            instances
                .ptr
                .cast::<vk::AccelerationStructureInstanceKHR>()
                .write(instance)
        };
        let top = [vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::INSTANCES)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                instances: vk::AccelerationStructureGeometryInstancesDataKHR::default().data(
                    vk::DeviceOrHostAddressConstKHR {
                        device_address: instances.address,
                    },
                ),
            })];
        let tlas = self.acceleration_structure(vk::AccelerationStructureTypeKHR::TOP_LEVEL, &top);
        let out = self.buffer(32, vk::BufferUsageFlags::STORAGE_BUFFER);
        let (pipeline, layout, set) = self.ray_query_pipeline(cs, entry, tlas.raw, out.raw);
        let built = (
            vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR,
            vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR,
        );
        self.run(|cb| {
            self.build(cb, accel, &blas, &bottom);
            self.memory_barrier(
                cb,
                built,
                (
                    vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR,
                    vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR,
                ),
            );
            self.build(cb, accel, &tlas, &top);
            self.memory_barrier(
                cb,
                built,
                (
                    vk::PipelineStageFlags2::COMPUTE_SHADER,
                    vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR,
                ),
            );
            // SAFETY: `cb` is recording; the pipeline, layout and set are live, and the set's
            // TLAS was built above.
            unsafe {
                self.device
                    .cmd_bind_pipeline(cb, vk::PipelineBindPoint::COMPUTE, pipeline);
                self.device.cmd_bind_descriptor_sets(
                    cb,
                    vk::PipelineBindPoint::COMPUTE,
                    layout,
                    0,
                    &[set],
                    &[],
                );
                self.device.cmd_dispatch(cb, 1, 1, 1);
            }
            self.to_host(
                cb,
                vk::PipelineStageFlags2::COMPUTE_SHADER,
                vk::AccessFlags2::SHADER_STORAGE_WRITE,
            );
        });
        // SAFETY: the buffer is mapped, coherent and 32 bytes; the GPU has finished.
        unsafe { std::slice::from_raw_parts(out.ptr.cast::<u32>(), 8) }.to_vec()
    }

    /// Records, submits and waits for one command buffer.
    fn run(&self, record: impl FnOnce(vk::CommandBuffer)) {
        let info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: the pool is live and used only from this thread.
        let cb = unsafe { self.device.allocate_command_buffers(&info) }.expect("command buffer")[0];
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: `cb` was just allocated and is not recording.
        unsafe { self.device.begin_command_buffer(cb, &begin) }.expect("begin");
        record(cb);
        let cbs = [cb];
        let submit = vk::SubmitInfo::default().command_buffers(&cbs);
        // SAFETY: `cb` is recording and ends here; the queue is used only from this thread; the
        // fence is waited on and destroyed before the command buffer is freed.
        unsafe {
            self.device.end_command_buffer(cb).expect("end");
            let fence = self
                .device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .expect("fence");
            self.device
                .queue_submit(self.queue, &[submit], fence)
                .expect("submit");
            self.device
                .wait_for_fences(&[fence], true, u64::MAX)
                .expect("wait");
            self.device.destroy_fence(fence, None);
            self.device.free_command_buffers(self.pool, &cbs);
        }
    }

    /// A mapped, coherent buffer with a device address (`usage` plus shader device address).
    fn buffer(&self, size: u64, usage: vk::BufferUsageFlags) -> Buffer {
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS);
        // SAFETY: the device is live; `info` is valid for the call.
        let raw = unsafe { self.device.create_buffer(&info, None) }.expect("buffer");
        // SAFETY: `raw` is live.
        let needs = unsafe { self.device.get_buffer_memory_requirements(raw) };
        let memory = self.allocate(
            needs,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            true,
        );
        self.dead.borrow_mut().push(Dead::Buffer(raw, memory));
        // SAFETY: `memory` fits `raw` (its requirements) and is bound once; it is host visible.
        let ptr = unsafe {
            self.device
                .bind_buffer_memory(raw, memory, 0)
                .expect("bind buffer");
            self.device
                .map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
                .expect("map")
        };
        let info = vk::BufferDeviceAddressInfo::default().buffer(raw);
        // SAFETY: `raw` is bound to memory allocated with DEVICE_ADDRESS.
        let address = unsafe { self.device.get_buffer_device_address(&info) };
        Buffer {
            raw,
            ptr: ptr.cast(),
            address,
        }
    }

    fn allocate(
        &self,
        needs: vk::MemoryRequirements,
        flags: vk::MemoryPropertyFlags,
        address: bool,
    ) -> vk::DeviceMemory {
        let index = (0..self.memory.memory_type_count)
            .find(|&i| {
                needs.memory_type_bits & (1 << i) != 0
                    && self.memory.memory_types[i as usize]
                        .property_flags
                        .contains(flags)
            })
            .expect("a memory type for the spike");
        let mut addressed =
            vk::MemoryAllocateFlagsInfo::default().flags(vk::MemoryAllocateFlags::DEVICE_ADDRESS);
        let mut info = vk::MemoryAllocateInfo::default()
            .allocation_size(needs.size)
            .memory_type_index(index);
        if address {
            info = info.push_next(&mut addressed);
        }
        // SAFETY: the device is live; `info` and its chain are valid for the call.
        unsafe { self.device.allocate_memory(&info, None) }.expect("memory")
    }

    /// The colour target and its view.
    fn target(&self, format: vk::Format) -> (vk::Image, vk::ImageView) {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(extent().into())
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC);
        // SAFETY: the device is live; `info` is valid for the call.
        let image = unsafe { self.device.create_image(&info, None) }.expect("image");
        // SAFETY: `image` is live.
        let needs = unsafe { self.device.get_image_memory_requirements(image) };
        let memory = self.allocate(needs, vk::MemoryPropertyFlags::empty(), false);
        self.dead.borrow_mut().push(Dead::Image(image, memory));
        // SAFETY: `memory` fits `image` and is bound once.
        unsafe { self.device.bind_image_memory(image, memory, 0) }.expect("bind image");
        let info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(color_range());
        // SAFETY: `image` is live and bound.
        let view = unsafe { self.device.create_image_view(&info, None) }.expect("view");
        self.dead.borrow_mut().push(Dead::View(view));
        (image, view)
    }

    fn module(&self, words: &[u32]) -> vk::ShaderModule {
        let info = vk::ShaderModuleCreateInfo::default().code(words);
        // SAFETY: the words passed the backend's SPIR-V checks.
        let module = unsafe { self.device.create_shader_module(&info, None) }.expect("module");
        self.dead.borrow_mut().push(Dead::Module(module));
        module
    }

    fn layout(&self, sets: &[vk::DescriptorSetLayout]) -> vk::PipelineLayout {
        let info = vk::PipelineLayoutCreateInfo::default().set_layouts(sets);
        // SAFETY: the device and set layouts are live.
        let layout = unsafe { self.device.create_pipeline_layout(&info, None) }.expect("layout");
        self.dead.borrow_mut().push(Dead::Layout(layout));
        layout
    }

    /// A graphics pipeline of task/mesh/fragment `stages` (no vertex input) for `format`.
    fn mesh_pipeline(
        &self,
        stages: &[(vk::ShaderStageFlags, &[u32], &str)],
        layout: vk::PipelineLayout,
        format: vk::Format,
    ) -> vk::Pipeline {
        let names: Vec<CString> = stages
            .iter()
            .map(|(_, _, entry)| CString::new(*entry).expect("entry name"))
            .collect();
        let infos: Vec<vk::PipelineShaderStageCreateInfo<'_>> = stages
            .iter()
            .zip(&names)
            .map(|((stage, words, _), name)| {
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(*stage)
                    .module(self.module(words))
                    .name(name)
            })
            .collect();
        let viewports = [vk::Viewport::default()
            .width(SIZE as f32)
            .height(SIZE as f32)
            .max_depth(1.0)];
        let scissors = [vk::Rect2D::default().extent(extent())];
        let viewport = vk::PipelineViewportStateCreateInfo::default()
            .viewports(&viewports)
            .scissors(&scissors);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let attachments = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
        let formats = [format];
        let mut rendering =
            vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats);
        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&infos)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&multisample)
            .color_blend_state(&blend)
            .layout(layout)
            .push_next(&mut rendering);
        // SAFETY: every state struct and name lives for the call; mesh pipelines take no vertex
        // input or input assembly state.
        let pipeline = unsafe {
            self.device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
        }
        .map_err(|(_, e)| e)
        .expect("mesh pipeline")[0];
        self.dead.borrow_mut().push(Dead::Pipeline(pipeline));
        pipeline
    }

    /// The ray-query compute pipeline, its layout, and a set holding `tlas` and `out`.
    fn ray_query_pipeline(
        &self,
        cs: &[u32],
        entry: &str,
        tlas: vk::AccelerationStructureKHR,
        out: vk::Buffer,
    ) -> (vk::Pipeline, vk::PipelineLayout, vk::DescriptorSet) {
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        // SAFETY: the device is live; `info` is valid for the call.
        let set_layout =
            unsafe { self.device.create_descriptor_set_layout(&info, None) }.expect("set layout");
        self.dead.borrow_mut().push(Dead::SetLayout(set_layout));
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1),
        ];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&sizes);
        // SAFETY: the device is live; `info` is valid for the call.
        let pool = unsafe { self.device.create_descriptor_pool(&info, None) }.expect("pool");
        self.dead.borrow_mut().push(Dead::Pool(pool));
        let layouts = [set_layout];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        // SAFETY: the pool has room for this one set.
        let set = unsafe { self.device.allocate_descriptor_sets(&info) }.expect("set")[0];
        let tlases = [tlas];
        let mut accel = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&tlases);
        let buffers = [vk::DescriptorBufferInfo::default()
            .buffer(out)
            .range(vk::WHOLE_SIZE)];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .push_next(&mut accel),
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&buffers),
        ];
        // SAFETY: the set, TLAS and buffer are live and the set is not in use.
        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
        let layout = self.layout(&layouts);
        let name = CString::new(entry).expect("entry name");
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(self.module(cs))
            .name(&name);
        let info = vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(layout);
        // SAFETY: the stage, module and layout are live for the call.
        let pipeline = unsafe {
            self.device
                .create_compute_pipelines(vk::PipelineCache::null(), &[info], None)
        }
        .map_err(|(_, e)| e)
        .expect("ray-query pipeline")[0];
        self.dead.borrow_mut().push(Dead::Pipeline(pipeline));
        (pipeline, layout, set)
    }

    /// An acceleration structure sized for `geometry` (one primitive), with its scratch.
    fn acceleration_structure(
        &self,
        ty: vk::AccelerationStructureTypeKHR,
        geometry: &[vk::AccelerationStructureGeometryKHR<'_>],
    ) -> Built {
        let Some(accel) = &self.accel else {
            panic!("the device was opened without VK_KHR_acceleration_structure");
        };
        let info = build_info(ty, geometry);
        let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
        // SAFETY: `info` describes valid geometry; one primitive per geometry.
        unsafe {
            accel.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &info,
                &[1],
                &mut sizes,
            );
        }
        let storage = self.buffer(
            sizes.acceleration_structure_size,
            vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR,
        );
        let create = vk::AccelerationStructureCreateInfoKHR::default()
            .buffer(storage.raw)
            .size(sizes.acceleration_structure_size)
            .ty(ty);
        // SAFETY: the storage buffer is live, bound and large enough.
        let raw = unsafe { accel.create_acceleration_structure(&create, None) }.expect("AS");
        self.dead.borrow_mut().push(Dead::Accel(raw));
        let align = self.scratch_alignment();
        let scratch = self.buffer(
            sizes.build_scratch_size + align,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        );
        Built {
            raw,
            scratch: scratch.address.next_multiple_of(align),
        }
    }

    /// Records the build of `built` from `geometry`.
    fn build(
        &self,
        cb: vk::CommandBuffer,
        accel: &ash::khr::acceleration_structure::Device,
        built: &Built,
        geometry: &[vk::AccelerationStructureGeometryKHR<'_>],
    ) {
        let info = build_info(built_type(geometry), geometry)
            .dst_acceleration_structure(built.raw)
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: built.scratch,
            });
        let ranges = [vk::AccelerationStructureBuildRangeInfoKHR::default().primitive_count(1)];
        // SAFETY: `cb` is recording; the destination, scratch and inputs are live.
        unsafe { accel.cmd_build_acceleration_structures(cb, &[info], &[&ranges]) };
    }

    fn scratch_alignment(&self) -> u64 {
        let mut accel = vk::PhysicalDeviceAccelerationStructurePropertiesKHR::default();
        {
            let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut accel);
            // SAFETY: the physical device came from this instance; the chain is writable.
            unsafe {
                self.instance
                    .raw
                    .get_physical_device_properties2(self.physical, &mut props)
            };
        }
        u64::from(accel.min_acceleration_structure_scratch_offset_alignment).max(1)
    }

    fn memory_barrier(
        &self,
        cb: vk::CommandBuffer,
        (src_stage, src_access): (vk::PipelineStageFlags2, vk::AccessFlags2),
        (dst_stage, dst_access): (vk::PipelineStageFlags2, vk::AccessFlags2),
    ) {
        let barriers = [vk::MemoryBarrier2::default()
            .src_stage_mask(src_stage)
            .src_access_mask(src_access)
            .dst_stage_mask(dst_stage)
            .dst_access_mask(dst_access)];
        let dependency = vk::DependencyInfo::default().memory_barriers(&barriers);
        // SAFETY: `cb` is recording; synchronization2 is enabled.
        unsafe { self.device.cmd_pipeline_barrier2(cb, &dependency) };
    }

    /// Makes `stage`'s `access` writes visible to the host.
    fn to_host(
        &self,
        cb: vk::CommandBuffer,
        stage: vk::PipelineStageFlags2,
        access: vk::AccessFlags2,
    ) {
        self.memory_barrier(
            cb,
            (stage, access),
            (vk::PipelineStageFlags2::HOST, vk::AccessFlags2::HOST_READ),
        );
    }

    fn image_barrier(
        &self,
        cb: vk::CommandBuffer,
        image: vk::Image,
        (old, new): (vk::ImageLayout, vk::ImageLayout),
        (src_stage, src_access): (vk::PipelineStageFlags2, vk::AccessFlags2),
        (dst_stage, dst_access): (vk::PipelineStageFlags2, vk::AccessFlags2),
    ) {
        let barriers = [vk::ImageMemoryBarrier2::default()
            .image(image)
            .old_layout(old)
            .new_layout(new)
            .src_stage_mask(src_stage)
            .src_access_mask(src_access)
            .dst_stage_mask(dst_stage)
            .dst_access_mask(dst_access)
            .subresource_range(color_range())];
        let dependency = vk::DependencyInfo::default().image_memory_barriers(&barriers);
        // SAFETY: `cb` is recording; the image is live; synchronization2 is enabled.
        unsafe { self.device.cmd_pipeline_barrier2(cb, &dependency) };
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        // SAFETY: waiting for idle makes every object unused; each is destroyed once, newest
        // first (acceleration structures before their buffers), then the pool and the device.
        unsafe {
            let _ = self.device.device_wait_idle();
            for dead in self.dead.get_mut().drain(..).rev() {
                match dead {
                    Dead::Buffer(b, m) => {
                        self.device.destroy_buffer(b, None);
                        self.device.free_memory(m, None);
                    }
                    Dead::Image(i, m) => {
                        self.device.destroy_image(i, None);
                        self.device.free_memory(m, None);
                    }
                    Dead::View(v) => self.device.destroy_image_view(v, None),
                    Dead::Module(m) => self.device.destroy_shader_module(m, None),
                    Dead::Pipeline(p) => self.device.destroy_pipeline(p, None),
                    Dead::Layout(l) => self.device.destroy_pipeline_layout(l, None),
                    Dead::SetLayout(l) => self.device.destroy_descriptor_set_layout(l, None),
                    Dead::Pool(p) => self.device.destroy_descriptor_pool(p, None),
                    Dead::Accel(a) => {
                        if let Some(accel) = &self.accel {
                            accel.destroy_acceleration_structure(a, None);
                        }
                    }
                }
            }
            self.device.destroy_command_pool(self.pool, None);
            self.device.destroy_device(None);
        }
    }
}

/// An acceleration structure and its aligned scratch address.
struct Built {
    raw: vk::AccelerationStructureKHR,
    scratch: vk::DeviceAddress,
}

fn build_info<'a>(
    ty: vk::AccelerationStructureTypeKHR,
    geometry: &'a [vk::AccelerationStructureGeometryKHR<'a>],
) -> vk::AccelerationStructureBuildGeometryInfoKHR<'a> {
    vk::AccelerationStructureBuildGeometryInfoKHR::default()
        .ty(ty)
        .flags(vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_TRACE)
        .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
        .geometries(geometry)
}

/// Instances build a TLAS; triangles a BLAS.
fn built_type(
    geometry: &[vk::AccelerationStructureGeometryKHR<'_>],
) -> vk::AccelerationStructureTypeKHR {
    match geometry.first().map(|g| g.geometry_type) {
        Some(vk::GeometryTypeKHR::INSTANCES) => vk::AccelerationStructureTypeKHR::TOP_LEVEL,
        _ => vk::AccelerationStructureTypeKHR::BOTTOM_LEVEL,
    }
}

fn extent() -> vk::Extent2D {
    vk::Extent2D {
        width: SIZE,
        height: SIZE,
    }
}

fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}

fn color_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .layer_count(1)
}

/// The names in `wanted` that `physical` does not offer.
fn missing_extensions(
    instance: &Instance,
    physical: vk::PhysicalDevice,
    wanted: &[&CStr],
) -> Vec<String> {
    // SAFETY: the physical device came from this instance.
    let available =
        unsafe { instance.raw.enumerate_device_extension_properties(physical) }.unwrap_or_default();
    wanted
        .iter()
        .filter(|name| {
            !available
                .iter()
                .any(|e| e.extension_name_as_c_str() == Ok(**name))
        })
        .map(|name| name.to_string_lossy().into_owned())
        .collect()
}

/// (`meshShader`, `taskShader`).
fn mesh_features(instance: &Instance, physical: vk::PhysicalDevice) -> (bool, bool) {
    let mut mesh = vk::PhysicalDeviceMeshShaderFeaturesEXT::default();
    {
        let mut all = vk::PhysicalDeviceFeatures2::default().push_next(&mut mesh);
        // SAFETY: the physical device has VK_EXT_mesh_shader; the chain is writable.
        unsafe {
            instance
                .raw
                .get_physical_device_features2(physical, &mut all)
        };
    }
    (mesh.mesh_shader == vk::TRUE, mesh.task_shader == vk::TRUE)
}

/// `accelerationStructure` and `rayQuery`.
fn ray_query_features(instance: &Instance, physical: vk::PhysicalDevice) -> bool {
    let mut accel = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default();
    let mut query = vk::PhysicalDeviceRayQueryFeaturesKHR::default();
    {
        let mut all = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut accel)
            .push_next(&mut query);
        // SAFETY: the physical device has both extensions; the chain is writable.
        unsafe {
            instance
                .raw
                .get_physical_device_features2(physical, &mut all)
        };
    }
    accel.acceleration_structure == vk::TRUE && query.ray_query == vk::TRUE
}

fn create_device(
    instance: &Instance,
    physical: vk::PhysicalDevice,
    family: u32,
    want: Want,
    task: bool,
) -> Option<ash::Device> {
    let priorities = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(family)
        .queue_priorities(&priorities)];
    let names: Vec<*const c_char> = want.extensions().iter().map(|n| n.as_ptr()).collect();
    let mut v12 = vk::PhysicalDeviceVulkan12Features::default().buffer_device_address(true);
    let mut v13 = vk::PhysicalDeviceVulkan13Features::default()
        .dynamic_rendering(true)
        .synchronization2(true)
        .maintenance4(true);
    let mut mesh = vk::PhysicalDeviceMeshShaderFeaturesEXT::default()
        .mesh_shader(true)
        .task_shader(task);
    let mut accel =
        vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default().acceleration_structure(true);
    let mut query = vk::PhysicalDeviceRayQueryFeaturesKHR::default().ray_query(true);
    let mut info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queues)
        .enabled_extension_names(&names)
        .push_next(&mut v12)
        .push_next(&mut v13);
    info = match want {
        Want::Mesh => info.push_next(&mut mesh),
        Want::RayQuery => info.push_next(&mut accel).push_next(&mut query),
    };
    // SAFETY: the physical device came from this instance and offers every extension and
    // feature requested (checked above); `info` and its chain live for the call.
    match unsafe { instance.raw.create_device(physical, &info, None) } {
        Ok(device) => Some(device),
        Err(e) => unavailable(format!("vkCreateDevice for {want:?}: {e}")),
    }
}
