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

/// The surface format for `format`, from those the surface offers.
pub(crate) fn surface_format(
    offered: &[vk::SurfaceFormatKHR],
    format: Format,
) -> Result<vk::SurfaceFormatKHR, RhiError> {
    let (wanted, color_space) = match format {
        Format::Bgra8UnormSrgb => (
            vk::Format::B8G8R8A8_SRGB,
            Some(vk::ColorSpaceKHR::SRGB_NONLINEAR),
        ),
        Format::Bgra8Unorm => (
            vk::Format::B8G8R8A8_UNORM,
            Some(vk::ColorSpaceKHR::SRGB_NONLINEAR),
        ),
        Format::Rgba16Float => (vk::Format::R16G16B16A16_SFLOAT, None),
        other => {
            return Err(RhiError::Unsupported(format!(
                "surfaces cannot present {other:?}; use Bgra8UnormSrgb, Bgra8Unorm or Rgba16Float"
            )));
        }
    };
    offered
        .iter()
        .find(|f| f.format == wanted && color_space.is_none_or(|c| f.color_space == c))
        .copied()
        .ok_or_else(|| {
            let names: Vec<String> = offered
                .iter()
                .map(|f| format!("{:?}/{:?}", f.format, f.color_space))
                .collect();
            RhiError::Unsupported(format!(
                "the surface offers no {wanted:?} for {format:?} (it offers {})",
                names.join(", ")
            ))
        })
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
    if limits.current_extent.is_some_and(|e| e != [width, height]) {
        return Ok(None);
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

    #[test]
    fn surface_formats_match_the_rhi_format_or_are_unsupported() {
        let srgb = vk::ColorSpaceKHR::SRGB_NONLINEAR;
        let f = |format, color_space| vk::SurfaceFormatKHR {
            format,
            color_space,
        };
        let offered = [
            f(vk::Format::B8G8R8A8_UNORM, srgb),
            f(vk::Format::B8G8R8A8_SRGB, srgb),
            f(
                vk::Format::R16G16B16A16_SFLOAT,
                vk::ColorSpaceKHR::EXTENDED_SRGB_LINEAR_EXT,
            ),
        ];
        assert_eq!(
            surface_format(&offered, Format::Bgra8UnormSrgb).unwrap(),
            offered[1]
        );
        assert_eq!(
            surface_format(&offered, Format::Bgra8Unorm).unwrap(),
            offered[0]
        );
        assert_eq!(
            surface_format(&offered, Format::Rgba16Float).unwrap(),
            offered[2]
        );
        let err = surface_format(&offered, Format::Rgba32Float).unwrap_err();
        assert!(matches!(err, RhiError::Unsupported(_)), "{err}");
        let err = surface_format(&offered[..1], Format::Bgra8UnormSrgb).unwrap_err();
        assert!(
            matches!(err, RhiError::Unsupported(_)),
            "not offered: {err}"
        );
        assert!(
            err.to_string().contains("B8G8R8A8_UNORM"),
            "names what is offered: {err}"
        );
        let wide = [f(
            vk::Format::B8G8R8A8_SRGB,
            vk::ColorSpaceKHR::DISPLAY_P3_NONLINEAR_EXT,
        )];
        assert!(
            surface_format(&wide, Format::Bgra8UnormSrgb).is_err(),
            "8-bit formats present in sRGB"
        );
    }
}
