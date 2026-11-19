//! Window surfaces: a `CAMetalLayer` and the drawable currently acquired from it.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSSize;
use objc2_metal::{MTLDevice, MTLPixelFormat};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};

use concordia_rhi::{
    Format, RhiError, SampleCount, SurfaceTarget, Texture, TextureDesc, TextureUsage,
};

/// Largest drawable side (Metal's texture limit on Apple GPUs).
const MAX_SIDE: u32 = 16_384;

pub(crate) type Drawable = Retained<ProtocolObject<dyn CAMetalDrawable>>;

pub(crate) struct SurfaceEntry {
    layer: Retained<CAMetalLayer>,
    format: Format,
    current: Option<(Drawable, Texture)>,
}

impl std::fmt::Debug for SurfaceEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceEntry")
            .field("format", &self.format)
            .field("acquired", &self.current.is_some())
            .finish()
    }
}

fn pixel_format(format: Format) -> Result<MTLPixelFormat, RhiError> {
    match format {
        Format::Bgra8UnormSrgb => Ok(MTLPixelFormat::BGRA8Unorm_sRGB),
        Format::Bgra8Unorm => Ok(MTLPixelFormat::BGRA8Unorm),
        Format::Rgba16Float => Ok(MTLPixelFormat::RGBA16Float),
        other => Err(RhiError::Unsupported(format!(
            "CAMetalLayer cannot present {other:?}; use Bgra8UnormSrgb, Bgra8Unorm or Rgba16Float"
        ))),
    }
}

impl SurfaceEntry {
    /// Configures the layer for `device` and `format`.
    ///
    /// # Safety
    /// `target` must point to a live `CAMetalLayer` that outlives the entry.
    pub(crate) unsafe fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        target: SurfaceTarget,
        format: Format,
    ) -> Result<Self, RhiError> {
        let SurfaceTarget::MetalLayer(ptr) = target;
        let pixel = pixel_format(format)?;
        // SAFETY: the caller guarantees a live CAMetalLayer; retaining keeps it alive with us.
        let layer = unsafe { Retained::retain(ptr.as_ptr().cast::<CAMetalLayer>()) }
            .ok_or_else(|| RhiError::invalid("surface", "null layer"))?;
        layer.setDevice(Some(device));
        layer.setPixelFormat(pixel);
        // Two drawables (Metal's minimum) keep input-to-display latency short.
        layer.setMaximumDrawableCount(2);
        Ok(Self {
            layer,
            format,
            current: None,
        })
    }

    /// The next drawable at `width`×`height`, with the texture description to register it under.
    pub(crate) fn next(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<Option<(Drawable, TextureDesc)>, RhiError> {
        if self.current.is_some() {
            return Err(RhiError::invalid(
                "acquire",
                "present the acquired image before acquiring another",
            ));
        }
        let (Some(w), Some(h)) = (
            std::num::NonZeroU32::new(width),
            std::num::NonZeroU32::new(height),
        ) else {
            return Err(RhiError::invalid(
                "acquire",
                format!("surface size {width}×{height} must be non-zero"),
            ));
        };
        if width > MAX_SIDE || height > MAX_SIDE {
            return Err(RhiError::invalid(
                "acquire",
                format!("surface size {width}×{height} exceeds {MAX_SIDE}"),
            ));
        }
        let size = self.layer.drawableSize();
        if size.width as u32 != width || size.height as u32 != height {
            self.layer
                .setDrawableSize(NSSize::new(f64::from(width), f64::from(height)));
        }
        let Some(drawable) = self.layer.nextDrawable() else {
            return Ok(None);
        };
        let desc = TextureDesc {
            width: w,
            height: h,
            mip_levels: std::num::NonZeroU32::MIN,
            samples: SampleCount::X1,
            format: self.format,
            usage: TextureUsage::RENDER_TARGET,
            label: "surface image".into(),
        };
        Ok(Some((drawable, desc)))
    }

    /// Records the acquired drawable and its texture handle.
    pub(crate) fn hold(&mut self, drawable: Drawable, texture: Texture) {
        self.current = Some((drawable, texture));
    }

    /// Takes the acquired drawable, if any.
    pub(crate) fn take(&mut self) -> Option<(Drawable, Texture)> {
        self.current.take()
    }
}

/// Calls `callback` with the wall-clock time the drawable reached the display (`None` if it
/// never did). Metal reports `presentedTime` on the `CACurrentMediaTime` clock; it is mapped to
/// `SystemTime` by the offset between the two clocks when the handler runs.
pub(crate) fn on_presented(drawable: &Drawable, callback: concordia_rhi::PresentedCallback) {
    use objc2_metal::MTLDrawable;
    let slot = std::sync::Mutex::new(Some(callback));
    let block = block2::RcBlock::new(
        move |presented: std::ptr::NonNull<ProtocolObject<dyn MTLDrawable>>| {
            // SAFETY: Metal passes the live drawable for the duration of the handler.
            let seconds = unsafe { presented.as_ref() }.presentedTime();
            let time = (seconds > 0.0).then(|| {
                let age = (objc2_quartz_core::CACurrentMediaTime() - seconds).max(0.0);
                std::time::SystemTime::now() - std::time::Duration::from_secs_f64(age)
            });
            if let Some(callback) = slot.lock().ok().and_then(|mut c| c.take()) {
                callback(time);
            }
        },
    );
    let as_drawable: &ProtocolObject<dyn MTLDrawable> = ProtocolObject::from_ref(&**drawable);
    // SAFETY: the block is retained by Metal for the handler's lifetime; it is Send-safe because
    // its only state is a Mutex around a Send callback.
    unsafe { as_drawable.addPresentedHandler(&*block as *const _ as *mut _) };
}
