//! Indirect draws, viewport and scissor, texture copies (M4 4.3a, D5): command-list structure and
//! the shared checks both backends run against their resources.

use std::num::{NonZeroU32, NonZeroU64};

use concordia_rhi::{
    BufferDesc, BufferUsage, ClearValue, ColorAttachment, Command, CommandList, Format, LoadOp,
    MemoryLocation, Rect, RenderPassDesc, SampleCount, StoreOp, Texture, TextureDesc,
    TextureOrigin, TextureUsage, Viewport, check,
};

fn nz(x: u32) -> NonZeroU32 {
    NonZeroU32::new(x).unwrap()
}

fn texture() -> Texture {
    Texture::from_raw(1, 1)
}

fn buffer() -> concordia_rhi::Buffer {
    concordia_rhi::Buffer::from_raw(2, 1)
}

fn begin() -> Command {
    Command::BeginRenderPass(RenderPassDesc {
        colors: vec![ColorAttachment {
            texture: texture(),
            resolve: None,
            load: LoadOp::Clear(ClearValue::Color([0.0; 4])),
            store: StoreOp::Store,
        }],
        depth: None,
    })
}

fn in_pass(command: Command) -> CommandList {
    let mut list = CommandList::new("t");
    list.push(begin())
        .push(command)
        .push(Command::EndRenderPass);
    list
}

fn alone(command: Command) -> CommandList {
    let mut list = CommandList::new("t");
    list.push(command);
    list
}

fn viewport(x: f32, y: f32, width: f32, height: f32) -> Viewport {
    Viewport {
        x,
        y,
        width,
        height,
        min_depth: 0.0,
        max_depth: 1.0,
    }
}

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width: nz(width),
        height: nz(height),
    }
}

fn copy() -> Command {
    Command::CopyTexture {
        src: texture(),
        src_origin: TextureOrigin::default(),
        dst: texture(),
        dst_origin: TextureOrigin::default(),
        extent: [nz(4), nz(4)],
    }
}

#[test]
fn indirect_draws_must_be_inside_a_pass_with_aligned_offsets() {
    let draw = |offset| Command::DrawIndirect {
        args: buffer(),
        offset,
    };
    let indexed = |offset| Command::DrawIndexedIndirect {
        indices: buffer(),
        args: buffer(),
        offset,
    };
    assert!(in_pass(draw(0)).validate().is_ok());
    assert!(in_pass(draw(16)).validate().is_ok());
    assert!(in_pass(indexed(20)).validate().is_ok());
    for command in [draw(0), indexed(0)] {
        let err = alone(command).validate().unwrap_err().to_string();
        assert!(err.contains("inside a render pass"), "{err}");
    }
    for command in [draw(6), indexed(2)] {
        let err = in_pass(command).validate().unwrap_err().to_string();
        assert!(err.contains("multiple of 4"), "{err}");
    }
}

#[test]
fn viewport_and_scissor_are_inside_passes_finite_and_non_empty() {
    assert!(
        in_pass(Command::SetViewport(viewport(0.0, 0.0, 8.0, 8.0)))
            .validate()
            .is_ok()
    );
    assert!(
        in_pass(Command::SetScissor(rect(0, 0, 8, 8)))
            .validate()
            .is_ok()
    );
    for command in [
        Command::SetViewport(viewport(0.0, 0.0, 8.0, 8.0)),
        Command::SetScissor(rect(0, 0, 8, 8)),
    ] {
        let err = alone(command).validate().unwrap_err().to_string();
        assert!(err.contains("inside a render pass"), "{err}");
    }
    let bad = [
        viewport(f32::NAN, 0.0, 8.0, 8.0),
        viewport(0.0, f32::INFINITY, 8.0, 8.0),
        viewport(-1.0, 0.0, 8.0, 8.0),
        viewport(0.0, 0.0, 0.0, 8.0),
        viewport(0.0, 0.0, 8.0, -2.0),
        Viewport {
            min_depth: 0.5,
            max_depth: 0.25,
            ..viewport(0.0, 0.0, 8.0, 8.0)
        },
        Viewport {
            max_depth: 1.5,
            ..viewport(0.0, 0.0, 8.0, 8.0)
        },
    ];
    for v in bad {
        let err = in_pass(Command::SetViewport(v))
            .validate()
            .unwrap_err()
            .to_string();
        assert!(err.contains("viewport"), "{v:?}: {err}");
    }
}

#[test]
fn copy_texture_is_outside_passes() {
    assert!(alone(copy()).validate().is_ok());
    let err = in_pass(copy()).validate().unwrap_err().to_string();
    assert!(err.contains("outside render passes"), "{err}");
}

fn args_buffer(size: u64, usage: BufferUsage) -> BufferDesc {
    BufferDesc {
        size: NonZeroU64::new(size).unwrap(),
        usage,
        memory: MemoryLocation::GpuOnly,
        label: "args".into(),
    }
}

#[test]
fn indirect_arguments_must_fit_an_indirect_buffer() {
    let args = args_buffer(64, BufferUsage::INDIRECT | BufferUsage::STORAGE);
    assert!(check::indirect_args(&args, 0, 16).is_ok());
    assert!(check::indirect_args(&args, 48, 16).is_ok());
    assert!(check::indirect_args(&args, 44, 20).is_ok());
    let err = check::indirect_args(&args, 52, 16).unwrap_err().to_string();
    assert!(err.contains("past the end"), "{err}");
    assert!(check::indirect_args(&args, u64::MAX - 4, 16).is_err());
    let storage = args_buffer(64, BufferUsage::STORAGE);
    let err = check::indirect_args(&storage, 0, 16)
        .unwrap_err()
        .to_string();
    assert!(err.contains("INDIRECT"), "{err}");
}

fn tex(width: u32, height: u32, mips: u32, format: Format, samples: SampleCount) -> TextureDesc {
    TextureDesc {
        width: nz(width),
        height: nz(height),
        mip_levels: nz(mips),
        samples,
        format,
        usage: TextureUsage::COPY_SRC | TextureUsage::COPY_DST | TextureUsage::SAMPLED,
        label: "t".into(),
    }
}

fn origin(mip: u32, x: u32, y: u32) -> TextureOrigin {
    TextureOrigin { mip, x, y }
}

#[test]
fn texture_copies_stay_inside_matching_single_sample_textures() {
    let a = tex(16, 16, 3, Format::Rgba8Unorm, SampleCount::X1);
    let b = tex(8, 8, 1, Format::Rgba8Unorm, SampleCount::X1);
    let ok = |src: &TextureDesc, s, dst: &TextureDesc, d, e: [u32; 2]| {
        // Each descriptor variable stands for one texture.
        let handle = |desc: &TextureDesc| Texture::from_raw(std::ptr::from_ref(desc) as u32, 0);
        let side = |desc, origin| check::CopySide {
            texture: handle(desc),
            desc,
            origin,
        };
        check::texture_copy(side(src, s), side(dst, d), [nz(e[0]), nz(e[1])])
    };
    assert!(ok(&a, origin(0, 8, 8), &b, origin(0, 0, 0), [8, 8]).is_ok());
    assert!(ok(&a, origin(1, 0, 0), &b, origin(0, 0, 0), [8, 8]).is_ok());
    assert!(ok(&a, origin(2, 2, 2), &a, origin(0, 14, 14), [2, 2]).is_ok());
    let fails = |result: Result<(), concordia_rhi::RhiError>, why: &str| {
        let err = result.unwrap_err().to_string();
        assert!(err.contains(why), "expected {why:?}: {err}");
    };
    fails(
        ok(&a, origin(0, 9, 0), &b, origin(0, 0, 0), [8, 8]),
        "outside",
    );
    fails(
        ok(&a, origin(0, 0, 0), &b, origin(0, 1, 0), [8, 8]),
        "outside",
    );
    fails(
        ok(&a, origin(2, 0, 0), &b, origin(0, 0, 0), [8, 8]),
        "outside",
    );
    fails(ok(&a, origin(3, 0, 0), &b, origin(0, 0, 0), [1, 1]), "mip");
    fails(
        ok(&a, origin(0, 0, 0), &a, origin(0, 3, 3), [4, 4]),
        "overlap",
    );
    assert!(ok(&a, origin(0, 0, 0), &a, origin(0, 4, 0), [4, 4]).is_ok());
    fails(
        ok(&a, origin(0, u32::MAX, 0), &b, origin(0, 0, 0), [8, 8]),
        "outside",
    );
    let srgb = tex(8, 8, 1, Format::Rgba8UnormSrgb, SampleCount::X1);
    fails(
        ok(&a, origin(0, 0, 0), &srgb, origin(0, 0, 0), [8, 8]),
        "format",
    );
    let msaa = tex(8, 8, 1, Format::Rgba8Unorm, SampleCount::X4);
    fails(
        ok(&msaa, origin(0, 0, 0), &b, origin(0, 0, 0), [8, 8]),
        "single-sample",
    );
    let depth = tex(8, 8, 1, Format::Depth32Float, SampleCount::X1);
    fails(
        ok(&depth, origin(0, 0, 0), &depth, origin(0, 0, 0), [8, 8]),
        "depth",
    );
    let mut no_src = b.clone();
    no_src.usage = TextureUsage::COPY_DST;
    fails(
        ok(&no_src, origin(0, 0, 0), &b, origin(0, 0, 0), [8, 8]),
        "COPY_SRC",
    );
    let mut no_dst = b.clone();
    no_dst.usage = TextureUsage::COPY_SRC;
    fails(
        ok(&b, origin(0, 0, 0), &no_dst, origin(0, 0, 0), [8, 8]),
        "COPY_DST",
    );
}

#[test]
fn viewports_and_scissors_must_lie_inside_the_pass() {
    let size = [nz(64), nz(32)];
    assert!(check::viewport_in(&viewport(0.0, 0.0, 64.0, 32.0), size).is_ok());
    assert!(check::viewport_in(&viewport(32.0, 16.0, 32.0, 16.0), size).is_ok());
    let err = check::viewport_in(&viewport(33.0, 0.0, 32.0, 16.0), size)
        .unwrap_err()
        .to_string();
    assert!(err.contains("outside"), "{err}");
    assert!(check::viewport_in(&viewport(0.0, 0.0, 64.0, 33.0), size).is_err());
    assert!(check::scissor_in(&rect(0, 0, 64, 32), size).is_ok());
    assert!(check::scissor_in(&rect(60, 30, 4, 2), size).is_ok());
    assert!(check::scissor_in(&rect(61, 0, 4, 2), size).is_err());
    assert!(check::scissor_in(&rect(0, 31, 4, 2), size).is_err());
    assert!(check::scissor_in(&rect(u32::MAX, 0, 4, 2), size).is_err());
}

fn depth_desc(
    width: u32,
    format: Format,
    samples: SampleCount,
    usage: TextureUsage,
) -> TextureDesc {
    TextureDesc {
        width: nz(width),
        height: nz(width / 2),
        mip_levels: nz(1),
        samples,
        format,
        usage,
        label: "depth".into(),
    }
}

/// A depth resolve (M4 4.4b) takes a multisampled depth attachment into a single-sample texture of
/// its size and format with `RENDER_TARGET` usage, and needs the device's min resolve.
#[test]
fn depth_resolve_needs_a_single_sample_depth_target() {
    let rt = TextureUsage::RENDER_TARGET;
    let source = depth_desc(64, Format::Depth32Float, SampleCount::X4, rt);
    let target = depth_desc(
        64,
        Format::Depth32Float,
        SampleCount::X1,
        rt | TextureUsage::SAMPLED,
    );
    assert!(check::depth_resolve(&source, &target, true).is_ok());
    let fails = |result: Result<(), concordia_rhi::RhiError>, why: &str| {
        let err = result.unwrap_err().to_string();
        assert!(err.contains(why), "expected {why:?}: {err}");
    };
    fails(
        check::depth_resolve(&source, &target, false),
        "min depth resolves",
    );
    fails(
        check::depth_resolve(&target, &target, true),
        "multisampled source",
    );
    fails(
        check::depth_resolve(&source, &source, true),
        "single-sample target",
    );
    let small = depth_desc(32, Format::Depth32Float, SampleCount::X1, rt);
    fails(
        check::depth_resolve(&source, &small, true),
        "size or format",
    );
    let color_source = depth_desc(64, Format::Rgba8Unorm, SampleCount::X4, rt);
    let color_target = depth_desc(64, Format::Rgba8Unorm, SampleCount::X1, rt);
    fails(
        check::depth_resolve(&color_source, &color_target, true),
        "not a depth format",
    );
    fails(
        check::depth_resolve(&source, &color_target, true),
        "size or format",
    );
    let sampled_only = depth_desc(
        64,
        Format::Depth32Float,
        SampleCount::X1,
        TextureUsage::SAMPLED,
    );
    fails(
        check::depth_resolve(&source, &sampled_only, true),
        "RENDER_TARGET",
    );
}
