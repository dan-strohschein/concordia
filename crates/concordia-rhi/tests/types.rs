//! Backend-neutral RHI types: formats, descriptor validation, barriers, command lists (M1 phase 1.2b).

use std::num::{NonZeroU32, NonZeroU64};

use concordia_rhi::{
    Access, AccessFlags, BufferDesc, BufferUsage, ClearValue, ColorAttachment, Command,
    CommandList, Format, Layout, LoadOp, MemoryLocation, RenderPassDesc, SampleCount, Stages,
    StoreOp, Texture, TextureDesc, TextureUsage,
};
use proptest::prelude::*;

fn nz(x: u32) -> NonZeroU32 {
    NonZeroU32::new(x).unwrap()
}

fn tex(width: u32, height: u32) -> TextureDesc {
    TextureDesc {
        width: nz(width),
        height: nz(height),
        mip_levels: nz(1),
        samples: SampleCount::X1,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC,
        label: "t".into(),
    }
}

#[test]
fn format_table_block_sizes_are_correct() {
    assert_eq!(Format::R8Unorm.bytes_per_pixel(), 1);
    assert_eq!(Format::Rgba8Unorm.bytes_per_pixel(), 4);
    assert_eq!(Format::Rgba8UnormSrgb.bytes_per_pixel(), 4);
    assert_eq!(Format::Bgra8UnormSrgb.bytes_per_pixel(), 4);
    assert_eq!(Format::Rgba16Float.bytes_per_pixel(), 8);
    assert_eq!(Format::Rgba32Float.bytes_per_pixel(), 16);
    assert_eq!(Format::R32Uint.bytes_per_pixel(), 4);
    assert_eq!(Format::R64Uint.bytes_per_pixel(), 8);
    assert_eq!(Format::Depth32Float.bytes_per_pixel(), 4);
    assert!(Format::Depth32Float.is_depth() && !Format::Rgba8Unorm.is_depth());
    assert!(Format::Rgba8UnormSrgb.is_srgb() && !Format::Rgba8Unorm.is_srgb());
}

#[test]
fn texture_desc_validation_rejects_bad_combinations() {
    assert!(tex(256, 256).validate().is_ok());
    let mut too_many_mips = tex(8, 8);
    too_many_mips.mip_levels = nz(5);
    assert!(
        too_many_mips
            .validate()
            .unwrap_err()
            .to_string()
            .contains("mip")
    );
    let mut msaa_mips = tex(8, 8);
    msaa_mips.samples = SampleCount::X4;
    msaa_mips.mip_levels = nz(2);
    assert!(msaa_mips.validate().is_err());
    let mut huge = tex(1, 1);
    huge.width = nz(32_768);
    assert!(huge.validate().is_err());
    let mut depth_storage = tex(4, 4);
    depth_storage.format = Format::Depth32Float;
    depth_storage.usage = TextureUsage::STORAGE;
    assert!(depth_storage.validate().is_err());
    let mut no_usage = tex(4, 4);
    no_usage.usage = TextureUsage::empty();
    assert!(no_usage.validate().is_err());
}

#[test]
fn buffer_desc_validation_rejects_empty_usage_and_huge_sizes() {
    let ok = BufferDesc {
        size: NonZeroU64::new(1024).unwrap(),
        usage: BufferUsage::STORAGE,
        memory: MemoryLocation::GpuOnly,
        label: "b".into(),
    };
    assert!(ok.validate().is_ok());
    let mut empty = ok.clone();
    empty.usage = BufferUsage::empty();
    assert!(empty.validate().is_err());
    let mut huge = ok;
    huge.size = NonZeroU64::new(1 << 40).unwrap();
    assert!(huge.validate().is_err());
}

fn access_strategy() -> impl Strategy<Value = Access> {
    (any::<u32>(), any::<u32>(), 0..Layout::ALL.len()).prop_map(|(s, a, l)| Access {
        stages: Stages::from_bits_truncate(s),
        access: AccessFlags::from_bits_truncate(a),
        layout: Layout::ALL[l],
    })
}

proptest! {
    #[test]
    fn barrier_access_merge_is_associative(a in access_strategy(), b in access_strategy(), c in access_strategy()) {
        prop_assert_eq!(a.merge(b).merge(c), a.merge(b.merge(c)));
    }
}

#[test]
fn command_list_validation_catches_structure_errors() {
    let target = Texture::from_raw(0, 1);
    let pass = RenderPassDesc {
        colors: vec![ColorAttachment {
            texture: target,
            resolve: None,
            load: LoadOp::Clear(ClearValue::Color([0.0, 0.0, 0.0, 1.0])),
            store: StoreOp::Store,
        }],
        depth: None,
    };
    let mut ok = CommandList::new("ok");
    ok.push(Command::BeginRenderPass(pass.clone()));
    ok.push(Command::Draw {
        vertices: 0..3,
        instances: 0..1,
    });
    ok.push(Command::EndRenderPass);
    ok.push(Command::Dispatch { groups: [1, 1, 1] });
    assert!(ok.validate().is_ok());

    let mut draw_outside = CommandList::new("draw outside");
    draw_outside.push(Command::Draw {
        vertices: 0..3,
        instances: 0..1,
    });
    assert!(
        draw_outside
            .validate()
            .unwrap_err()
            .to_string()
            .contains("render pass")
    );

    let mut unclosed = CommandList::new("unclosed");
    unclosed.push(Command::BeginRenderPass(pass.clone()));
    assert!(unclosed.validate().is_err());

    let mut dispatch_inside = CommandList::new("dispatch inside");
    dispatch_inside.push(Command::BeginRenderPass(pass));
    dispatch_inside.push(Command::Dispatch { groups: [1, 1, 1] });
    dispatch_inside.push(Command::EndRenderPass);
    assert!(dispatch_inside.validate().is_err());
}
