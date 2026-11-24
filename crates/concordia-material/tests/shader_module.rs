//! The material Slang module (`shaders/material.slang`) is the GPU mirror of this crate's BSDF,
//! shared by the renderer and the GPU path tracer (M4 4.1b1). It must compile with only its own
//! folder and the shader builtins: no renderer code.

use concordia_shaders::{
    CompileRequest, ShaderCompiler, SlangToolchain, SlangcCompiler, Stage, Target,
};

#[test]
fn material_module_compiles_without_the_renderer() {
    let toolchain = SlangToolchain::locate().expect("run `just slang-fetch`");
    let compiler = SlangcCompiler::new(toolchain);
    let tests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/shaders");
    for target in [Target::Msl, Target::SpirV] {
        let request = CompileRequest {
            path: tests.join("uses_material.slang"),
            entry: "main".into(),
            stage: Stage::Compute,
            target,
            defines: Default::default(),
            include_dirs: vec![concordia_material::shader_dir()],
        };
        compiler
            .compile(&request)
            .unwrap_or_else(|e| panic!("{target:?}: {e}"));
    }
}

#[test]
fn material_module_names_no_renderer_types() {
    let text = std::fs::read_to_string(concordia_material::shader_dir().join("material.slang"))
        .expect("the module exists");
    for renderer_only in [
        "Frame",
        "g_textures",
        "VsOut",
        "import bindless",
        "import skin",
    ] {
        assert!(
            !text.contains(renderer_only),
            "material.slang mentions {renderer_only}"
        );
    }
}
