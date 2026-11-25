//! Rust mirrors of shader structs match Slang's reflected layout.

use concordia_render::{DrawPush, ExpandPush, MeshletPush, ShadowPush, shader_dir};
use concordia_shaders::{
    CompileRequest, ShaderCompiler, ShaderStruct, SlangToolchain, SlangcCompiler, Stage, Target,
    check_layout,
};

/// Checks `T` against the push constants of `forward.slang`'s vertex entry point `entry`.
fn check_push<T: ShaderStruct>(entry: &str) {
    check_push_in::<T>("forward.slang", entry, Stage::Vertex);
}

/// Checks `T` against the push constants of entry point `entry` of `file`.
fn check_push_in<T: ShaderStruct>(file: &str, entry: &str, stage: Stage) {
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    for target in [Target::Msl, Target::SpirV] {
        let module = compiler
            .compile(&CompileRequest {
                path: shader_dir().join(file),
                entry: entry.into(),
                stage,
                target,
                defines: Default::default(),
                include_dirs: concordia_render::include_dirs(),
            })
            .unwrap();
        check_layout::<T>(&module.reflection.push.expect("push constants")).unwrap();
    }
}

#[test]
fn draw_push_matches_forward_slang() {
    check_push::<DrawPush>("vs");
    assert!(DrawPush::default().bytes().len() <= concordia_rhi::MAX_PUSH_CONSTANTS);
}

#[test]
fn shadow_push_matches_forward_slang() {
    check_push::<ShadowPush>("shadow_vs");
}

#[test]
fn meshlet_push_matches_meshlet_slang() {
    check_push_in::<MeshletPush>("meshlet.slang", "meshlet_vs", Stage::Vertex);
    check_push_in::<MeshletPush>("meshlet.slang", "meshlet_shadow_vs", Stage::Vertex);
    check_push_in::<ExpandPush>("meshlet.slang", "meshlet_expand", Stage::Compute);
}

/// `fs` reads its push constants as a `DrawPush` whichever vertex shader ran.
#[test]
fn meshlet_push_keeps_the_fields_fs_reads_where_draw_push_has_them() {
    let (draw, meshlet) = (DrawPush::layout(), MeshletPush::layout());
    let offset = |l: &concordia_shaders::Layout, name: &str| {
        l.fields.iter().find(|f| f.name == name).map(|f| f.offset)
    };
    for field in ["frame", "materials", "material"] {
        assert_eq!(offset(&meshlet, field), offset(&draw, field), "{field}");
        assert!(offset(&draw, field).is_some(), "{field}");
    }
    assert!(MeshletPush::default().bytes().len() <= concordia_rhi::MAX_PUSH_CONSTANTS);
}

#[test]
fn meshlet_push_bytes_follow_the_layout() {
    let push = MeshletPush {
        frame: 1,
        instances: 2,
        entries: 3,
        materials: 4,
        material: 5,
        meshes: 6,
        first: 7,
        cascade: 8,
    };
    let words: Vec<u32> = (1..=8).collect();
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    assert_eq!(push.bytes(), bytes);
    assert_eq!(push.bytes().len(), MeshletPush::layout().size);
    let expand = ExpandPush {
        items: 1,
        item_count: 2,
        buckets: 3,
        entries: 4,
        args: 5,
        total: 6,
    };
    assert_eq!(expand.bytes(), bytes[..24]);
    assert_eq!(expand.bytes().len(), ExpandPush::layout().size);
}
