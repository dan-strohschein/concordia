//! `VulkanDevice`: the RHI `Device` on one universal queue. All mutable state is behind one Mutex;
//! finished submissions are retired lazily (on the next submit, wait or destroy).

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use ash::vk;
use concordia_rhi::{
    BindlessIndex, Buffer, BufferDesc, BufferUsage, ComputePipelineDesc, Device, GpuTimeCallback,
    GpuTiming, GraphicsPipelineDesc, Heap, HeapDesc, MeshPipelineDesc, Pipeline, RhiCaps, RhiError,
    SampleCount, Slots, Submit, Texture, TextureDesc, TextureUsage, Timeline, TimestampGranularity,
};
use gpu_allocator::vulkan::{Allocator, AllocatorCreateDesc};

use crate::bindless::Bindless;
use crate::convert::{self, error};
use crate::encode::{self, Target, stale};
use crate::instance::{Candidate, Instance};
use crate::pipeline::{self, VkPipeline};
use crate::record::{self, ProbeRecord};
use crate::resources::{self, Ctx, Dead, Placement, VkBuffer, VkHeap, VkTexture};
use crate::timing::Queries;
use crate::track::TexState;
use crate::validation::ValidationLog;
use crate::{Options, Validation};

/// A timeline and the highest value any submission will signal on it.
pub(crate) struct VkTimeline {
    raw: vk::Semaphore,
    signaled: u64,
}

/// Every live object, addressed by RHI handles.
#[derive(Default)]
pub(crate) struct Resources {
    buffers: Slots<VkBuffer>,
    textures: Slots<VkTexture>,
    pipelines: Slots<VkPipeline>,
    heaps: Slots<VkHeap>,
    timelines: Slots<VkTimeline>,
}

impl Resources {
    pub fn buffer(&self, h: Buffer) -> Result<&VkBuffer, RhiError> {
        self.buffers
            .get(h.index(), h.generation())
            .ok_or_else(|| stale("buffer", h.index(), h.generation()))
    }

    pub fn texture(&self, h: Texture) -> Result<&VkTexture, RhiError> {
        self.textures
            .get(h.index(), h.generation())
            .ok_or_else(|| stale("texture", h.index(), h.generation()))
    }

    pub fn pipeline(&self, h: Pipeline) -> Result<&VkPipeline, RhiError> {
        self.pipelines
            .get(h.index(), h.generation())
            .ok_or_else(|| stale("pipeline", h.index(), h.generation()))
    }

    fn buffer_mut(&mut self, h: Buffer) -> Result<&mut VkBuffer, RhiError> {
        self.buffers
            .get_mut(h.index(), h.generation())
            .ok_or_else(|| stale("buffer", h.index(), h.generation()))
    }

    fn timeline(&mut self, h: Timeline) -> Result<&mut VkTimeline, RhiError> {
        self.timelines
            .get_mut(h.index(), h.generation())
            .ok_or_else(|| stale("timeline", h.index(), h.generation()))
    }
}

/// A submission the GPU may still be running.
struct InFlight {
    value: u64,
    cb: vk::CommandBuffer,
    queries: Option<(vk::QueryPool, Queries)>,
    callback: Option<GpuTimeCallback>,
}

struct State {
    allocator: Option<Allocator>,
    bindless: Option<Bindless>,
    resources: Resources,
    command_pool: vk::CommandPool,
    spare: Vec<vk::CommandBuffer>,
    /// Submissions so far (the progress timeline's last signal).
    submitted: u64,
    in_flight: VecDeque<InFlight>,
    /// Objects freed once progress reaches their value.
    graveyard: Vec<(u64, Dead)>,
    /// New textures to move to their home layouts at the start of the next submission.
    inits: Vec<Texture>,
    heap_memory_types: Option<u32>,
    lost: bool,
}

/// A Vulkan 1.3 device (ADR-0088).
pub struct VulkanDevice {
    caps: RhiCaps,
    timestamp_period_ns: f32,
    timestamp_bits: u32,
    device: ash::Device,
    physical: vk::PhysicalDevice,
    /// Used only while `state` is locked (queues need external synchronization).
    queue: vk::Queue,
    /// Signalled to `State::submitted` by every submission.
    progress: vk::Semaphore,
    state: Mutex<State>,
    instance: Instance,
}

impl std::fmt::Debug for VulkanDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VulkanDevice")
            .field("caps", &self.caps)
            .finish_non_exhaustive()
    }
}

fn no_device(reason: impl Into<String>) -> RhiError {
    RhiError::NoDevice {
        reason: reason.into(),
    }
}

/// The chosen physical device and its record.
pub(crate) fn choose(instance: &Instance) -> Result<Candidate, RhiError> {
    let mut candidates = instance.candidates()?;
    let records: Vec<ProbeRecord> = candidates.iter().map(|c| c.record.clone()).collect();
    let index = record::choose(&records).map_err(no_device)?;
    Ok(candidates.swap_remove(index))
}

impl VulkanDevice {
    /// Opens the best Vulkan 1.3 device with `options`.
    pub fn new(options: &Options) -> Result<Self, RhiError> {
        let instance = Instance::new(options.validation, options.log.clone().unwrap_or_default())?;
        let chosen = choose(&instance)?;
        let caps = record::caps_from_record(&chosen.record).masked(&options.mask);
        if !caps.bindless {
            return Err(no_device(format!(
                "{} holds {} bindless textures; the backend needs {}",
                caps.device_name,
                caps.max_bindless_textures,
                record::BINDLESS_CAPACITY
            )));
        }
        let device = create_logical(&instance, &chosen)?;
        // SAFETY: the device was created with one queue in `chosen.family`.
        let queue = unsafe { device.get_device_queue(chosen.family, 0) };
        let mut made = Self {
            caps,
            timestamp_period_ns: chosen.record.timestamp_period_ns,
            timestamp_bits: chosen.record.queue_timestamp_valid_bits,
            device,
            physical: chosen.physical,
            queue,
            progress: vk::Semaphore::null(),
            state: Mutex::new(State {
                allocator: None,
                bindless: None,
                resources: Resources::default(),
                command_pool: vk::CommandPool::null(),
                spare: Vec::new(),
                submitted: 0,
                in_flight: VecDeque::new(),
                graveyard: Vec::new(),
                inits: Vec::new(),
                heap_memory_types: None,
                lost: false,
            }),
            instance,
        };
        made.init(chosen.family, chosen.record.features.sampler_anisotropy)?;
        Ok(made)
    }

    /// Creates the allocator, bindless tables, command pool and progress timeline (`Drop` undoes a
    /// partial init).
    fn init(&mut self, family: u32, anisotropy: bool) -> Result<(), RhiError> {
        let mut allocator = Allocator::new(&AllocatorCreateDesc {
            instance: self.instance.raw.clone(),
            device: self.device.clone(),
            physical_device: self.physical,
            debug_settings: Default::default(),
            buffer_device_address: true,
            allocation_sizes: Default::default(),
        })
        .map_err(|e| RhiError::OutOfMemory(format!("the GPU allocator: {e}")))?;
        let tables = Bindless::new(&self.device, &mut allocator, anisotropy);
        let state = self
            .state
            .get_mut()
            .map_err(|_| RhiError::DeviceLost("state poisoned".into()))?;
        state.allocator = Some(allocator);
        state.bindless = Some(tables?);
        let pool = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(family);
        // SAFETY: `pool` is valid; the family exists on this device.
        state.command_pool = unsafe { self.device.create_command_pool(&pool, None) }
            .map_err(|e| error("vkCreateCommandPool", e))?;
        self.progress = timeline_semaphore(&self.device, 0)?;
        Ok(())
    }

    /// True when the Khronos validation layer is on.
    pub fn validation_enabled(&self) -> bool {
        self.instance.validating()
    }

    /// The validation layer's error messages.
    pub fn validation_log(&self) -> &ValidationLog {
        &self.instance.log
    }

    /// Reports `message` as a validation error through the messenger (tests the log's wiring).
    #[doc(hidden)]
    pub fn report_validation_error_for_tests(&self, message: &str) {
        if let Ok(message) = std::ffi::CString::new(message) {
            self.instance.report(&message);
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, RhiError> {
        self.state
            .lock()
            .map_err(|_| RhiError::DeviceLost("the device's state was poisoned by a panic".into()))
    }

    /// Frees what finished submissions held and reports their timing.
    fn retire(&self) {
        let mut reports = Vec::new();
        if let Ok(mut state) = self.state.lock() {
            // SAFETY: the progress semaphore is live.
            match unsafe { self.device.get_semaphore_counter_value(self.progress) } {
                Ok(done) => self.retire_to(&mut state, done, &mut reports),
                Err(vk::Result::ERROR_DEVICE_LOST) => state.lost = true,
                Err(_) => {}
            }
        }
        for (callback, timing) in reports {
            callback(timing);
        }
    }

    fn retire_to(
        &self,
        state: &mut State,
        done: u64,
        reports: &mut Vec<(GpuTimeCallback, GpuTiming)>,
    ) {
        while state.in_flight.front().is_some_and(|f| f.value <= done) {
            let Some(finished) = state.in_flight.pop_front() else {
                break;
            };
            let timing = finished.queries.map(|(pool, queries)| {
                let timing = self.read_queries(pool, &queries);
                // SAFETY: the submission using the pool has completed.
                unsafe { self.device.destroy_query_pool(pool, None) };
                timing
            });
            // SAFETY: the command buffer's submission completed; its pool is locked with `state`.
            if unsafe {
                self.device
                    .reset_command_buffer(finished.cb, vk::CommandBufferResetFlags::empty())
            }
            .is_ok()
            {
                state.spare.push(finished.cb);
            }
            if let Some(callback) = finished.callback {
                reports.push((callback, timing.unwrap_or_default()));
            }
        }
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut state.graveyard)
            .into_iter()
            .partition(|(v, _)| *v <= done);
        state.graveyard = waiting;
        for (_, dead) in ready {
            self.free(state, dead);
        }
    }

    fn free(&self, state: &mut State, dead: Dead) {
        if let Some(tables) = state.bindless.as_mut() {
            match &dead {
                Dead::Buffer(b) => {
                    if let Some(slot) = b.bindless {
                        tables.write_address(slot, 0);
                        tables.buffer_slots.give_back(slot);
                    }
                }
                Dead::Texture(t) => {
                    if let Some(slot) = t.bindless {
                        tables.texture_slots.give_back(slot);
                    }
                }
                _ => {}
            }
        }
        if let Some(allocator) = state.allocator.as_mut() {
            resources::free(
                &mut Ctx {
                    device: &self.device,
                    allocator,
                },
                dead,
            );
        }
    }

    fn read_queries(&self, pool: vk::QueryPool, queries: &Queries) -> GpuTiming {
        let mut data = vec![[0u64; 2]; queries.count as usize];
        let flags = vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WITH_AVAILABILITY;
        // SAFETY: the pool holds `count` queries and the submission writing them has completed;
        // NOT_READY still fills the data, with availability words saying which are valid.
        let _ = unsafe {
            self.device
                .get_query_pool_results(pool, 0, &mut data, flags)
        };
        let ticks: Vec<Option<u64>> = data
            .iter()
            .map(|[t, ready]| (*ready != 0).then_some(*t))
            .collect();
        queries.timing(&ticks, self.timestamp_period_ns, self.timestamp_bits)
    }

    /// Queues `dead` to be freed once every submission so far completes.
    fn bury(&self, dead: Dead) {
        if let Ok(mut state) = self.state.lock() {
            let value = state.submitted;
            state.graveyard.push((value, dead));
        }
        self.retire();
    }

    fn supports(&self, desc: &TextureDesc) -> Result<(), RhiError> {
        if desc.samples != SampleCount::X1 && !self.caps.msaa_sample_counts.contains(&desc.samples)
        {
            return Err(RhiError::Unsupported(format!(
                "{}× MSAA on {}",
                desc.samples.get(),
                self.caps.device_name
            )));
        }
        // SAFETY: `physical` came from this instance.
        let props = unsafe {
            self.instance
                .raw
                .get_physical_device_format_properties(self.physical, convert::format(desc.format))
        };
        let needed = convert::format_features(desc.usage, desc.format);
        match props.optimal_tiling_features.contains(needed) {
            true => Ok(()),
            false => Err(RhiError::Unsupported(format!(
                "{:?} with {:?} on {} (needs {needed:?})",
                desc.format, desc.usage, self.caps.device_name
            ))),
        }
    }

    fn insert_texture(
        &self,
        state: &mut State,
        mut texture: VkTexture,
    ) -> Result<Texture, RhiError> {
        if resources::bindless_texture(texture.desc.usage)? {
            let tables = state
                .bindless
                .as_mut()
                .ok_or_else(|| RhiError::DeviceLost("no bindless tables".into()))?;
            let Some(slot) = tables.texture_slots.take() else {
                self.free(state, Dead::Texture(texture));
                return Err(RhiError::OutOfMemory(format!(
                    "bindless texture table ({} slots)",
                    record::BINDLESS_CAPACITY
                )));
            };
            let sampled = texture
                .desc
                .usage
                .contains(TextureUsage::SAMPLED)
                .then(|| (texture.view, convert::layout(texture.state.home)));
            let storage = texture
                .desc
                .usage
                .contains(TextureUsage::STORAGE)
                .then_some(texture.mip0);
            tables.write_texture(&self.device, slot, sampled, storage);
            texture.bindless = Some(slot);
        }
        let (index, generation) = state.resources.textures.insert(texture);
        let handle = Texture::from_raw(index, generation);
        state.inits.push(handle);
        Ok(handle)
    }

    fn heap_memory_types(&self, state: &mut State) -> u32 {
        *state.heap_memory_types.get_or_insert_with(|| {
            let probe = |format, samples, usage| {
                let desc = TextureDesc {
                    width: std::num::NonZeroU32::new(256).unwrap_or(std::num::NonZeroU32::MIN),
                    height: std::num::NonZeroU32::new(256).unwrap_or(std::num::NonZeroU32::MIN),
                    mip_levels: std::num::NonZeroU32::MIN,
                    samples,
                    format,
                    usage,
                    label: "heap probe".into(),
                };
                resources::requirements(&self.device, &desc).memory_type_bits
            };
            use concordia_rhi::Format;
            let rt = TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED;
            probe(
                Format::Rgba8Unorm,
                SampleCount::X1,
                rt | TextureUsage::STORAGE | TextureUsage::COPY_SRC,
            ) & probe(Format::Depth32Float, SampleCount::X1, rt)
                & probe(
                    Format::Rgba16Float,
                    SampleCount::X4,
                    TextureUsage::RENDER_TARGET,
                )
        })
    }

    fn submit_with(
        &self,
        submit: Submit<'_>,
        on_time: Option<GpuTimeCallback>,
    ) -> Result<(), RhiError> {
        submit.commands.validate()?;
        self.retire();
        let mut guard = self.lock()?;
        let state = &mut *guard;
        if state.lost {
            return Err(RhiError::DeviceLost(
                "an earlier submission lost the device".into(),
            ));
        }
        let waits = waits(state, submit.wait)?;
        let signals = signals(state, submit.signal)?;
        let queries = match (&on_time, self.caps.timestamp_granularity) {
            (Some(_), TimestampGranularity::None) | (None, _) => None,
            (Some(_), granularity) => Some(Queries::plan(
                &submit.commands.commands,
                granularity.times_passes(),
            )),
        };
        let pool = queries
            .as_ref()
            .map(|q| self.query_pool(q.count))
            .transpose()?;
        let cb = self.command_buffer(state)?;
        let recorded = self.record_and_submit(
            state,
            cb,
            pool.zip(queries.as_ref()),
            &submit,
            &waits,
            &signals,
        );
        let states = match recorded {
            Ok(states) => states,
            Err(e) => {
                // SAFETY: the command buffer never reached the queue; its pool is locked with `state`.
                unsafe {
                    let _ = self
                        .device
                        .reset_command_buffer(cb, vk::CommandBufferResetFlags::empty());
                    if let Some(pool) = pool {
                        self.device.destroy_query_pool(pool, None);
                    }
                }
                state.spare.push(cb);
                state.lost |= matches!(e, RhiError::DeviceLost(_));
                return Err(e);
            }
        };
        self.commit(state, states, submit.signal);
        let immediate = on_time.is_some() && pool.is_none();
        let (callback, now) = match immediate {
            true => (None, on_time),
            false => (on_time, None),
        };
        state.in_flight.push_back(InFlight {
            value: state.submitted,
            cb,
            queries: pool.zip(queries),
            callback,
        });
        drop(guard);
        if let Some(report) = now {
            report(GpuTiming::default());
        }
        Ok(())
    }

    fn record_and_submit(
        &self,
        state: &mut State,
        cb: vk::CommandBuffer,
        queries: Option<(vk::QueryPool, &Queries)>,
        submit: &Submit<'_>,
        waits: &[vk::SemaphoreSubmitInfo<'static>],
        signals: &[vk::SemaphoreSubmitInfo<'static>],
    ) -> Result<BTreeMap<Texture, TexState>, RhiError> {
        let tables = state
            .bindless
            .as_ref()
            .ok_or_else(|| RhiError::DeviceLost("no bindless tables".into()))?;
        let target = Target {
            device: &self.device,
            cb,
            layout: tables.pipeline_layout,
            sets: tables.sets,
            queries,
        };
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: `cb` is a reset, unused command buffer from the locked pool.
        unsafe { self.device.begin_command_buffer(cb, &begin) }
            .map_err(|e| error("vkBeginCommandBuffer", e))?;
        let recorded = encode::record(
            &target,
            &state.resources,
            &submit.commands.commands,
            &state.inits,
        );
        // SAFETY: `cb` is recording (ended even after an encoding error so it can be reset).
        let ended = unsafe { self.device.end_command_buffer(cb) };
        let states = recorded?;
        ended.map_err(|e| error("vkEndCommandBuffer", e))?;
        let value = state.submitted + 1;
        let mut signal_infos = signals.to_vec();
        signal_infos.push(semaphore_info(self.progress, value));
        let cbs = [vk::CommandBufferSubmitInfo::default().command_buffer(cb)];
        let info = vk::SubmitInfo2::default()
            .wait_semaphore_infos(waits)
            .command_buffer_infos(&cbs)
            .signal_semaphore_infos(&signal_infos);
        // SAFETY: the queue is used only under the state lock; every handle in `info` is live.
        unsafe {
            self.device
                .queue_submit2(self.queue, &[info], vk::Fence::null())
        }
        .map_err(|e| error("vkQueueSubmit2", e))?;
        state.submitted = value;
        Ok(states)
    }

    /// After a successful submission: texture states, pending inits and timeline signals.
    fn commit(
        &self,
        state: &mut State,
        states: BTreeMap<Texture, TexState>,
        signal: &[(Timeline, u64)],
    ) {
        for (handle, texture_state) in states {
            if let Some(t) = state
                .resources
                .textures
                .get_mut(handle.index(), handle.generation())
            {
                t.state = texture_state;
            }
        }
        state.inits.clear();
        for (timeline, value) in signal {
            if let Ok(t) = state.resources.timeline(*timeline) {
                t.signaled = t.signaled.max(*value);
            }
        }
    }

    fn command_buffer(&self, state: &mut State) -> Result<vk::CommandBuffer, RhiError> {
        if let Some(cb) = state.spare.pop() {
            return Ok(cb);
        }
        let info = vk::CommandBufferAllocateInfo::default()
            .command_pool(state.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: the pool is live and locked with `state`.
        let cbs = unsafe { self.device.allocate_command_buffers(&info) }
            .map_err(|e| error("vkAllocateCommandBuffers", e))?;
        cbs.first()
            .copied()
            .ok_or_else(|| RhiError::OutOfMemory("a command buffer".into()))
    }

    fn query_pool(&self, count: u32) -> Result<vk::QueryPool, RhiError> {
        let info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(count);
        // SAFETY: `info` is valid.
        unsafe { self.device.create_query_pool(&info, None) }
            .map_err(|e| error("vkCreateQueryPool", e))
    }

    fn wait_value(
        &self,
        semaphore: vk::Semaphore,
        value: u64,
        timeout: Duration,
    ) -> Result<bool, RhiError> {
        let semaphores = [semaphore];
        let values = [value];
        let info = vk::SemaphoreWaitInfo::default()
            .semaphores(&semaphores)
            .values(&values);
        let nanos = u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX);
        // SAFETY: the semaphore is a live timeline (waiting needs no external synchronization).
        match unsafe { self.device.wait_semaphores(&info, nanos) } {
            Ok(()) => Ok(true),
            Err(vk::Result::TIMEOUT) => Ok(false),
            Err(e) => Err(error("vkWaitSemaphores", e)),
        }
    }
}

fn semaphore_info(semaphore: vk::Semaphore, value: u64) -> vk::SemaphoreSubmitInfo<'static> {
    vk::SemaphoreSubmitInfo::default()
        .semaphore(semaphore)
        .value(value)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
}

fn waits(
    state: &mut State,
    wait: &[(Timeline, u64)],
) -> Result<Vec<vk::SemaphoreSubmitInfo<'static>>, RhiError> {
    wait.iter()
        .map(|(t, v)| Ok(semaphore_info(state.resources.timeline(*t)?.raw, *v)))
        .collect()
}

fn signals(
    state: &mut State,
    signal: &[(Timeline, u64)],
) -> Result<Vec<vk::SemaphoreSubmitInfo<'static>>, RhiError> {
    signal
        .iter()
        .map(|(t, v)| {
            let timeline = state.resources.timeline(*t)?;
            match *v > timeline.signaled {
                true => Ok(semaphore_info(timeline.raw, *v)),
                false => Err(RhiError::invalid(
                    "submit",
                    format!(
                        "timeline values only grow: signalling {v} after {}",
                        timeline.signaled
                    ),
                )),
            }
        })
        .collect()
}

fn timeline_semaphore(device: &ash::Device, initial: u64) -> Result<vk::Semaphore, RhiError> {
    let mut kind = vk::SemaphoreTypeCreateInfo::default()
        .semaphore_type(vk::SemaphoreType::TIMELINE)
        .initial_value(initial);
    let info = vk::SemaphoreCreateInfo::default().push_next(&mut kind);
    // SAFETY: `info` is valid; timeline semaphores are enabled on the device.
    unsafe { device.create_semaphore(&info, None) }.map_err(|e| error("vkCreateSemaphore", e))
}

fn create_logical(instance: &Instance, chosen: &Candidate) -> Result<ash::Device, RhiError> {
    let priorities = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(chosen.family)
        .queue_priorities(&priorities)];
    let mut v12 = vk::PhysicalDeviceVulkan12Features::default()
        .timeline_semaphore(true)
        .buffer_device_address(true)
        .descriptor_binding_partially_bound(true)
        .descriptor_binding_sampled_image_update_after_bind(true)
        .descriptor_binding_storage_image_update_after_bind(true)
        .descriptor_binding_update_unused_while_pending(true);
    let mut v13 = vk::PhysicalDeviceVulkan13Features::default()
        .dynamic_rendering(true)
        .synchronization2(true);
    let core = vk::PhysicalDeviceFeatures::default()
        .shader_storage_image_read_without_format(true)
        .shader_storage_image_write_without_format(true)
        .shader_int64(true)
        .sampler_anisotropy(chosen.record.features.sampler_anisotropy);
    let mut features = vk::PhysicalDeviceFeatures2::default()
        .features(core)
        .push_next(&mut v12)
        .push_next(&mut v13);
    let portability = chosen
        .record
        .extensions
        .iter()
        .any(|e| e == "VK_KHR_portability_subset");
    let extensions: Vec<*const std::ffi::c_char> = portability
        .then_some(c"VK_KHR_portability_subset".as_ptr())
        .into_iter()
        .collect();
    let info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queues)
        .enabled_extension_names(&extensions)
        .push_next(&mut features);
    // SAFETY: the physical device came from this instance and supports every enabled feature
    // (`record::choose` checked them); `info` and its chain are valid for the call.
    unsafe { instance.raw.create_device(chosen.physical, &info, None) }
        .map_err(|e| error("vkCreateDevice", e))
}

impl Device for VulkanDevice {
    fn caps(&self) -> &RhiCaps {
        &self.caps
    }

    fn allocated_bytes(&self) -> Option<u64> {
        let state = self.state.lock().ok()?;
        Some(
            state
                .allocator
                .as_ref()?
                .generate_report()
                .total_allocated_bytes,
        )
    }

    fn create_buffer(&self, desc: &BufferDesc) -> Result<Buffer, RhiError> {
        desc.validate()?;
        if desc.size.get() > self.caps.max_buffer_bytes {
            return Err(RhiError::invalid(
                "buffer desc",
                format!(
                    "{:?} is {} bytes; this device allows {}",
                    desc.label, desc.size, self.caps.max_buffer_bytes
                ),
            ));
        }
        let mut guard = self.lock()?;
        let state = &mut *guard;
        let allocator = state
            .allocator
            .as_mut()
            .ok_or_else(|| RhiError::DeviceLost("no allocator".into()))?;
        let mut buffer = resources::create_buffer(
            &mut Ctx {
                device: &self.device,
                allocator,
            },
            desc,
        )?;
        if desc.usage.contains(BufferUsage::BINDLESS) {
            let tables = state
                .bindless
                .as_mut()
                .ok_or_else(|| RhiError::DeviceLost("no bindless tables".into()))?;
            let Some(slot) = tables.buffer_slots.take() else {
                self.free(state, Dead::Buffer(buffer));
                return Err(RhiError::OutOfMemory(format!(
                    "bindless buffer table ({} slots)",
                    record::BINDLESS_CAPACITY
                )));
            };
            tables.write_address(slot, buffer.address);
            buffer.bindless = Some(slot);
        }
        let (index, generation) = state.resources.buffers.insert(buffer);
        Ok(Buffer::from_raw(index, generation))
    }

    fn create_texture(&self, desc: &TextureDesc) -> Result<Texture, RhiError> {
        desc.validate()?;
        resources::bindless_texture(desc.usage)?;
        self.supports(desc)?;
        let mut guard = self.lock()?;
        let state = &mut *guard;
        let allocator = state
            .allocator
            .as_mut()
            .ok_or_else(|| RhiError::DeviceLost("no allocator".into()))?;
        let texture = resources::create_texture(
            &mut Ctx {
                device: &self.device,
                allocator,
            },
            desc,
            Placement::Own,
        )?;
        self.insert_texture(state, texture)
    }

    fn create_heap(&self, desc: &HeapDesc) -> Result<Heap, RhiError> {
        let mut guard = self.lock()?;
        let state = &mut *guard;
        let types = self.heap_memory_types(state);
        let allocator = state
            .allocator
            .as_mut()
            .ok_or_else(|| RhiError::DeviceLost("no allocator".into()))?;
        let heap = resources::create_heap(
            &mut Ctx {
                device: &self.device,
                allocator,
            },
            desc.size.get(),
            &desc.label,
            types,
        )?;
        let (index, generation) = state.resources.heaps.insert(heap);
        Ok(Heap::from_raw(index, generation))
    }

    fn texture_heap_footprint(&self, desc: &TextureDesc) -> Result<(u64, u64), RhiError> {
        desc.validate()?;
        self.supports(desc)?;
        let req = resources::requirements(&self.device, desc);
        Ok((req.size, req.alignment))
    }

    fn create_placed_texture(
        &self,
        heap: Heap,
        offset: u64,
        desc: &TextureDesc,
    ) -> Result<Texture, RhiError> {
        desc.validate()?;
        resources::bindless_texture(desc.usage)?;
        self.supports(desc)?;
        let mut guard = self.lock()?;
        let state = &mut *guard;
        let State {
            allocator,
            resources: live,
            ..
        } = state;
        let heap_ref = live
            .heaps
            .get(heap.index(), heap.generation())
            .ok_or_else(|| stale("heap", heap.index(), heap.generation()))?;
        let allocator = allocator
            .as_mut()
            .ok_or_else(|| RhiError::DeviceLost("no allocator".into()))?;
        let texture = resources::create_texture(
            &mut Ctx {
                device: &self.device,
                allocator,
            },
            desc,
            Placement::Placed(heap_ref, offset),
        )?;
        self.insert_texture(state, texture)
    }

    fn create_compute_pipeline(&self, desc: &ComputePipelineDesc) -> Result<Pipeline, RhiError> {
        let mut state = self.lock()?;
        let layout = state
            .bindless
            .as_ref()
            .map(|b| b.pipeline_layout)
            .ok_or_else(|| RhiError::DeviceLost("no layout".into()))?;
        let made = pipeline::compute(&self.device, layout, desc)?;
        let (index, generation) = state.resources.pipelines.insert(made);
        Ok(Pipeline::from_raw(index, generation))
    }

    fn create_graphics_pipeline(&self, desc: &GraphicsPipelineDesc) -> Result<Pipeline, RhiError> {
        let mut state = self.lock()?;
        let layout = state
            .bindless
            .as_ref()
            .map(|b| b.pipeline_layout)
            .ok_or_else(|| RhiError::DeviceLost("no layout".into()))?;
        let made = pipeline::graphics(&self.device, layout, desc, &self.caps)?;
        let (index, generation) = state.resources.pipelines.insert(made);
        Ok(Pipeline::from_raw(index, generation))
    }

    fn create_mesh_pipeline(&self, desc: &MeshPipelineDesc) -> Result<Pipeline, RhiError> {
        Err(RhiError::Unsupported(format!(
            "{:?}: mesh pipelines arrive on Vulkan in M4 4.3b",
            desc.label
        )))
    }

    fn write_buffer(&self, buffer: Buffer, offset: u64, data: &[u8]) -> Result<(), RhiError> {
        let mut state = self.lock()?;
        let bytes = state.resources.buffer_mut(buffer)?.mapped()?;
        let range = byte_range(bytes.len(), offset, data.len() as u64)?;
        bytes[range].copy_from_slice(data);
        Ok(())
    }

    fn read_buffer(&self, buffer: Buffer, offset: u64, len: u64) -> Result<Vec<u8>, RhiError> {
        let mut state = self.lock()?;
        let bytes = state.resources.buffer_mut(buffer)?.mapped()?;
        let range = byte_range(bytes.len(), offset, len)?;
        Ok(bytes[range].to_vec())
    }

    fn buffer_index(&self, buffer: Buffer) -> Result<BindlessIndex, RhiError> {
        let state = self.lock()?;
        let b = state.resources.buffer(buffer)?;
        b.bindless.map(BindlessIndex).ok_or_else(|| {
            RhiError::invalid(
                "buffer",
                format!("{:?} was not created with BINDLESS", b.desc.label),
            )
        })
    }

    fn texture_index(&self, texture: Texture) -> Result<BindlessIndex, RhiError> {
        let state = self.lock()?;
        let t = state.resources.texture(texture)?;
        t.bindless.map(BindlessIndex).ok_or_else(|| {
            RhiError::invalid(
                "texture",
                format!("{:?} was not created with BINDLESS", t.desc.label),
            )
        })
    }

    fn create_timeline(&self, initial: u64) -> Result<Timeline, RhiError> {
        let raw = timeline_semaphore(&self.device, initial)?;
        let mut state = self.lock()?;
        let (index, generation) = state.resources.timelines.insert(VkTimeline {
            raw,
            signaled: initial,
        });
        Ok(Timeline::from_raw(index, generation))
    }

    fn timeline_value(&self, timeline: Timeline) -> Result<u64, RhiError> {
        let raw = self.lock()?.resources.timeline(timeline)?.raw;
        // SAFETY: the semaphore is a live timeline.
        let value = unsafe { self.device.get_semaphore_counter_value(raw) }
            .map_err(|e| error("vkGetSemaphoreCounterValue", e))?;
        self.retire();
        Ok(value)
    }

    fn wait_timeline(
        &self,
        timeline: Timeline,
        value: u64,
        timeout: Duration,
    ) -> Result<bool, RhiError> {
        let raw = self.lock()?.resources.timeline(timeline)?.raw;
        let reached = self.wait_value(raw, value, timeout)?;
        self.retire();
        Ok(reached)
    }

    fn submit(&self, submit: Submit<'_>) -> Result<(), RhiError> {
        self.submit_with(submit, None)
    }

    fn submit_timed(
        &self,
        submit: Submit<'_>,
        on_gpu_time: GpuTimeCallback,
    ) -> Result<(), RhiError> {
        self.submit_with(submit, Some(on_gpu_time))
    }

    fn wait_idle(&self) -> Result<(), RhiError> {
        let target = self.lock()?.submitted;
        self.wait_value(self.progress, target, Duration::MAX)?;
        self.retire();
        Ok(())
    }

    fn destroy_buffer(&self, buffer: Buffer) {
        let removed = self.state.lock().ok().and_then(|mut s| {
            s.resources
                .buffers
                .remove(buffer.index(), buffer.generation())
        });
        if let Some(b) = removed {
            self.bury(Dead::Buffer(b));
        }
    }

    fn destroy_texture(&self, texture: Texture) {
        let removed = self.state.lock().ok().and_then(|mut s| {
            s.inits.retain(|t| *t != texture);
            s.resources
                .textures
                .remove(texture.index(), texture.generation())
        });
        if let Some(t) = removed {
            self.bury(Dead::Texture(t));
        }
    }

    fn destroy_heap(&self, heap: Heap) {
        let removed = self
            .state
            .lock()
            .ok()
            .and_then(|mut s| s.resources.heaps.remove(heap.index(), heap.generation()));
        if let Some(h) = removed {
            self.bury(Dead::Heap(h));
        }
    }

    unsafe fn create_surface(
        &self,
        target: concordia_rhi::SurfaceTarget,
        _: concordia_rhi::Format,
    ) -> Result<concordia_rhi::Surface, RhiError> {
        Err(RhiError::Unsupported(format!(
            "{target:?}: Vulkan surfaces arrive in M4 4.1v(b)"
        )))
    }

    fn acquire(
        &self,
        _: concordia_rhi::Surface,
        _: u32,
        _: u32,
    ) -> Result<Option<Texture>, RhiError> {
        Err(RhiError::Unsupported(
            "Vulkan surfaces arrive in M4 4.1v(b)".into(),
        ))
    }

    fn present(
        &self,
        _: concordia_rhi::Surface,
        _: concordia_rhi::Present<'_>,
    ) -> Result<(), RhiError> {
        Err(RhiError::Unsupported(
            "Vulkan surfaces arrive in M4 4.1v(b)".into(),
        ))
    }

    fn destroy_surface(&self, _: concordia_rhi::Surface) {}
}

fn byte_range(available: usize, offset: u64, len: u64) -> Result<std::ops::Range<usize>, RhiError> {
    let end = offset
        .checked_add(len)
        .and_then(|e| usize::try_from(e).ok());
    match (usize::try_from(offset), end) {
        (Ok(start), Some(end)) if end <= available => Ok(start..end),
        _ => Err(RhiError::invalid(
            "buffer access",
            format!("{len} bytes at {offset} overflow a {available}-byte buffer"),
        )),
    }
}

impl Drop for VulkanDevice {
    fn drop(&mut self) {
        // SAFETY: waiting for the device to go idle before destroying anything it may use.
        unsafe {
            let _ = self.device.device_wait_idle();
        }
        self.retire();
        let state = match self.state.get_mut() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut dead: Vec<Dead> = std::mem::take(&mut state.graveyard)
            .into_iter()
            .map(|(_, d)| d)
            .collect();
        dead.extend(
            state
                .resources
                .buffers
                .drain()
                .into_iter()
                .map(Dead::Buffer),
        );
        dead.extend(
            state
                .resources
                .textures
                .drain()
                .into_iter()
                .map(Dead::Texture),
        );
        dead.extend(state.resources.heaps.drain().into_iter().map(Dead::Heap));
        dead.extend(
            state
                .resources
                .pipelines
                .drain()
                .into_iter()
                .map(|p| Dead::Pipeline(p.raw)),
        );
        dead.extend(
            state
                .resources
                .timelines
                .drain()
                .into_iter()
                .map(|t| Dead::Semaphore(t.raw)),
        );
        // SAFETY: the device is idle; every object below was created from it and is unused.
        unsafe {
            for finished in state.in_flight.drain(..) {
                if let Some((pool, _)) = finished.queries {
                    self.device.destroy_query_pool(pool, None);
                }
            }
            if let Some(mut allocator) = state.allocator.take() {
                let mut ctx = Ctx {
                    device: &self.device,
                    allocator: &mut allocator,
                };
                for d in dead {
                    resources::free(&mut ctx, d);
                }
                if let Some(tables) = state.bindless.take() {
                    tables.destroy(&self.device, &mut allocator);
                }
                drop(allocator);
            }
            if state.command_pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(state.command_pool, None);
            }
            if self.progress != vk::Semaphore::null() {
                self.device.destroy_semaphore(self.progress, None);
            }
            self.device.destroy_device(None);
        }
    }
}

/// The record of the device `new` would open, without opening it.
pub(crate) fn probe_record(validation: Validation) -> Result<ProbeRecord, RhiError> {
    let instance = Instance::new(validation, ValidationLog::default())?;
    Ok(choose(&instance)?.record)
}
