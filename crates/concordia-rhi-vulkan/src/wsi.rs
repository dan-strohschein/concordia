//! Window surfaces and swapchains (`VK_KHR_surface`, `VK_KHR_swapchain` and the platform surface
//! extensions). Planning is pure (`swapchain`); this module makes the Vulkan calls.

use std::ffi::CStr;

use ash::vk;
use concordia_rhi::{Format, RhiError, SurfaceTarget, Texture};

use crate::convert::error;
use crate::instance::Instance;
use crate::swapchain::{Plan, SurfaceLimits};

/// How long `acquire` waits for a free image before skipping the frame (Metal's `nextDrawable`
/// waits up to a second too).
pub(crate) const ACQUIRE_TIMEOUT_NS: u64 = 1_000_000_000;

/// The window-system functions a device with `VK_KHR_swapchain` loads.
pub(crate) struct Wsi {
    pub surface: ash::khr::surface::Instance,
    pub swapchain: ash::khr::swapchain::Device,
}

/// A swapchain and what is made per image.
pub(crate) struct Chain {
    pub raw: vk::SwapchainKHR,
    pub extent: [u32; 2],
    pub images: Vec<vk::Image>,
    pub views: Vec<vk::ImageView>,
    /// Signalled by the frame drawing into each image; presentation waits on it.
    pub rendered: Vec<vk::Semaphore>,
}

/// The image currently acquired from a surface.
pub(crate) struct Acquired {
    pub index: u32,
    pub texture: Texture,
    /// Signalled when the image is ready; the frame's submission waits on it.
    pub semaphore: vk::Semaphore,
}

/// A surface: the native target's `VkSurfaceKHR`, its format and its swapchain.
pub(crate) struct VkSurface {
    pub raw: vk::SurfaceKHR,
    pub format: Format,
    pub surface_format: vk::SurfaceFormatKHR,
    pub chain: Option<Chain>,
    pub acquired: Option<Acquired>,
    /// Unsignalled acquire semaphores to reuse.
    pub spare: Vec<vk::Semaphore>,
    /// Out of date or suboptimal: rebuild before the next acquire.
    pub stale: bool,
}

fn needs(instance: &Instance, name: &CStr, target: &SurfaceTarget) -> Result<(), RhiError> {
    match instance.has_surface(name) {
        true => Ok(()),
        false => Err(RhiError::Unsupported(format!(
            "{target:?} needs {}, which the Vulkan loader does not have",
            name.to_string_lossy()
        ))),
    }
}

/// Creates the `VkSurfaceKHR` for `target`.
///
/// # Safety
/// The native objects behind `target` must be alive and outlive the surface.
pub(crate) unsafe fn create_raw(
    instance: &Instance,
    target: SurfaceTarget,
) -> Result<vk::SurfaceKHR, RhiError> {
    let (entry, raw) = (&instance.entry, &instance.raw);
    // SAFETY: (every arm) the extension is enabled on the instance (checked by `needs`), the
    // create infos are valid for the call, and the caller guarantees the native objects.
    unsafe {
        match target {
            SurfaceTarget::Headless => {
                needs(instance, c"VK_EXT_headless_surface", &target)?;
                ash::ext::headless_surface::Instance::new(entry, raw)
                    .create_headless_surface(&vk::HeadlessSurfaceCreateInfoEXT::default(), None)
            }
            SurfaceTarget::Xlib { display, window } => {
                needs(instance, c"VK_KHR_xlib_surface", &target)?;
                let window = vk::Window::try_from(window)
                    .map_err(|_| RhiError::invalid("surface", "the X11 window id overflows"))?;
                let info = vk::XlibSurfaceCreateInfoKHR::default()
                    .dpy(display.as_ptr())
                    .window(window);
                ash::khr::xlib_surface::Instance::new(entry, raw).create_xlib_surface(&info, None)
            }
            SurfaceTarget::Xcb { connection, window } => {
                needs(instance, c"VK_KHR_xcb_surface", &target)?;
                let info = vk::XcbSurfaceCreateInfoKHR::default()
                    .connection(connection.as_ptr())
                    .window(window);
                ash::khr::xcb_surface::Instance::new(entry, raw).create_xcb_surface(&info, None)
            }
            SurfaceTarget::Wayland { display, surface } => {
                needs(instance, c"VK_KHR_wayland_surface", &target)?;
                let info = vk::WaylandSurfaceCreateInfoKHR::default()
                    .display(display.as_ptr())
                    .surface(surface.as_ptr());
                ash::khr::wayland_surface::Instance::new(entry, raw)
                    .create_wayland_surface(&info, None)
            }
            SurfaceTarget::MetalLayer(layer) => {
                needs(instance, c"VK_EXT_metal_surface", &target)?;
                let info = vk::MetalSurfaceCreateInfoEXT::default().layer(layer.as_ptr());
                ash::ext::metal_surface::Instance::new(entry, raw).create_metal_surface(&info, None)
            }
        }
    }
    .map_err(|e| error("vkCreate*SurfaceKHR", e))
}

impl Wsi {
    /// Whether `family` on `physical` can present to `surface`.
    pub fn supports(
        &self,
        physical: vk::PhysicalDevice,
        family: u32,
        surface: vk::SurfaceKHR,
    ) -> Result<bool, RhiError> {
        // SAFETY: the surface and physical device belong to this instance.
        unsafe {
            self.surface
                .get_physical_device_surface_support(physical, family, surface)
        }
        .map_err(|e| error("vkGetPhysicalDeviceSurfaceSupportKHR", e))
    }

    /// The formats `surface` offers.
    pub fn formats(
        &self,
        physical: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
    ) -> Result<Vec<vk::SurfaceFormatKHR>, RhiError> {
        // SAFETY: as above.
        unsafe {
            self.surface
                .get_physical_device_surface_formats(physical, surface)
        }
        .map_err(|e| error("vkGetPhysicalDeviceSurfaceFormatsKHR", e))
    }

    /// What `surface` allows now.
    pub fn limits(
        &self,
        physical: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
    ) -> Result<SurfaceLimits, RhiError> {
        // SAFETY: as above.
        let caps = unsafe {
            self.surface
                .get_physical_device_surface_capabilities(physical, surface)
        }
        .map_err(|e| error("vkGetPhysicalDeviceSurfaceCapabilitiesKHR", e))?;
        // SAFETY: as above.
        let present_modes = unsafe {
            self.surface
                .get_physical_device_surface_present_modes(physical, surface)
        }
        .map_err(|e| error("vkGetPhysicalDeviceSurfacePresentModesKHR", e))?;
        let current = caps.current_extent;
        Ok(SurfaceLimits {
            min_images: caps.min_image_count,
            max_images: caps.max_image_count,
            current_extent: (current.width != u32::MAX).then_some([current.width, current.height]),
            min_extent: [caps.min_image_extent.width, caps.min_image_extent.height],
            max_extent: [caps.max_image_extent.width, caps.max_image_extent.height],
            usage: caps.supported_usage_flags,
            composite_alpha: caps.supported_composite_alpha,
            present_modes,
        })
    }

    /// Builds a swapchain for `surface` following `plan`, replacing `old` (may be null).
    pub fn build(
        &self,
        device: &ash::Device,
        surface: &VkSurface,
        plan: &Plan,
        transform: vk::SurfaceTransformFlagsKHR,
        old: vk::SwapchainKHR,
    ) -> Result<Chain, RhiError> {
        let [width, height] = plan.extent;
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(surface.raw)
            .min_image_count(plan.images)
            .image_format(surface.surface_format.format)
            .image_color_space(surface.surface_format.color_space)
            .image_extent(vk::Extent2D { width, height })
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(transform)
            .composite_alpha(plan.composite_alpha)
            .present_mode(plan.present_mode)
            .clipped(true)
            .old_swapchain(old);
        // SAFETY: the surface is live and supported by the device's queue family; `old` is null
        // or a retired swapchain of the same surface.
        let raw = unsafe { self.swapchain.create_swapchain(&info, None) }
            .map_err(|e| error("vkCreateSwapchainKHR", e))?;
        let mut chain = Chain {
            raw,
            extent: plan.extent,
            images: Vec::new(),
            views: Vec::new(),
            rendered: Vec::new(),
        };
        match self.fill(device, surface.surface_format.format, &mut chain) {
            Ok(()) => Ok(chain),
            Err(e) => {
                self.destroy(device, chain);
                Err(e)
            }
        }
    }

    /// Gets the chain's images and makes a view and a rendered semaphore for each.
    fn fill(
        &self,
        device: &ash::Device,
        format: vk::Format,
        chain: &mut Chain,
    ) -> Result<(), RhiError> {
        // SAFETY: the swapchain is live.
        chain.images = unsafe { self.swapchain.get_swapchain_images(chain.raw) }
            .map_err(|e| error("vkGetSwapchainImagesKHR", e))?;
        for &image in &chain.images {
            let view = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            // SAFETY: the image belongs to the live swapchain; the info is valid.
            chain.views.push(
                unsafe { device.create_image_view(&view, None) }
                    .map_err(|e| error("vkCreateImageView", e))?,
            );
            chain.rendered.push(binary_semaphore(device)?);
        }
        Ok(())
    }

    /// Destroys a chain whose images the GPU no longer uses.
    pub fn destroy(&self, device: &ash::Device, chain: Chain) {
        // SAFETY: the caller waited for the queue to go idle; nothing uses these any more.
        unsafe {
            for view in chain.views {
                device.destroy_image_view(view, None);
            }
            for semaphore in chain.rendered {
                device.destroy_semaphore(semaphore, None);
            }
            self.swapchain.destroy_swapchain(chain.raw, None);
        }
    }

    /// Destroys a surface, its chain and its semaphores (after the queue went idle).
    pub fn destroy_surface(&self, device: &ash::Device, mut surface: VkSurface) {
        if let Some(chain) = surface.chain.take() {
            self.destroy(device, chain);
        }
        // SAFETY: the queue is idle; the semaphores and the surface are no longer used.
        unsafe {
            for semaphore in surface.spare.drain(..) {
                device.destroy_semaphore(semaphore, None);
            }
            if let Some(acquired) = surface.acquired.take() {
                device.destroy_semaphore(acquired.semaphore, None);
            }
            self.surface.destroy_surface(surface.raw, None);
        }
    }

    /// The surface's current transform (for `preTransform`).
    pub fn transform(
        &self,
        physical: vk::PhysicalDevice,
        surface: vk::SurfaceKHR,
    ) -> Result<vk::SurfaceTransformFlagsKHR, RhiError> {
        // SAFETY: as in `supports`.
        let caps = unsafe {
            self.surface
                .get_physical_device_surface_capabilities(physical, surface)
        }
        .map_err(|e| error("vkGetPhysicalDeviceSurfaceCapabilitiesKHR", e))?;
        Ok(caps.current_transform)
    }
}

/// A new binary semaphore.
pub(crate) fn binary_semaphore(device: &ash::Device) -> Result<vk::Semaphore, RhiError> {
    // SAFETY: the info is valid.
    unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }
        .map_err(|e| error("vkCreateSemaphore", e))
}
