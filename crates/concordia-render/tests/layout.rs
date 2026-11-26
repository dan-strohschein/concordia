//! Rust mirrors of shader structs match Slang's reflected layout.

use concordia_render::{
    CompactPush, CullPush, DrawPush, HzbPush, MeshletPush, ScanAddPush, ScanPush, ShadowPush,
    TaskPush, shader_dir,
};
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
}

#[test]
fn cull_pushes_match_their_shaders() {
    check_push_in::<CullPush>("meshlet.slang", "meshlet_cull", Stage::Compute);
    check_push_in::<CompactPush>("meshlet.slang", "meshlet_compact", Stage::Compute);
    check_push_in::<ScanPush>("scan.slang", "scan_groups", Stage::Compute);
    check_push_in::<ScanAddPush>("scan.slang", "scan_add", Stage::Compute);
    check_push_in::<HzbPush>("hzb.slang", "hzb_build", Stage::Compute);
}

#[test]
fn task_push_matches_the_mesh_stages() {
    check_push_in::<TaskPush>("meshlet.slang", "meshlet_object", Stage::Amplification);
    check_push_in::<TaskPush>("meshlet.slang", "meshlet_mesh", Stage::Mesh);
    check_push_in::<TaskPush>("meshlet.slang", "meshlet_shadow_mesh", Stage::Mesh);
}

/// `fs` reads its push constants as a `DrawPush` after the mesh stage too.
#[test]
fn task_push_keeps_the_fields_fs_reads_where_draw_push_has_them() {
    let (draw, task) = (DrawPush::layout(), TaskPush::layout());
    let offset = |l: &concordia_shaders::Layout, name: &str| {
        l.fields.iter().find(|f| f.name == name).map(|f| f.offset)
    };
    for field in ["frame", "materials", "material"] {
        assert_eq!(offset(&task, field), offset(&draw, field), "{field}");
    }
    let push = TaskPush {
        frame: 1,
        instances: 2,
        items: 3,
        materials: 4,
        material: 5,
        meshes: 6,
        first: 7,
        count: 8,
        item_count: 9,
        views: 10,
        codes: 11,
        cull: 12,
        cascade: 13,
        pass: 14,
        visibility: 15,
    };
    let words: Vec<u8> = (1u32..=15).flat_map(|w| w.to_le_bytes()).collect();
    assert_eq!(push.bytes(), words);
    assert_eq!(push.bytes().len(), TaskPush::layout().size);
    assert!(push.bytes().len() <= concordia_rhi::MAX_PUSH_CONSTANTS);
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
    let cull = CullPush {
        items: 1,
        item_count: 2,
        buckets: 3,
        views: 4,
        instances: 5,
        meshes: 6,
        codes: 7,
        drawn: 8,
        total: 9,
        pass: 10,
        visibility: 11,
    };
    let eleven: Vec<u8> = (1u32..=11).flat_map(|w| w.to_le_bytes()).collect();
    assert_eq!(cull.bytes(), eleven);
    assert_eq!(cull.bytes().len(), CullPush::layout().size);
    let compact = CompactPush {
        items: 1,
        item_count: 2,
        buckets: 3,
        drawn: 4,
        sums: 5,
        entries: 6,
        args: 7,
        total: 8,
    };
    assert_eq!(compact.bytes(), bytes);
    assert_eq!(compact.bytes().len(), CompactPush::layout().size);
}
