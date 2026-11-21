//! Rust mirrors of shader structs match Slang's reflected layout.

use concordia_render::{DrawPush, ShadowPush, shader_dir};
use concordia_shaders::{
    CompileRequest, ShaderCompiler, ShaderStruct, SlangToolchain, SlangcCompiler, Stage, Target,
    check_layout,
};

/// Checks `T` against the push constants of `forward.slang`'s vertex entry point `entry`.
fn check_push<T: ShaderStruct>(entry: &str) {
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    for target in [Target::Msl, Target::SpirV] {
        let module = compiler
            .compile(&CompileRequest {
                path: shader_dir().join("forward.slang"),
                entry: entry.into(),
                stage: Stage::Vertex,
                target,
                defines: Default::default(),
                include_dirs: vec![concordia_shaders::builtin_dir()],
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
