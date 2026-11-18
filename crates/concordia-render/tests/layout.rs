//! Rust mirrors of shader structs match Slang's reflected layout.

use concordia_render::{DrawPush, shader_dir};
use concordia_shaders::{
    CompileRequest, ShaderCompiler, SlangToolchain, SlangcCompiler, Stage, Target, check_layout,
};

#[test]
fn draw_push_matches_forward_slang() {
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    for target in [Target::Msl, Target::SpirV] {
        let module = compiler
            .compile(&CompileRequest {
                path: shader_dir().join("forward.slang"),
                entry: "vs".into(),
                stage: Stage::Vertex,
                target,
                defines: Default::default(),
                include_dirs: vec![concordia_shaders::builtin_dir()],
            })
            .unwrap();
        check_layout::<DrawPush>(&module.reflection.push.expect("push constants")).unwrap();
    }
    assert!(DrawPush::default().bytes().len() <= concordia_rhi::MAX_PUSH_CONSTANTS);
}
