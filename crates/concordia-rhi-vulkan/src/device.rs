//! `VulkanDevice`: the RHI `Device` on one universal queue. All mutable state is behind one Mutex;
//! finished submissions are retired lazily (on the next submit, wait or destroy).

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use ash::vk;
use concordia_rhi::{
    BindlessIndex, Buffer, BufferDesc, BufferUsage, ComputePipelineDesc, Device, GpuTimeCallback,
    GpuTiming, GraphicsPipelineDesc, Heap, HeapDesc, Layout, MeshPipelineDesc, Pipeline,
    PresentedCallback, RhiCaps, RhiError, SampleCount, Slots, Submit, Surface, SurfaceTarget,
    Texture, TextureDesc, TextureUsage, Timeline, TimestampGranularity,
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
use crate::wsi::{self, Acquired, VkSurface, Wsi};
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
    /// Called when the frame presented with this submission retires.
    presented: Option<PresentedCallback>,
}

/// What a present adds to a submission: the acquired image's init, and its semaphores.
#[derive(Default)]
struct Extra {
    inits: Vec<Texture>,
    waits: Vec<vk::SemaphoreSubmitInfo<'static>>,
    signals: Vec<vk::SemaphoreSubmitInfo<'static>>,
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
    surfaces: Slots<VkSurface>,
    /// The texture every empty bindless slot shows.
    empty_slot: Option<VkTexture>,
    lost: bool,
}

/// A Vulkan 1.3 device (ADR-0088): the device state, shared with a retire thread that fires
/// timing and presentation callbacks as the GPU finishes (as Metal's completion handlers do).
pub struct VulkanDevice {
    inner: Arc<Inner>,
    retirer: Option<(Arc<AtomicBool>, JoinHandle<()>)>,
}

/// A [`VulkanDevice`]'s state (reached through `Deref`).
pub struct Inner {
    /// Wakes the retire thread when work is submitted.
    waker: OnceLock<std::thread::Thread>,
    caps: RhiCaps,
    timestamp_period_ns: f32,
    timestamp_bits: u32,
    device: ash::Device,
    physical: vk::PhysicalDevice,
    /// The universal queue's family.
    family: u32,
    /// Surface and swapchain functions (when the instance and device have the extensions).
    wsi: Option<Wsi>,
    /// `VK_KHR_swapchain_mutable_format` is enabled (sRGB views of UNORM swapchains).
    mutable_format: bool,
    /// `VK_EXT_mesh_shader`'s commands, when the (masked) caps have mesh shaders.
    mesh: Option<ash::ext::mesh_shader::Device>,
    /// Used only while `state` is locked (queues need external synchronization).
    queue: vk::Queue,
    /// Signalled to `State::submitted` by every submission.
    progress: vk::Semaphore,
    /// Fail calls once the validation log has errors (`Options::strict`).
    strict: bool,
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

impl std::ops::Deref for VulkanDevice {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.inner
    }
}

/// How often the retire thread looks at the progress timeline while work is in flight.
const RETIRE_BUSY: Duration = Duration::from_millis(1);
/// How long it sleeps with nothing in flight (a submission wakes it).
const RETIRE_IDLE: Duration = Duration::from_millis(50);

impl VulkanDevice {
    /// Opens the best Vulkan 1.3 device with `options`.
    pub fn new(options: &Options) -> Result<Self, RhiError> {
        let inner = Arc::new(Inner::new(options)?);
        let stop = Arc::new(AtomicBool::new(false));
        let (shared, stopping) = (inner.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("concordia-vulkan-retire".into())
            .spawn(move || retire_loop(&shared, &stopping))
            .map_err(|e| RhiError::invalid("retire thread", e.to_string()))?;
        let _ = inner.waker.set(thread.thread().clone());
        Ok(Self {
            inner,
            retirer: Some((stop, thread)),
        })
    }
}

/// Retires finished submissions until `stop`, polling the timeline under the state lock (`retire`)
/// and making no Vulkan call outside it.
fn retire_loop(inner: &Inner, stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) {
        let busy = inner.oldest_in_flight().is_some();
        std::thread::park_timeout(if busy { RETIRE_BUSY } else { RETIRE_IDLE });
        if busy {
            inner.retire();
        }
    }
}

impl Drop for VulkanDevice {
    fn drop(&mut self) {
        if let Some((stop, thread)) = self.retirer.take() {
            stop.store(true, Ordering::Release);
            thread.thread().unpark();
            let _ = thread.join();
        }
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

impl Inner {
    /// Opens the best Vulkan 1.3 device with `options`.
    fn new(options: &Options) -> Result<Self, RhiError> {
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
        let presents = can_present(&instance, &chosen);
        let mutable_format =
            presents && chosen.record.extensions.iter().any(|e| e == MUTABLE_FORMAT);
        let device = create_logical(&chosen, &instance, (presents, mutable_format), &caps)?;
        let mesh = caps
            .mesh_shaders
            .then(|| ash::ext::mesh_shader::Device::new(&instance.raw, &device));
        // SAFETY: the device was created with one queue in `chosen.family`.
        let queue = unsafe { device.get_device_queue(chosen.family, 0) };
        let wsi = presents.then(|| Wsi {
            surface: ash::khr::surface::Instance::new(&instance.entry, &instance.raw),
            swapchain: ash::khr::swapchain::Device::new(&instance.raw, &device),
        });
        let mut made = Self {
            waker: OnceLock::new(),
            caps,
            timestamp_period_ns: chosen.record.timestamp_period_ns,
            timestamp_bits: chosen.record.queue_timestamp_valid_bits,
            device,
            physical: chosen.physical,
            family: chosen.family,
            wsi,
            mutable_format,
            mesh,
            queue,
            progress: vk::Semaphore::null(),
            strict: options.strict,
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
                surfaces: Slots::default(),
                empty_slot: None,
                lost: false,
            }),
            instance,
        };
        made.init(chosen.family, chosen.record.features.sampler_anisotropy)?;
        Ok(made)
    }

    /// Creates the allocator, bindless tables, command pool, progress timeline and the texture
    /// empty slots show (`Drop` undoes a partial init).
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
        let stages = crate::bindless::stages(self.caps.mesh_shaders, self.caps.object_shaders);
        let tables = Bindless::new(&self.device, &mut allocator, anisotropy, stages);
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
        self.fill_empty_slots()
    }

    /// Creates the 1×1 texture (cleared to zero, resting in `General`) that every empty bindless
    /// slot shows, in both texture sets: an unwritten or freed descriptor is undefined to read, and
    /// lavapipe faults on one, whereas Metal reads its nil entries as zero.
    fn fill_empty_slots(&mut self) -> Result<(), RhiError> {
        let (device, queue) = (&self.device, self.queue);
        let state = self
            .state
            .get_mut()
            .map_err(|_| RhiError::DeviceLost("state poisoned".into()))?;
        let allocator = state
            .allocator
            .as_mut()
            .ok_or_else(|| RhiError::DeviceLost("no allocator".into()))?;
        let desc = TextureDesc {
            width: std::num::NonZeroU32::MIN,
            height: std::num::NonZeroU32::MIN,
            mip_levels: std::num::NonZeroU32::MIN,
            samples: concordia_rhi::SampleCount::X1,
            format: concordia_rhi::Format::Rgba8Unorm,
            usage: TextureUsage::SAMPLED | TextureUsage::STORAGE | TextureUsage::COPY_DST,
            label: "empty bindless slot".into(),
        };
        let texture =
            resources::create_texture(&mut Ctx { device, allocator }, &desc, Placement::Own)?;
        let (image, sampled, storage) = (texture.raw, texture.view, texture.mip0);
        state.empty_slot = Some(texture);
        run_once(device, queue, state.command_pool, |cb| {
            clear_to_general(device, cb, image)
        })?;
        if let Some(tables) = state.bindless.as_mut() {
            tables.fill_empty(device, sampled, storage);
        }
        Ok(())
    }

    fn wsi(&self) -> Result<&Wsi, RhiError> {
        self.wsi.as_ref().ok_or_else(|| {
            RhiError::Unsupported(format!(
                "{} can't present: the loader lacks VK_KHR_surface or the device VK_KHR_swapchain",
                self.caps.device_name
            ))
        })
    }

    /// A surface entry for `raw` after checking the queue can present to it in `format`.
    fn surface_entry(
        &self,
        wsi: &Wsi,
        raw: vk::SurfaceKHR,
        format: concordia_rhi::Format,
        target: &SurfaceTarget,
    ) -> Result<VkSurface, RhiError> {
        if !wsi.supports(self.physical, self.family, raw)? {
            return Err(RhiError::Unsupported(format!(
                "{}'s queue can't present to {target:?}",
                self.caps.device_name
            )));
        }
        let offered = wsi.formats(self.physical, raw)?;
        Ok(VkSurface {
            raw,
            format,
            choice: crate::swapchain::surface_format(&offered, format, self.mutable_format)?,
            chain: None,
            acquired: None,
            spare: Vec::new(),
            stale: false,
        })
    }

    /// Rebuilds `entry`'s swapchain when it has none, its size changed or it went stale. Called
    /// with the state locked (the queue is waited on).
    fn ensure_chain(
        &self,
        wsi: &Wsi,
        entry: &mut VkSurface,
        plan: &crate::swapchain::Plan,
    ) -> Result<(), RhiError> {
        if !entry.stale
            && entry
                .chain
                .as_ref()
                .is_some_and(|c| c.extent == plan.extent)
        {
            return Ok(());
        }
        // SAFETY: the queue is used only under the state lock, which the caller holds.
        unsafe { self.device.queue_wait_idle(self.queue) }
            .map_err(|e| error("vkQueueWaitIdle", e))?;
        let transform = wsi.transform(self.physical, entry.raw)?;
        let old = entry.chain.take();
        let old_raw = old.as_ref().map_or(vk::SwapchainKHR::null(), |c| c.raw);
        let built = wsi.build(&self.device, entry, plan, transform, old_raw);
        if let Some(old) = old {
            wsi.destroy(&self.device, old);
        }
        entry.chain = Some(built?);
        entry.stale = false;
        Ok(())
    }

    /// Acquires the chain's next image as a texture, or `None` (no image in time, or out of date:
    /// then `entry.stale` is set).
    fn next_image(
        &self,
        wsi: &Wsi,
        entry: &mut VkSurface,
        resources: &mut Resources,
    ) -> Result<Option<Acquired>, RhiError> {
        let chain = entry
            .chain
            .as_ref()
            .ok_or_else(|| RhiError::invalid("acquire", "the surface has no swapchain"))?;
        let semaphore = match entry.spare.pop() {
            Some(s) => s,
            None => wsi::binary_semaphore(&self.device)?,
        };
        // SAFETY: the swapchain is live and the semaphore unsignalled with no pending operation.
        let next = unsafe {
            wsi.swapchain.acquire_next_image(
                chain.raw,
                wsi::ACQUIRE_TIMEOUT_NS,
                semaphore,
                vk::Fence::null(),
            )
        };
        let (index, suboptimal) = match next {
            Ok(next) => next,
            Err(vk::Result::TIMEOUT | vk::Result::NOT_READY) => {
                entry.spare.push(semaphore);
                return Ok(None);
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                entry.spare.push(semaphore);
                entry.stale = true;
                return Ok(None);
            }
            Err(e) => {
                entry.spare.push(semaphore);
                return Err(error("vkAcquireNextImageKHR", e));
            }
        };
        entry.stale |= suboptimal;
        let i = index as usize;
        let [width, height] = chain.extent;
        let side = |n| std::num::NonZeroU32::new(n).unwrap_or(std::num::NonZeroU32::MIN);
        let texture = VkTexture {
            raw: chain.images[i],
            view: chain.views[i],
            mip0: chain.views[i],
            allocation: None,
            desc: TextureDesc {
                width: side(width),
                height: side(height),
                mip_levels: std::num::NonZeroU32::MIN,
                samples: SampleCount::X1,
                format: entry.format,
                usage: TextureUsage::RENDER_TARGET,
                label: "surface image".into(),
            },
            // Moved from `Undefined` to `Present` at the start of the frame's submission.
            state: TexState {
                home: Layout::Present,
                defined: true,
            },
            bindless: None,
            swapchain: true,
        };
        let (slot, generation) = resources.textures.insert(texture);
        Ok(Some(Acquired {
            index,
            texture: Texture::from_raw(slot, generation),
            semaphore,
        }))
    }

    /// Queues the image for presentation; `Ok(true)` when the swapchain should be rebuilt.
    fn queue_present(
        &self,
        wsi: &Wsi,
        swapchain: vk::SwapchainKHR,
        index: u32,
        rendered: vk::Semaphore,
    ) -> Result<bool, RhiError> {
        let (waits, swapchains, indices) = ([rendered], [swapchain], [index]);
        let info = vk::PresentInfoKHR::default()
            .wait_semaphores(&waits)
            .swapchains(&swapchains)
            .image_indices(&indices);
        // SAFETY: the queue is used under the state lock (held by the caller); the image was
        // acquired from this swapchain and the frame's submission signals `rendered`.
        match unsafe { wsi.swapchain.queue_present(self.queue, &info) } {
            Ok(suboptimal) => Ok(suboptimal),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(true),
            Err(e) => Err(error("vkQueuePresentKHR", e)),
        }
    }

    fn mark_stale(&self, state: &mut State, surface: Surface, stale: bool) {
        if let Some(entry) = state
            .surfaces
            .get_mut(surface.index(), surface.generation())
        {
            entry.stale |= stale;
        }
    }

    /// Waits for the surface's work (and a pending acquire) to finish, then destroys it. Called
    /// with the state locked.
    fn release_surface(&self, wsi: &Wsi, entry: VkSurface) {
        if let Some(acquired) = &entry.acquired {
            // A no-op batch consumes the acquire's signal, so the semaphore can be destroyed.
            let waits = [binary_info(acquired.semaphore)];
            let info = vk::SubmitInfo2::default().wait_semaphore_infos(&waits);
            // SAFETY: the queue is used under the state lock; the semaphore is live.
            let _ = unsafe {
                self.device
                    .queue_submit2(self.queue, &[info], vk::Fence::null())
            };
        }
        // SAFETY: as above.
        let _ = unsafe { self.device.queue_wait_idle(self.queue) };
        wsi.destroy_surface(&self.device, entry);
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

    /// In strict mode, an error naming the validation layer's errors so far.
    fn checked(&self) -> Result<(), RhiError> {
        if !self.strict {
            return Ok(());
        }
        let errors = self.instance.log.errors();
        match errors.is_empty() {
            true => Ok(()),
            false => Err(RhiError::invalid(
                "validation",
                format!(
                    "the Khronos validation layer reported {} error(s):\n{}",
                    errors.len(),
                    errors.join("\n")
                ),
            )),
        }
    }

    /// The oldest submission the GPU may still be running.
    fn oldest_in_flight(&self) -> Option<u64> {
        self.state.lock().ok()?.in_flight.front().map(|f| f.value)
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, RhiError> {
        self.state
            .lock()
            .map_err(|_| RhiError::DeviceLost("the device's state was poisoned by a panic".into()))
    }

    /// Frees what finished submissions held and reports their timing.
    fn retire(&self) {
        let mut reports = Reports::default();
        if let Ok(mut state) = self.state.lock() {
            // SAFETY: the progress semaphore is live.
            match unsafe { self.device.get_semaphore_counter_value(self.progress) } {
                Ok(done) => self.retire_to(&mut state, done, &mut reports),
                Err(vk::Result::ERROR_DEVICE_LOST) => state.lost = true,
                Err(_) => {}
            }
        }
        reports.fire();
    }

    fn retire_to(&self, state: &mut State, done: u64, reports: &mut Reports) {
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
                reports.timings.push((callback, timing.unwrap_or_default()));
            }
            reports.presented.extend(finished.presented);
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
                        tables.clear_texture(&self.device, slot);
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
        let now = self.submit_locked(&mut guard, &submit, on_time, Extra::default())?;
        drop(guard);
        if let Some(report) = now {
            report(GpuTiming::default());
        }
        self.checked()
    }

    /// Records and submits under the lock; returns a timing callback to call (unlocked) at once
    /// when the submission can't be timed.
    fn submit_locked(
        &self,
        state: &mut State,
        submit: &Submit<'_>,
        on_time: Option<GpuTimeCallback>,
        extra: Extra,
    ) -> Result<Option<GpuTimeCallback>, RhiError> {
        if state.lost {
            return Err(RhiError::DeviceLost(
                "an earlier submission lost the device".into(),
            ));
        }
        let mut waits = waits(state, submit.wait)?;
        waits.extend(extra.waits);
        let mut signals = signals(state, submit.signal)?;
        signals.extend(extra.signals);
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
            submit,
            (&waits, &signals),
            &extra.inits,
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
            presented: None,
        });
        if let Some(retirer) = self.waker.get() {
            retirer.unpark();
        }
        Ok(now)
    }

    fn record_and_submit(
        &self,
        state: &mut State,
        cb: vk::CommandBuffer,
        queries: Option<(vk::QueryPool, &Queries)>,
        submit: &Submit<'_>,
        (waits, signals): (
            &[vk::SemaphoreSubmitInfo<'static>],
            &[vk::SemaphoreSubmitInfo<'static>],
        ),
        extra_inits: &[Texture],
    ) -> Result<BTreeMap<Texture, TexState>, RhiError> {
        let tables = state
            .bindless
            .as_ref()
            .ok_or_else(|| RhiError::DeviceLost("no bindless tables".into()))?;
        let target = Target {
            device: &self.device,
            cb,
            layout: tables.pipeline_layout,
            stages: tables.stages,
            sets: tables.sets,
            mesh: self.mesh.as_ref(),
            queries,
        };
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: `cb` is a reset, unused command buffer from the locked pool.
        unsafe { self.device.begin_command_buffer(cb, &begin) }
            .map_err(|e| error("vkBeginCommandBuffer", e))?;
        let inits: Vec<Texture> = state.inits.iter().chain(extra_inits).copied().collect();
        let recorded = encode::record(&target, &state.resources, &submit.commands.commands, &inits);
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

/// Callbacks collected under the lock and fired after it.
#[derive(Default)]
struct Reports {
    timings: Vec<(GpuTimeCallback, GpuTiming)>,
    presented: Vec<PresentedCallback>,
}

impl Reports {
    fn fire(self) {
        for (callback, timing) in self.timings {
            callback(timing);
        }
        // The frame's GPU work has finished; Vulkan can't say when it reached the display
        // (`VK_KHR_present_wait` later), so this is the time it retired.
        let now = std::time::SystemTime::now();
        for callback in self.presented {
            callback(Some(now));
        }
    }
}

const MUTABLE_FORMAT: &str = "VK_KHR_swapchain_mutable_format";

/// Whether the instance can make surfaces and the device can present (`VK_KHR_swapchain`).
fn can_present(instance: &Instance, chosen: &Candidate) -> bool {
    instance.has_surface(crate::instance::SURFACE)
        && chosen
            .record
            .extensions
            .iter()
            .any(|e| e == "VK_KHR_swapchain")
}

/// A binary semaphore in a submission (waited on or signalled by all commands).
fn binary_info(semaphore: vk::Semaphore) -> vk::SemaphoreSubmitInfo<'static> {
    semaphore_info(semaphore, 0)
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

/// The logical device: the required features, plus `VK_EXT_mesh_shader` when `caps` (masked) has
/// mesh shaders, and the swapchain extensions when it `presents`.
fn create_logical(
    chosen: &Candidate,
    instance: &Instance,
    (presents, mutable_format): (bool, bool),
    caps: &RhiCaps,
) -> Result<ash::Device, RhiError> {
    let priorities = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(chosen.family)
        .queue_priorities(&priorities)];
    let mut v11 = vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(true);
    let mut v12 = vk::PhysicalDeviceVulkan12Features::default()
        .timeline_semaphore(true)
        .buffer_device_address(true)
        .descriptor_binding_partially_bound(true)
        .descriptor_binding_sampled_image_update_after_bind(true)
        .descriptor_binding_storage_image_update_after_bind(true)
        .descriptor_binding_update_unused_while_pending(true)
        .shader_sampled_image_array_non_uniform_indexing(true)
        .shader_storage_image_array_non_uniform_indexing(true);
    let mut v13 = vk::PhysicalDeviceVulkan13Features::default()
        .dynamic_rendering(true)
        .synchronization2(true);
    let core = vk::PhysicalDeviceFeatures::default()
        .shader_storage_image_read_without_format(true)
        .shader_storage_image_write_without_format(true)
        .shader_int64(true)
        .sampler_anisotropy(chosen.record.features.sampler_anisotropy);
    let mut mesh = vk::PhysicalDeviceMeshShaderFeaturesEXT::default()
        .mesh_shader(true)
        .task_shader(caps.object_shaders);
    let mut features = vk::PhysicalDeviceFeatures2::default()
        .features(core)
        .push_next(&mut v11)
        .push_next(&mut v12)
        .push_next(&mut v13);
    if caps.mesh_shaders {
        features = features.push_next(&mut mesh);
    }
    let portability = chosen
        .record
        .extensions
        .iter()
        .any(|e| e == "VK_KHR_portability_subset");
    let extensions: Vec<*const std::ffi::c_char> = portability
        .then_some(c"VK_KHR_portability_subset".as_ptr())
        .into_iter()
        .chain(presents.then_some(ash::khr::swapchain::NAME.as_ptr()))
        .chain(mutable_format.then_some(ash::khr::swapchain_mutable_format::NAME.as_ptr()))
        .chain(
            caps.mesh_shaders
                .then_some(ash::ext::mesh_shader::NAME.as_ptr()),
        )
        .collect();
    let info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queues)
        .enabled_extension_names(&extensions)
        .push_next(&mut features);
    // SAFETY: the physical device came from this instance and supports every enabled feature
    // (`record::choose` checked them; mesh and task shaders only when its record has them);
    // `info` and its chain are valid for the call.
    unsafe { instance.raw.create_device(chosen.physical, &info, None) }
        .map_err(|e| error("vkCreateDevice", e))
}

/// Records `record` into a one-off command buffer from `pool`, submits it to `queue` and waits.
fn run_once(
    device: &ash::Device,
    queue: vk::Queue,
    pool: vk::CommandPool,
    record: impl FnOnce(vk::CommandBuffer),
) -> Result<(), RhiError> {
    let info = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    // SAFETY: the pool is live and used only by the thread opening the device.
    let cb = unsafe { device.allocate_command_buffers(&info) }
        .map_err(|e| error("vkAllocateCommandBuffers", e))?[0];
    let begin =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    let cbs = [cb];
    // SAFETY: `cb` is new; it is recorded, submitted and waited on with a fence before it is freed,
    // and the queue is used only by the thread opening the device.
    let result = unsafe {
        device
            .begin_command_buffer(cb, &begin)
            .map_err(|e| error("vkBeginCommandBuffer", e))
            .and_then(|()| {
                record(cb);
                device
                    .end_command_buffer(cb)
                    .map_err(|e| error("vkEndCommandBuffer", e))
            })
            .and_then(|()| {
                device
                    .create_fence(&vk::FenceCreateInfo::default(), None)
                    .map_err(|e| error("vkCreateFence", e))
            })
            .and_then(|fence| {
                let submit = vk::SubmitInfo::default().command_buffers(&cbs);
                let waited = device
                    .queue_submit(queue, &[submit], fence)
                    .and_then(|()| device.wait_for_fences(&[fence], true, u64::MAX))
                    .map_err(|e| error("vkQueueSubmit", e));
                device.destroy_fence(fence, None);
                waited
            })
    };
    // SAFETY: the submission finished (or never started), so the buffer is not in use.
    unsafe { device.free_command_buffers(pool, &cbs) };
    result
}

/// Records moving a 1×1 color `image` from `Undefined` to `General`, cleared to zero and visible to
/// any later read.
fn clear_to_general(device: &ash::Device, cb: vk::CommandBuffer, image: vk::Image) {
    type Scope = (vk::PipelineStageFlags2, vk::AccessFlags2);
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1);
    let barrier = |old, src: Scope, dst: Scope| {
        [vk::ImageMemoryBarrier2::default()
            .image(image)
            .old_layout(old)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_stage_mask(src.0)
            .src_access_mask(src.1)
            .dst_stage_mask(dst.0)
            .dst_access_mask(dst.1)
            .subresource_range(range)]
    };
    let clear = (
        vk::PipelineStageFlags2::CLEAR,
        vk::AccessFlags2::TRANSFER_WRITE,
    );
    let before = barrier(
        vk::ImageLayout::UNDEFINED,
        (vk::PipelineStageFlags2::NONE, vk::AccessFlags2::NONE),
        clear,
    );
    let after = barrier(
        vk::ImageLayout::GENERAL,
        clear,
        (
            vk::PipelineStageFlags2::ALL_COMMANDS,
            vk::AccessFlags2::MEMORY_READ,
        ),
    );
    // SAFETY: `cb` is recording; the image is live, 1×1 with one mip and layer, with TRANSFER_DST
    // usage; synchronization2 is enabled.
    unsafe {
        device.cmd_pipeline_barrier2(
            cb,
            &vk::DependencyInfo::default().image_memory_barriers(&before),
        );
        device.cmd_clear_color_image(
            cb,
            image,
            vk::ImageLayout::GENERAL,
            &vk::ClearColorValue::default(),
            &[range],
        );
        device.cmd_pipeline_barrier2(
            cb,
            &vk::DependencyInfo::default().image_memory_barriers(&after),
        );
    }
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
        desc.validate(&self.caps)?;
        let mut state = self.lock()?;
        let layout = state
            .bindless
            .as_ref()
            .map(|b| b.pipeline_layout)
            .ok_or_else(|| RhiError::DeviceLost("no layout".into()))?;
        let made = pipeline::mesh(&self.device, layout, desc, &self.caps)?;
        let (index, generation) = state.resources.pipelines.insert(made);
        Ok(Pipeline::from_raw(index, generation))
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
        let read = bytes[range].to_vec();
        drop(state);
        self.checked()?;
        Ok(read)
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
        self.checked()?;
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
        self.checked()
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
            if s.resources.texture(texture).is_ok_and(|t| t.swapchain) {
                return None; // the surface releases its images (present, destroy_surface)
            }
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
        target: SurfaceTarget,
        format: concordia_rhi::Format,
    ) -> Result<Surface, RhiError> {
        let wsi = self.wsi()?;
        // SAFETY: forwarded from the caller (the native objects outlive the surface).
        let raw = unsafe { wsi::create_raw(&self.instance, target)? };
        let made = self
            .surface_entry(wsi, raw, format, &target)
            .and_then(|entry| {
                let (index, generation) = self.lock()?.surfaces.insert(entry);
                Ok(Surface::from_raw(index, generation))
            });
        if made.is_err() {
            // SAFETY: the surface was just created and nothing uses it.
            unsafe { wsi.surface.destroy_surface(raw, None) };
        }
        made
    }

    fn acquire(
        &self,
        surface: Surface,
        width: u32,
        height: u32,
    ) -> Result<Option<Texture>, RhiError> {
        let wsi = self.wsi()?;
        self.retire();
        let mut guard = self.lock()?;
        let State {
            surfaces,
            resources,
            ..
        } = &mut *guard;
        let entry = surfaces
            .get_mut(surface.index(), surface.generation())
            .ok_or_else(|| stale("surface", surface.index(), surface.generation()))?;
        if entry.acquired.is_some() {
            return Err(RhiError::invalid(
                "acquire",
                "present the acquired image before acquiring another",
            ));
        }
        // A second try after the swapchain went out of date and was rebuilt.
        for _ in 0..2 {
            let limits = wsi.limits(self.physical, entry.raw)?;
            let Some(plan) = crate::swapchain::plan(&limits, width, height)? else {
                return Ok(None);
            };
            self.ensure_chain(wsi, entry, &plan)?;
            if let Some(acquired) = self.next_image(wsi, entry, resources)? {
                let texture = acquired.texture;
                entry.acquired = Some(acquired);
                return Ok(Some(texture));
            }
            if !entry.stale {
                return Ok(None);
            }
        }
        Ok(None)
    }

    fn present(
        &self,
        surface: Surface,
        present: concordia_rhi::Present<'_>,
    ) -> Result<(), RhiError> {
        present.commands.validate()?;
        let wsi = self.wsi()?;
        self.retire();
        let mut guard = self.lock()?;
        let state = &mut *guard;
        let entry = state
            .surfaces
            .get_mut(surface.index(), surface.generation())
            .ok_or_else(|| stale("surface", surface.index(), surface.generation()))?;
        let acquired = entry.acquired.take().ok_or_else(|| {
            RhiError::invalid("present", "acquire an image from the surface first")
        })?;
        let chain = entry
            .chain
            .as_ref()
            .ok_or_else(|| RhiError::invalid("present", "the surface has no swapchain"))?;
        let (swapchain, rendered) = (chain.raw, chain.rendered[acquired.index as usize]);
        let extra = Extra {
            inits: vec![acquired.texture],
            waits: vec![binary_info(acquired.semaphore)],
            signals: vec![binary_info(rendered)],
        };
        let submit = Submit {
            queue: concordia_rhi::QueueKind::Graphics,
            commands: present.commands,
            wait: &[],
            signal: present.signal,
        };
        let submitted = self.submit_locked(state, &submit, present.on_gpu_time, extra);
        let texture = acquired.texture;
        state
            .resources
            .textures
            .remove(texture.index(), texture.generation());
        let value = state.submitted;
        state
            .graveyard
            .push((value, Dead::Semaphore(acquired.semaphore)));
        let now = match submitted {
            Ok(now) => now,
            Err(e) => {
                // The image stays acquired and unpresented: rebuild the swapchain next time.
                self.mark_stale(state, surface, true);
                return Err(e);
            }
        };
        if let (Some(callback), Some(last)) = (present.on_presented, state.in_flight.back_mut()) {
            last.presented = Some(callback);
        }
        let presented = self.queue_present(wsi, swapchain, acquired.index, rendered);
        self.mark_stale(state, surface, presented.as_ref().is_ok_and(|s| *s));
        drop(guard);
        if let Some(report) = now {
            report(GpuTiming::default());
        }
        presented?;
        self.checked()
    }

    fn destroy_surface(&self, surface: Surface) {
        let Some(wsi) = self.wsi.as_ref() else { return };
        let Ok(mut guard) = self.lock() else { return };
        let state = &mut *guard;
        let Some(entry) = state.surfaces.remove(surface.index(), surface.generation()) else {
            return;
        };
        if let Some(acquired) = &entry.acquired {
            let t = acquired.texture;
            state.resources.textures.remove(t.index(), t.generation());
        }
        self.release_surface(wsi, entry);
    }
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

impl Drop for Inner {
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
        if let Some(wsi) = &self.wsi {
            for entry in state.surfaces.drain() {
                wsi.destroy_surface(&self.device, entry);
            }
        }
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
        dead.extend(state.empty_slot.take().map(Dead::Texture));
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
