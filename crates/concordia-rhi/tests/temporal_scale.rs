//! The temporal scaler (M4 4.6b, ADR-0103): its descriptor, the `TemporalScale` command's
//! structure, and the shared check of its textures against the scaler.

use std::num::NonZeroU32;

use concordia_rhi::{
    ClearValue, ColorAttachment, Command, CommandList, Format, LoadOp, RenderPassDesc, SampleCount,
    StoreOp, TemporalScale, TemporalScaler, TemporalScalerDesc, Texture, TextureDesc, TextureUsage,
    check,
};

fn nz(x: u32) -> NonZeroU32 {
    NonZeroU32::new(x).unwrap()
}

fn desc() -> TemporalScalerDesc {
    TemporalScalerDesc {
        input: [nz(64), nz(36)],
        output: [nz(96), nz(54)],
        color: Format::Rgba16Float,
        depth: Format::Depth32Float,
        motion: Format::Rg16Float,
        reactive: Some(Format::R8Unorm),
        output_format: Format::Rgba16Float,
    }
}

#[test]
fn temporal_scaler_desc_is_validated() {
    desc().validate().expect("a 1.5× scaler");
    let bad = |change: fn(&mut TemporalScalerDesc), why: &str| {
        let mut d = desc();
        change(&mut d);
        let err = d.validate().expect_err(why);
        assert!(err.to_string().contains(why), "{why}: {err}");
    };
    bad(|d| d.output = [nz(32), nz(54)], "smaller than the input");
    bad(|d| d.depth = Format::R32Float, "depth");
    bad(|d| d.motion = Format::Rgba8Unorm, "motion");
    bad(|d| d.reactive = Some(Format::Rgba16Float), "reactive");
    bad(|d| d.color = Format::Depth32Float, "color");
    bad(|d| d.output_format = Format::R8Unorm, "output");
}

fn scale() -> TemporalScale {
    let t = |i| Texture::from_raw(i, 1);
    TemporalScale {
        scaler: TemporalScaler::from_raw(0, 1),
        color: t(1),
        depth: t(2),
        motion: t(3),
        reactive: Some(t(4)),
        output: t(5),
        jitter: [0.25, -0.125],
        motion_scale: [-64.0, -36.0],
        pre_exposure: 1.0,
        reset: true,
    }
}

fn list(commands: Vec<Command>) -> CommandList {
    CommandList {
        label: "temporal".into(),
        commands,
    }
}

#[test]
fn temporal_scale_is_validated() {
    list(vec![Command::TemporalScale(scale())])
        .validate()
        .expect("outside a pass");
    let pass = Command::BeginRenderPass(RenderPassDesc {
        colors: vec![ColorAttachment {
            texture: Texture::from_raw(9, 1),
            resolve: None,
            load: LoadOp::Clear(ClearValue::Color([0.0; 4])),
            store: StoreOp::Store,
        }],
        depth: None,
    });
    let err = list(vec![
        pass,
        Command::TemporalScale(scale()),
        Command::EndRenderPass,
    ])
    .validate()
    .expect_err("inside a pass");
    assert!(err.to_string().contains("render pass"), "{err}");
    let bad = |change: fn(&mut TemporalScale), why: &str| {
        let mut s = scale();
        change(&mut s);
        let err = list(vec![Command::TemporalScale(s)])
            .validate()
            .expect_err(why);
        assert!(err.to_string().contains(why), "{why}: {err}");
    };
    bad(|s| s.jitter = [f32::NAN, 0.0], "jitter");
    bad(|s| s.jitter = [0.0, 1.5], "jitter");
    bad(|s| s.motion_scale = [f32::INFINITY, 1.0], "motion scale");
    bad(|s| s.pre_exposure = 0.0, "pre-exposure");
}

fn texture_desc(size: [u32; 2], format: Format, usage: TextureUsage) -> TextureDesc {
    TextureDesc {
        width: nz(size[0]),
        height: nz(size[1]),
        mip_levels: nz(1),
        samples: SampleCount::X1,
        format,
        usage,
        label: "t".into(),
    }
}

#[test]
fn temporal_scale_textures_must_match_the_scaler() {
    let read = TextureUsage::SAMPLED;
    let color = texture_desc([64, 36], Format::Rgba16Float, read);
    let depth = texture_desc(
        [64, 36],
        Format::Depth32Float,
        read | TextureUsage::RENDER_TARGET,
    );
    let motion = texture_desc([64, 36], Format::Rg16Float, read);
    let reactive = texture_desc([64, 36], Format::R8Unorm, read);
    let output = texture_desc([96, 54], Format::Rgba16Float, TextureUsage::STORAGE);
    let textures = check::TemporalTextures {
        color: &color,
        depth: &depth,
        motion: &motion,
        reactive: Some(&reactive),
        output: &output,
    };
    check::temporal_scale(&desc(), &textures).expect("matching textures");

    let wrong_size = texture_desc([63, 36], Format::Rgba16Float, read);
    let err = check::temporal_scale(
        &desc(),
        &check::TemporalTextures {
            color: &wrong_size,
            ..textures
        },
    )
    .expect_err("a color of the wrong size");
    assert!(err.to_string().contains("color"), "{err}");
    let wrong_format = texture_desc([96, 54], Format::Rgba8Unorm, TextureUsage::STORAGE);
    let err = check::temporal_scale(
        &desc(),
        &check::TemporalTextures {
            output: &wrong_format,
            ..textures
        },
    )
    .expect_err("an output of the wrong format");
    assert!(err.to_string().contains("output"), "{err}");
    let unreadable = texture_desc([64, 36], Format::Rg16Float, TextureUsage::COPY_DST);
    let err = check::temporal_scale(
        &desc(),
        &check::TemporalTextures {
            motion: &unreadable,
            ..textures
        },
    )
    .expect_err("a motion texture shaders can't read");
    assert!(err.to_string().contains("SAMPLED"), "{err}");
    let err = check::temporal_scale(
        &desc(),
        &check::TemporalTextures {
            reactive: None,
            ..textures
        },
    )
    .expect_err("a scaler with a reactive mask needs one");
    assert!(err.to_string().contains("reactive"), "{err}");
    let mut multisampled = color.clone();
    multisampled.samples = SampleCount::X4;
    let err = check::temporal_scale(
        &desc(),
        &check::TemporalTextures {
            color: &multisampled,
            ..textures
        },
    )
    .expect_err("a multisampled input");
    assert!(err.to_string().contains("single-sample"), "{err}");
}
