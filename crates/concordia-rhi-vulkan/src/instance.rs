//! The loader, the instance with its optional validation layer, and physical-device probes.

use std::ffi::{CStr, c_char};

use ash::vk;
use concordia_rhi::RhiError;

use crate::Validation;
use crate::convert::{error, sample_counts};
use crate::record::{DeviceType, Features, ProbeRecord};
use crate::validation::{ValidationLog, callback};

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";
/// Device extensions the probe reports when present.
const NOTED_EXTENSIONS: [&CStr; 5] = [
    c"VK_EXT_mesh_shader",
    c"VK_KHR_ray_query",
    c"VK_KHR_portability_subset",
    c"VK_KHR_swapchain",
    c"VK_KHR_swapchain_mutable_format",
];
/// `VK_KHR_surface`, which every platform surface extension needs.
pub(crate) const SURFACE: &CStr = c"VK_KHR_surface";
/// Platform surface extensions, enabled when the loader has them (with `VK_KHR_surface`).
pub(crate) const PLATFORM_SURFACES: [&CStr; 5] = [
    c"VK_EXT_headless_surface",
    c"VK_KHR_xlib_surface",
    c"VK_KHR_xcb_surface",
    c"VK_KHR_wayland_surface",
    c"VK_EXT_metal_surface",
];
const PORTABILITY_ENUMERATION: &CStr = c"VK_KHR_portability_enumeration";

/// A physical device the backend could open.
pub(crate) struct Candidate {
    /// The device.
    pub physical: vk::PhysicalDevice,
    /// Its first graphics-and-compute queue family.
    pub family: u32,
    /// What the probe saw.
    pub record: ProbeRecord,
}

/// An instance, with a validation messenger when the layer is enabled.
pub(crate) struct Instance {
    pub entry: ash::Entry,
    pub raw: ash::Instance,
    pub debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    /// Kept alive while the messenger may call back into it.
    pub log: ValidationLog,
    /// Surface extensions enabled on the instance.
    pub surfaces: Vec<&'static CStr>,
}

fn no_device(reason: impl Into<String>) -> RhiError {
    RhiError::NoDevice {
        reason: reason.into(),
    }
}

impl Instance {
    /// Loads the Vulkan loader and creates a 1.3 instance.
    pub fn new(validation: Validation, log: ValidationLog) -> Result<Self, RhiError> {
        // SAFETY: loading the system loader runs its initialisers; no Vulkan objects exist yet.
        let entry = unsafe { ash::Entry::load() }
            .map_err(|e| no_device(format!("no Vulkan loader (libvulkan) could be opened: {e}")))?;
        check_instance_version(&entry)?;
        let layer = choose_layer(&entry, validation)?;
        let portability = has_instance_extension(&entry, PORTABILITY_ENUMERATION);
        let surfaces = surface_extensions(&entry);
        let raw = create_instance(&entry, layer, portability, &surfaces)?;
        let mut instance = Self {
            entry,
            raw,
            debug: None,
            log,
            surfaces,
        };
        if layer {
            instance.debug = Some(instance.messenger()?);
        }
        Ok(instance)
    }

    /// True when the validation layer is on.
    pub fn validating(&self) -> bool {
        self.debug.is_some()
    }

    /// True when surface extension `name` is enabled.
    pub fn has_surface(&self, name: &CStr) -> bool {
        self.surfaces.contains(&name)
    }

    fn messenger(
        &self,
    ) -> Result<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT), RhiError> {
        let utils = ash::ext::debug_utils::Instance::new(&self.entry, &self.raw);
        let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(callback))
            .user_data(self.log.user_data());
        // SAFETY: the instance has VK_EXT_debug_utils enabled; `self.log` outlives the messenger
        // (it is destroyed in `Drop` before the log's handle is released).
        let messenger = unsafe { utils.create_debug_utils_messenger(&info, None) }
            .map_err(|e| error("vkCreateDebugUtilsMessengerEXT", e))?;
        Ok((utils, messenger))
    }

    /// Reports an error-severity message as the validation layer would (test hook).
    pub fn report(&self, message: &CStr) {
        let Some((utils, _)) = &self.debug else {
            return;
        };
        let data = vk::DebugUtilsMessengerCallbackDataEXT::default().message(message);
        // SAFETY: the messenger exists and `data` is a valid, fully initialised callback record.
        unsafe {
            utils.submit_debug_utils_message(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
                &data,
            );
        }
    }

    /// Every physical device with a probe record.
    pub fn candidates(&self) -> Result<Vec<Candidate>, RhiError> {
        // SAFETY: the instance is valid.
        let physicals = unsafe { self.raw.enumerate_physical_devices() }
            .map_err(|e| error("vkEnumeratePhysicalDevices", e))?;
        physicals.into_iter().map(|p| self.candidate(p)).collect()
    }

    fn candidate(&self, physical: vk::PhysicalDevice) -> Result<Candidate, RhiError> {
        let (family, timestamp_bits) = self.queue_family(physical);
        let mut v12 = vk::PhysicalDeviceVulkan12Properties::default();
        let mut v13 = vk::PhysicalDeviceVulkan13Properties::default();
        let mut props = vk::PhysicalDeviceProperties2::default()
            .push_next(&mut v12)
            .push_next(&mut v13);
        // SAFETY: `physical` came from this instance; the chain holds valid, writable structs.
        unsafe {
            self.raw
                .get_physical_device_properties2(physical, &mut props)
        };
        let base = props.properties;
        let limits = base.limits;
        let record = ProbeRecord {
            device_name: base
                .device_name_as_c_str()
                .map_or_else(|_| "unknown".into(), |n| n.to_string_lossy().into_owned()),
            device_type: device_type(base.device_type),
            api_version: [
                vk::api_version_major(base.api_version),
                vk::api_version_minor(base.api_version),
                vk::api_version_patch(base.api_version),
            ],
            unified_memory: self.unified_memory(physical),
            features: self.features(physical),
            msaa_sample_counts: sample_counts(
                limits.framebuffer_color_sample_counts & limits.framebuffer_depth_sample_counts,
            ),
            max_buffer_bytes: v13.max_buffer_size,
            max_update_after_bind_sampled_images: v12
                .max_descriptor_set_update_after_bind_sampled_images,
            max_update_after_bind_storage_images: v12
                .max_descriptor_set_update_after_bind_storage_images,
            max_per_stage_update_after_bind_sampled_images: v12
                .max_per_stage_descriptor_update_after_bind_sampled_images,
            max_per_stage_update_after_bind_storage_images: v12
                .max_per_stage_descriptor_update_after_bind_storage_images,
            graphics_compute_queue: family.is_some(),
            queue_timestamp_valid_bits: timestamp_bits,
            timestamp_period_ns: limits.timestamp_period,
            extensions: self.noted_extensions(physical)?,
            surface_extensions: self.surface_names(),
        };
        Ok(Candidate {
            physical,
            family: family.unwrap_or(0),
            record,
        })
    }

    fn surface_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .surfaces
            .iter()
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn queue_family(&self, physical: vk::PhysicalDevice) -> (Option<u32>, u32) {
        // SAFETY: `physical` came from this instance.
        let families = unsafe {
            self.raw
                .get_physical_device_queue_family_properties(physical)
        };
        let wanted = vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE;
        families
            .iter()
            .position(|f| f.queue_flags.contains(wanted) && f.queue_count > 0)
            .map_or((None, 0), |i| {
                (u32::try_from(i).ok(), families[i].timestamp_valid_bits)
            })
    }

    fn unified_memory(&self, physical: vk::PhysicalDevice) -> bool {
        // SAFETY: `physical` came from this instance.
        let memory = unsafe { self.raw.get_physical_device_memory_properties(physical) };
        let heaps = &memory.memory_heaps[..memory.memory_heap_count as usize];
        heaps
            .iter()
            .all(|h| h.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
    }

    fn features(&self, physical: vk::PhysicalDevice) -> Features {
        let mut v11 = vk::PhysicalDeviceVulkan11Features::default();
        let mut v12 = vk::PhysicalDeviceVulkan12Features::default();
        let mut v13 = vk::PhysicalDeviceVulkan13Features::default();
        let mut all = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut v11)
            .push_next(&mut v12)
            .push_next(&mut v13);
        // SAFETY: `physical` came from this instance; the chain holds valid, writable structs.
        unsafe { self.raw.get_physical_device_features2(physical, &mut all) };
        let core = all.features;
        let on = |b: vk::Bool32| b == vk::TRUE;
        Features {
            dynamic_rendering: on(v13.dynamic_rendering),
            synchronization2: on(v13.synchronization2),
            timeline_semaphore: on(v12.timeline_semaphore),
            buffer_device_address: on(v12.buffer_device_address),
            descriptor_binding_partially_bound: on(v12.descriptor_binding_partially_bound),
            descriptor_binding_sampled_image_update_after_bind: on(
                v12.descriptor_binding_sampled_image_update_after_bind
            ),
            descriptor_binding_storage_image_update_after_bind: on(
                v12.descriptor_binding_storage_image_update_after_bind
            ),
            descriptor_binding_update_unused_while_pending: on(
                v12.descriptor_binding_update_unused_while_pending
            ),
            shader_storage_image_read_without_format: on(
                core.shader_storage_image_read_without_format
            ),
            shader_storage_image_write_without_format: on(
                core.shader_storage_image_write_without_format
            ),
            shader_int64: on(core.shader_int64),
            shader_draw_parameters: on(v11.shader_draw_parameters),
            sampler_anisotropy: on(core.sampler_anisotropy),
        }
    }

    fn noted_extensions(&self, physical: vk::PhysicalDevice) -> Result<Vec<String>, RhiError> {
        // SAFETY: `physical` came from this instance.
        let available = unsafe { self.raw.enumerate_device_extension_properties(physical) }
            .map_err(|e| error("vkEnumerateDeviceExtensionProperties", e))?;
        let mut noted: Vec<String> = available
            .iter()
            .filter_map(|e| e.extension_name_as_c_str().ok())
            .filter(|name| NOTED_EXTENSIONS.contains(name))
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        noted.sort();
        Ok(noted)
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: every device created from this instance is destroyed before it (the device owns
        // its instance and drops it last); the messenger is destroyed before the instance.
        unsafe {
            if let Some((utils, messenger)) = self.debug.take() {
                utils.destroy_debug_utils_messenger(messenger, None);
            }
            self.raw.destroy_instance(None);
        }
    }
}

fn check_instance_version(entry: &ash::Entry) -> Result<(), RhiError> {
    // SAFETY: the entry is loaded.
    let version = unsafe { entry.try_enumerate_instance_version() }
        .map_err(|e| error("vkEnumerateInstanceVersion", e))?
        .unwrap_or(vk::API_VERSION_1_0);
    match version >= vk::API_VERSION_1_3 {
        true => Ok(()),
        false => Err(no_device(format!(
            "the Vulkan loader is {}.{}; the backend needs 1.3",
            vk::api_version_major(version),
            vk::api_version_minor(version)
        ))),
    }
}

/// Whether to enable the validation layer.
fn choose_layer(entry: &ash::Entry, validation: Validation) -> Result<bool, RhiError> {
    if validation == Validation::Off {
        return Ok(false);
    }
    // SAFETY: the entry is loaded.
    let layers = unsafe { entry.enumerate_instance_layer_properties() }
        .map_err(|e| error("vkEnumerateInstanceLayerProperties", e))?;
    let present = layers
        .iter()
        .any(|l| l.layer_name_as_c_str() == Ok(VALIDATION_LAYER));
    match (present, validation) {
        (false, Validation::Required) => Err(no_device(
            "validation is required but VK_LAYER_KHRONOS_validation is not installed \
             (Debian/Ubuntu: vulkan-validationlayers)",
        )),
        (present, _) => Ok(present),
    }
}

fn has_instance_extension(entry: &ash::Entry, name: &CStr) -> bool {
    // SAFETY: the entry is loaded.
    unsafe { entry.enumerate_instance_extension_properties(None) }
        .map(|all| all.iter().any(|e| e.extension_name_as_c_str() == Ok(name)))
        .unwrap_or(false)
}

/// `VK_KHR_surface` and the platform surface extensions the loader has (none without it).
fn surface_extensions(entry: &ash::Entry) -> Vec<&'static CStr> {
    if !has_instance_extension(entry, SURFACE) {
        return Vec::new();
    }
    std::iter::once(SURFACE)
        .chain(
            PLATFORM_SURFACES
                .into_iter()
                .filter(|name| has_instance_extension(entry, name)),
        )
        .collect()
}

fn create_instance(
    entry: &ash::Entry,
    layer: bool,
    portability: bool,
    surfaces: &[&CStr],
) -> Result<ash::Instance, RhiError> {
    let app = vk::ApplicationInfo::default()
        .application_name(c"concordia")
        .engine_name(c"concordia")
        .api_version(vk::API_VERSION_1_3);
    let layers: Vec<*const c_char> = layer
        .then_some(VALIDATION_LAYER.as_ptr())
        .into_iter()
        .collect();
    let mut extensions: Vec<*const c_char> = surfaces.iter().map(|n| n.as_ptr()).collect();
    if layer {
        extensions.push(ash::ext::debug_utils::NAME.as_ptr());
    }
    let mut flags = vk::InstanceCreateFlags::empty();
    if portability {
        extensions.push(PORTABILITY_ENUMERATION.as_ptr());
        flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
    }
    let info = vk::InstanceCreateInfo::default()
        .flags(flags)
        .application_info(&app)
        .enabled_layer_names(&layers)
        .enabled_extension_names(&extensions);
    // SAFETY: `info` and the name arrays it points to live for the call.
    unsafe { entry.create_instance(&info, None) }.map_err(|e| match e {
        vk::Result::ERROR_INCOMPATIBLE_DRIVER => {
            no_device("no Vulkan 1.3 driver (ERROR_INCOMPATIBLE_DRIVER)")
        }
        other => error("vkCreateInstance", other),
    })
}

fn device_type(raw: vk::PhysicalDeviceType) -> DeviceType {
    match raw {
        vk::PhysicalDeviceType::DISCRETE_GPU => DeviceType::Discrete,
        vk::PhysicalDeviceType::INTEGRATED_GPU => DeviceType::Integrated,
        vk::PhysicalDeviceType::VIRTUAL_GPU => DeviceType::Virtual,
        vk::PhysicalDeviceType::CPU => DeviceType::Cpu,
        _ => DeviceType::Other,
    }
}
