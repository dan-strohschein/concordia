//! Checks every backend runs against its live resources before recording a command.

use std::num::NonZeroU32;

use crate::caps::SampleCount;
use crate::command::{Rect, TextureOrigin, Viewport};
use crate::error::RhiError;
use crate::resource::{BufferDesc, BufferUsage, TextureDesc, TextureUsage};

/// `bytes` of indirect arguments at `offset` fit `args`, which has `BufferUsage::INDIRECT`.
pub fn indirect_args(args: &BufferDesc, offset: u64, bytes: u64) -> Result<(), RhiError> {
    let invalid = |why: String| RhiError::invalid("indirect arguments", why);
    if !args.usage.contains(BufferUsage::INDIRECT) {
        return Err(invalid(format!("{:?} lacks INDIRECT usage", args.label)));
    }
    match offset.checked_add(bytes) {
        Some(end) if end <= args.size.get() => Ok(()),
        _ => Err(invalid(format!(
            "{bytes} bytes at {offset} run past the end of {:?} ({} bytes)",
            args.label, args.size
        ))),
    }
}

/// A copy of `extent` from `src_origin` in `src` to `dst_origin` in `dst` stays inside both mips,
/// between single-sample, non-depth textures of one format with the copy usages.
pub fn texture_copy(
    src: &TextureDesc,
    src_origin: TextureOrigin,
    dst: &TextureDesc,
    dst_origin: TextureOrigin,
    extent: [NonZeroU32; 2],
) -> Result<(), RhiError> {
    let invalid = |why: String| RhiError::invalid("texture copy", why);
    if src.format != dst.format {
        return Err(invalid(format!(
            "format {:?} differs from {:?}",
            src.format, dst.format
        )));
    }
    if src.format.is_depth() {
        return Err(invalid(format!("depth format {:?}", src.format)));
    }
    if src.samples != SampleCount::X1 || dst.samples != SampleCount::X1 {
        return Err(invalid("textures must be single-sample".into()));
    }
    if !src.usage.contains(TextureUsage::COPY_SRC) {
        return Err(invalid(format!("{:?} lacks COPY_SRC usage", src.label)));
    }
    if !dst.usage.contains(TextureUsage::COPY_DST) {
        return Err(invalid(format!("{:?} lacks COPY_DST usage", dst.label)));
    }
    region_in(src, src_origin, extent).map_err(invalid)?;
    region_in(dst, dst_origin, extent).map_err(invalid)
}

/// The region lies inside its mip of `desc`.
fn region_in(
    desc: &TextureDesc,
    origin: TextureOrigin,
    extent: [NonZeroU32; 2],
) -> Result<(), String> {
    if origin.mip >= desc.mip_levels.get() {
        return Err(format!("mip {} of {:?}", origin.mip, desc.label));
    }
    let size = [
        (desc.width.get() >> origin.mip).max(1),
        (desc.height.get() >> origin.mip).max(1),
    ];
    let fits = |start: u32, len: NonZeroU32, limit: u32| {
        start.checked_add(len.get()).is_some_and(|end| end <= limit)
    };
    match fits(origin.x, extent[0], size[0]) && fits(origin.y, extent[1], size[1]) {
        true => Ok(()),
        false => Err(format!(
            "region {}×{} at {origin:?} is outside {:?} ({}×{})",
            extent[0], extent[1], desc.label, size[0], size[1]
        )),
    }
}

/// `viewport` lies inside a pass whose targets are `size` (width, height).
pub fn viewport_in(viewport: &Viewport, size: [NonZeroU32; 2]) -> Result<(), RhiError> {
    let inside = viewport.x + viewport.width <= size[0].get() as f32
        && viewport.y + viewport.height <= size[1].get() as f32;
    match inside {
        true => Ok(()),
        false => Err(RhiError::invalid(
            "viewport",
            format!(
                "{viewport:?} is outside the {}×{} targets",
                size[0], size[1]
            ),
        )),
    }
}

/// `rect` lies inside a pass whose targets are `size` (width, height).
pub fn scissor_in(rect: &Rect, size: [NonZeroU32; 2]) -> Result<(), RhiError> {
    let fits = |start: u32, len: NonZeroU32, limit: NonZeroU32| {
        start
            .checked_add(len.get())
            .is_some_and(|end| end <= limit.get())
    };
    match fits(rect.x, rect.width, size[0]) && fits(rect.y, rect.height, size[1]) {
        true => Ok(()),
        false => Err(RhiError::invalid(
            "scissor",
            format!("{rect:?} is outside the {}×{} targets", size[0], size[1]),
        )),
    }
}
