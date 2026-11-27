//! Attachment-less render passes and target-less pipelines (M4 4.7a): fragments write storage
//! buffers over an explicit extent, with no colour or depth target.

use std::num::NonZeroU32;

use concordia_rhi::{
    ClearValue, ColorAttachment, Command, CommandList, Format, LoadOp, RenderPassDesc, StoreOp,
    Texture, pipeline_targets,
};

fn nz(x: u32) -> NonZeroU32 {
    NonZeroU32::new(x).unwrap()
}

fn colour() -> ColorAttachment {
    ColorAttachment {
        texture: Texture::from_raw(1, 1),
        resolve: None,
        load: LoadOp::Clear(ClearValue::Color([0.0; 4])),
        store: StoreOp::Store,
    }
}

fn list(desc: RenderPassDesc) -> CommandList {
    let mut list = CommandList::new("t");
    list.push(Command::BeginRenderPass(desc))
        .push(Command::EndRenderPass);
    list
}

#[test]
fn render_pass_without_attachments_requires_extent() {
    let bare = RenderPassDesc {
        colors: vec![],
        depth: None,
        extent: None,
    };
    let err = list(bare).validate().unwrap_err().to_string();
    assert!(err.contains("extent"), "{err}");

    let sized = RenderPassDesc {
        colors: vec![],
        depth: None,
        extent: Some([nz(16_384), nz(16_384)]),
    };
    assert!(list(sized).validate().is_ok());

    let both = RenderPassDesc {
        colors: vec![colour()],
        depth: None,
        extent: Some([nz(64), nz(64)]),
    };
    let err = list(both).validate().unwrap_err().to_string();
    assert!(err.contains("extent"), "{err}");
}

#[test]
fn pipelines_without_targets_need_a_fragment_stage() {
    assert!(
        pipeline_targets(&[], None, false).is_err(),
        "nothing to write"
    );
    assert!(
        pipeline_targets(&[], None, true).is_ok(),
        "fragments write storage"
    );
    assert!(pipeline_targets(&[Format::Rgba8Unorm], None, false).is_ok());
    assert!(pipeline_targets(&[Format::Rgba8Unorm; 9], None, true).is_err());
}
