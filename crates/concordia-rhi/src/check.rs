//! Checks every backend runs against its live resources before recording a command.

use std::num::NonZeroU32;

use crate::caps::SampleCount;
use crate::command::{Rect, TextureOrigin, Viewport};
use crate::error::RhiError;
use crate::handle::Texture;
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

/// A multisampled depth attachment `source` can resolve into `target`: a single-sample texture
/// of its size and depth format with `RENDER_TARGET` usage, on a device with min resolves.
pub fn depth_resolve(
    source: &TextureDesc,
    target: &TextureDesc,
    min_supported: bool,
) -> Result<(), RhiError> {
    let invalid = |why: String| RhiError::invalid("depth resolve", why);
    if !min_supported {
        return Err(invalid("the device has no min depth resolves".into()));
    }
    if !source.format.is_depth() {
        return Err(invalid(format!(
            "{:?} is not a depth format",
            source.format
        )));
    }
    if source.samples == SampleCount::X1 {
        return Err(invalid(format!(
            "{:?} needs a multisampled source",
            source.label
        )));
    }
    if target.samples != SampleCount::X1 {
        return Err(invalid(format!(
            "{:?} needs a single-sample target",
            target.label
        )));
    }
    if (target.width, target.height, target.format) != (source.width, source.height, source.format)
    {
        return Err(invalid(format!(
            "{:?} and {:?} differ in size or format",
            source.label, target.label
        )));
    }
    if !target.usage.contains(TextureUsage::RENDER_TARGET) {
        return Err(invalid(format!(
            "{:?} lacks RENDER_TARGET usage",
            target.label
        )));
    }
    Ok(())
}

/// One side of a texture copy: the texture, its descriptor and the region's corner.
#[derive(Debug, Clone, Copy)]
pub struct CopySide<'a> {
    /// The texture.
    pub texture: Texture,
    /// Its descriptor.
    pub desc: &'a TextureDesc,
    /// The region's corner and mip.
    pub origin: TextureOrigin,
}

/// A copy of `extent` from `src` to `dst` stays inside both mips, between single-sample, non-depth
/// textures of one format with the copy usages, without overlapping itself.
pub fn texture_copy(
    from: CopySide<'_>,
    to: CopySide<'_>,
    extent: [NonZeroU32; 2],
) -> Result<(), RhiError> {
    let (src, src_origin, dst, dst_origin) = (from.desc, from.origin, to.desc, to.origin);
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
    region_in(dst, dst_origin, extent).map_err(invalid)?;
    if from.texture == to.texture && overlap(src_origin, dst_origin, extent) {
        return Err(invalid(format!(
            "regions at {src_origin:?} and {dst_origin:?} overlap in {:?}",
            src.label
        )));
    }
    Ok(())
}

/// Two same-sized regions of one mip share a texel.
fn overlap(a: TextureOrigin, b: TextureOrigin, extent: [NonZeroU32; 2]) -> bool {
    let apart = |p: u32, q: u32, len: NonZeroU32| p.abs_diff(q) >= len.get();
    a.mip == b.mip && !apart(a.x, b.x, extent[0]) && !apart(a.y, b.y, extent[1])
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

/// A temporal scale's textures, as their descriptors.
#[derive(Debug, Clone, Copy)]
pub struct TemporalTextures<'a> {
    /// This frame's color.
    pub color: &'a TextureDesc,
    /// This frame's depth.
    pub depth: &'a TextureDesc,
    /// Motion vectors.
    pub motion: &'a TextureDesc,
    /// The reactive mask.
    pub reactive: Option<&'a TextureDesc>,
    /// The result.
    pub output: &'a TextureDesc,
}

/// Each texture is single-sample, of the scaler's size and format; inputs are `SAMPLED` and the
/// output `STORAGE`; a reactive mask is given exactly when the scaler reads one.
pub fn temporal_scale(
    desc: &crate::TemporalScalerDesc,
    textures: &TemporalTextures<'_>,
) -> Result<(), RhiError> {
    let bad = |reason: String| Err(RhiError::invalid("temporal scale", reason));
    match (desc.reactive, textures.reactive) {
        (Some(_), None) => return bad("the scaler reads a reactive mask; none was given".into()),
        (None, Some(_)) => return bad("the scaler has no reactive mask; one was given".into()),
        _ => {}
    }
    let (input, output) = (desc.input, desc.output);
    let read = TextureUsage::SAMPLED;
    let sides = [
        ("color", Some(textures.color), input, Some(desc.color), read),
        ("depth", Some(textures.depth), input, Some(desc.depth), read),
        (
            "motion",
            Some(textures.motion),
            input,
            Some(desc.motion),
            read,
        ),
        ("reactive", textures.reactive, input, desc.reactive, read),
        (
            "output",
            Some(textures.output),
            output,
            Some(desc.output_format),
            TextureUsage::STORAGE,
        ),
    ];
    for (name, texture, size, format, usage) in sides {
        let (Some(t), Some(format)) = (texture, format) else {
            continue;
        };
        if t.samples != SampleCount::X1 {
            return bad(format!("{name} {:?} must be single-sample", t.label));
        }
        if [t.width, t.height] != size {
            return bad(format!(
                "{name} {:?} is {}×{}; the scaler takes {}×{}",
                t.label, t.width, t.height, size[0], size[1]
            ));
        }
        if t.format != format {
            return bad(format!(
                "{name} {:?} is {:?}; the scaler takes {format:?}",
                t.label, t.format
            ));
        }
        if !t.usage.contains(usage) {
            return bad(format!("{name} {:?} needs {usage:?} usage", t.label));
        }
    }
    Ok(())
}
