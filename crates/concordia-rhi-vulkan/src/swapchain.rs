//! Swapchain planning (pure): the image count, extent, format, present mode and composite alpha
//! a surface's limits allow for a requested size.

use ash::vk;
use concordia_rhi::{Format, RhiError};

/// What a surface allows (`vkGetPhysicalDeviceSurfaceCapabilitiesKHR` and present modes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SurfaceLimits {
    /// `minImageCount`.
    pub min_images: u32,
    /// `maxImageCount` (0: no limit).
    pub max_images: u32,
    /// `currentExtent`, or `None` when the swapchain decides (`0xFFFFFFFF`).
    pub current_extent: Option<[u32; 2]>,
    /// `minImageExtent`.
    pub min_extent: [u32; 2],
    /// `maxImageExtent`.
    pub max_extent: [u32; 2],
    /// `supportedUsageFlags`.
    pub usage: vk::ImageUsageFlags,
    /// `supportedCompositeAlpha`.
    pub composite_alpha: vk::CompositeAlphaFlagsKHR,
    /// The surface's present modes.
    pub present_modes: Vec<vk::PresentModeKHR>,
}

/// How to build a swapchain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Plan {
    /// Images to ask for.
    pub images: u32,
    /// Image size.
    pub extent: [u32; 2],
    /// Present mode.
    pub present_mode: vk::PresentModeKHR,
    /// Composite alpha.
    pub composite_alpha: vk::CompositeAlphaFlagsKHR,
}

/// Images per swapchain when the surface allows (like Metal's two drawables: short latency).
const IMAGES: u32 = 2;

/// The swapchain's surface format and the format its image views use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SurfaceChoice {
    /// The swapchain's format and color space.
    pub surface: vk::SurfaceFormatKHR,
    /// The views' format (sRGB over a UNORM swapchain when `mutable`).
    pub view: vk::Format,
    /// The swapchain needs `MUTABLE_FORMAT` (`VK_KHR_swapchain_mutable_format`).
    pub mutable: bool,
}

/// The surface format for `format`, from those the surface offers; `mutable_format` says whether
/// the device can view a UNORM swapchain as sRGB.
pub(crate) fn surface_format(
    offered: &[vk::SurfaceFormatKHR],
    format: Format,
    mutable_format: bool,
) -> Result<SurfaceChoice, RhiError> {
    let srgb = Some(vk::ColorSpaceKHR::SRGB_NONLINEAR);
    let (wanted, linear, color_space) = match format {
        Format::Bgra8UnormSrgb => (
            vk::Format::B8G8R8A8_SRGB,
            Some(vk::Format::B8G8R8A8_UNORM),
            srgb,
        ),
        Format::Bgra8Unorm => (vk::Format::B8G8R8A8_UNORM, None, srgb),
        Format::Rgba16Float => (vk::Format::R16G16B16A16_SFLOAT, None, None),
        other => {
            return Err(RhiError::Unsupported(format!(
                "surfaces cannot present {other:?}; use Bgra8UnormSrgb, Bgra8Unorm or Rgba16Float"
            )));
        }
    };
    let find = |f: vk::Format| {
        offered
            .iter()
            .find(|o| o.format == f && color_space.is_none_or(|c| o.color_space == c))
            .copied()
    };
    if let Some(surface) = find(wanted) {
        return Ok(SurfaceChoice {
            surface,
            view: wanted,
            mutable: false,
        });
    }
    if let Some(surface) = linear.filter(|_| mutable_format).and_then(find) {
        return Ok(SurfaceChoice {
            surface,
            view: wanted,
            mutable: true,
        });
    }
    let names: Vec<String> = offered
        .iter()
        .map(|f| format!("{:?}/{:?}", f.format, f.color_space))
        .collect();
    let hint = match linear {
        Some(l) if !mutable_format => {
            format!("; viewing its {l:?} as sRGB needs VK_KHR_swapchain_mutable_format")
        }
        _ => String::new(),
    };
    Err(RhiError::Unsupported(format!(
        "the surface offers no {wanted:?} for {format:?} (it offers {}){hint}",
        names.join(", ")
    )))
}

/// The swapchain for a `width`×`height` image, or `None` when the surface has a fixed size that
/// differs (skip the frame until the window reports its new size).
pub(crate) fn plan(
    limits: &SurfaceLimits,
    width: u32,
    height: u32,
) -> Result<Option<Plan>, RhiError> {
    let [min_w, min_h] = limits.min_extent;
    let [max_w, max_h] = limits.max_extent;
    // A window's surface is its current size; a different request means it is being resized.
    if width > 0 && height > 0 && limits.current_extent.is_some_and(|e| e != [width, height]) {
        return Ok(None);
    }
    if width == 0 || height == 0 || width > max_w || height > max_h {
        return Err(RhiError::invalid(
            "acquire",
            format!(
                "surface size {width}×{height} is outside {}×{}..={max_w}×{max_h}",
                min_w.max(1),
                min_h.max(1)
            ),
        ));
    }
    if !limits.usage.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT) {
        return Err(RhiError::Unsupported(
            "the surface's images can't be color attachments".into(),
        ));
    }
    if !limits.present_modes.contains(&vk::PresentModeKHR::FIFO) {
        return Err(RhiError::Unsupported(
            "the surface has no FIFO present mode (required by Vulkan)".into(),
        ));
    }
    let images = match limits.max_images {
        0 => IMAGES.max(limits.min_images),
        max => IMAGES.max(limits.min_images).min(max),
    };
    Ok(Some(Plan {
        images,
        extent: [width, height],
        present_mode: vk::PresentModeKHR::FIFO,
        composite_alpha: composite_alpha(limits.composite_alpha),
    }))
}

fn composite_alpha(supported: vk::CompositeAlphaFlagsKHR) -> vk::CompositeAlphaFlagsKHR {
    use vk::CompositeAlphaFlagsKHR as A;
    [A::OPAQUE, A::INHERIT, A::PRE_MULTIPLIED, A::POST_MULTIPLIED]
        .into_iter()
        .find(|a| supported.contains(*a))
        .unwrap_or(A::OPAQUE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> SurfaceLimits {
        SurfaceLimits {
            min_images: 1,
            max_images: 0,
            current_extent: None,
            min_extent: [1, 1],
            max_extent: [16_384, 16_384],
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
            composite_alpha: vk::CompositeAlphaFlagsKHR::OPAQUE
                | vk::CompositeAlphaFlagsKHR::INHERIT,
            present_modes: vec![vk::PresentModeKHR::FIFO],
        }
    }

    fn planned(limits: &SurfaceLimits, w: u32, h: u32) -> Plan {
        plan(limits, w, h).unwrap().expect("a plan")
    }

    #[test]
    fn two_images_unless_the_surface_needs_more_or_allows_fewer() {
        assert_eq!(
            planned(&limits(), 64, 64).images,
            2,
            "like Metal's two drawables"
        );
        let more = SurfaceLimits {
            min_images: 3,
            ..limits()
        };
        assert_eq!(planned(&more, 64, 64).images, 3);
        let one = SurfaceLimits {
            min_images: 1,
            max_images: 1,
            ..limits()
        };
        assert_eq!(planned(&one, 64, 64).images, 1);
    }

    #[test]
    fn fifo_is_the_present_mode_even_when_mailbox_exists() {
        let both = SurfaceLimits {
            present_modes: vec![vk::PresentModeKHR::MAILBOX, vk::PresentModeKHR::FIFO],
            ..limits()
        };
        assert_eq!(planned(&both, 8, 8).present_mode, vk::PresentModeKHR::FIFO);
        let none = SurfaceLimits {
            present_modes: vec![vk::PresentModeKHR::MAILBOX],
            ..limits()
        };
        assert!(matches!(plan(&none, 8, 8), Err(RhiError::Unsupported(_))));
    }

    #[test]
    fn a_free_extent_is_the_request_and_a_fixed_one_must_match_it() {
        assert_eq!(planned(&limits(), 640, 480).extent, [640, 480]);
        let fixed = SurfaceLimits {
            current_extent: Some([800, 600]),
            min_extent: [800, 600],
            max_extent: [800, 600],
            ..limits()
        };
        assert_eq!(planned(&fixed, 800, 600).extent, [800, 600]);
        assert_eq!(
            plan(&fixed, 640, 480).unwrap(),
            None,
            "skip until the sizes agree"
        );
        // A window being resized larger: its limits are its current size, so this is not an error.
        assert_eq!(
            plan(&fixed, 860, 600).unwrap(),
            None,
            "a request past a fixed extent waits for the window too"
        );
    }

    #[test]
    fn sizes_outside_the_surface_limits_are_invalid() {
        for (w, h) in [(0, 10), (10, 0), (16_385, 10), (10, 16_385)] {
            let err = plan(&limits(), w, h).expect_err("out of range");
            assert!(matches!(err, RhiError::Invalid { .. }), "{w}×{h}: {err}");
            assert!(err.to_string().contains(&format!("{w}×{h}")), "{err}");
        }
    }

    #[test]
    fn surfaces_without_color_attachment_usage_are_unsupported() {
        let no_color = SurfaceLimits {
            usage: vk::ImageUsageFlags::TRANSFER_DST,
            ..limits()
        };
        assert!(matches!(
            plan(&no_color, 8, 8),
            Err(RhiError::Unsupported(_))
        ));
    }

    #[test]
    fn opaque_composite_alpha_is_preferred_then_inherit() {
        assert_eq!(
            planned(&limits(), 8, 8).composite_alpha,
            vk::CompositeAlphaFlagsKHR::OPAQUE
        );
        let inherit = SurfaceLimits {
            composite_alpha: vk::CompositeAlphaFlagsKHR::INHERIT
                | vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            ..limits()
        };
        assert_eq!(
            planned(&inherit, 8, 8).composite_alpha,
            vk::CompositeAlphaFlagsKHR::INHERIT
        );
        let other = SurfaceLimits {
            composite_alpha: vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            ..limits()
        };
        assert_eq!(
            planned(&other, 8, 8).composite_alpha,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED
        );
    }

    fn f(format: vk::Format, color_space: vk::ColorSpaceKHR) -> vk::SurfaceFormatKHR {
        vk::SurfaceFormatKHR {
            format,
            color_space,
        }
    }

    const SRGB: vk::ColorSpaceKHR = vk::ColorSpaceKHR::SRGB_NONLINEAR;

    #[test]
    fn surface_formats_match_the_rhi_format_or_are_unsupported() {
        let offered = [
            f(vk::Format::B8G8R8A8_UNORM, SRGB),
            f(vk::Format::B8G8R8A8_SRGB, SRGB),
            f(
                vk::Format::R16G16B16A16_SFLOAT,
                vk::ColorSpaceKHR::EXTENDED_SRGB_LINEAR_EXT,
            ),
        ];
        let choose = |format| surface_format(&offered, format, false).unwrap();
        let exact = |surface: vk::SurfaceFormatKHR| SurfaceChoice {
            surface,
            view: surface.format,
            mutable: false,
        };
        assert_eq!(choose(Format::Bgra8UnormSrgb), exact(offered[1]));
        assert_eq!(choose(Format::Bgra8Unorm), exact(offered[0]));
        assert_eq!(choose(Format::Rgba16Float), exact(offered[2]));
        let err = surface_format(&offered, Format::Rgba32Float, true).unwrap_err();
        assert!(matches!(err, RhiError::Unsupported(_)), "{err}");
        let wide = [f(
            vk::Format::B8G8R8A8_SRGB,
            vk::ColorSpaceKHR::DISPLAY_P3_NONLINEAR_EXT,
        )];
        assert!(
            surface_format(&wide, Format::Bgra8UnormSrgb, true).is_err(),
            "8-bit formats present in sRGB"
        );
    }

    /// Mesa's headless surfaces offer only UNORM: an sRGB swapchain is then a UNORM one viewed
    /// as sRGB, which needs `VK_KHR_swapchain_mutable_format`.
    #[test]
    fn srgb_views_of_unorm_surfaces_need_mutable_format() {
        let unorm_only = [
            f(vk::Format::R8G8B8A8_UNORM, SRGB),
            f(vk::Format::B8G8R8A8_UNORM, SRGB),
        ];
        assert_eq!(
            surface_format(&unorm_only, Format::Bgra8UnormSrgb, true).unwrap(),
            SurfaceChoice {
                surface: unorm_only[1],
                view: vk::Format::B8G8R8A8_SRGB,
                mutable: true,
            }
        );
        let err = surface_format(&unorm_only, Format::Bgra8UnormSrgb, false).unwrap_err();
        assert!(matches!(err, RhiError::Unsupported(_)), "{err}");
        let text = err.to_string();
        assert!(
            text.contains("B8G8R8A8_UNORM") && text.contains("mutable"),
            "{text}"
        );
        let srgb_too = [unorm_only[1], f(vk::Format::B8G8R8A8_SRGB, SRGB)];
        assert!(
            !surface_format(&srgb_too, Format::Bgra8UnormSrgb, true)
                .unwrap()
                .mutable,
            "an offered sRGB format is used as is"
        );
    }
}
