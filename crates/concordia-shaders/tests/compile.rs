//! Compiling Slang with the pinned toolchain (M1 phase 1.3).

mod common;

use common::{request, toolchain};
use concordia_shaders::{ShaderCode, ShaderCompiler, ShaderError, SlangcCompiler, Stage, Target};

#[test]
fn slang_toolchain_is_pinned_version() {
    let pin = include_str!("../../../tools/slang/slang.toml");
    let tc = toolchain();
    assert!(
        pin.contains(&format!("version = \"{}\"", tc.version())),
        "{}",
        tc.version()
    );
}

#[test]
fn compiles_compute_entry_to_msl() {
    let compiler = SlangcCompiler::new(toolchain());
    let module = compiler
        .compile(&request("fill.slang", "fill", Stage::Compute, Target::Msl))
        .unwrap();
    let ShaderCode::Msl(msl) = &module.code else {
        panic!("expected MSL")
    };
    assert!(
        msl.contains("[[kernel]]") && msl.contains(" fill("),
        "{msl}"
    );
    assert_eq!(module.reflection.threads_per_group, Some([64, 1, 1]));
    assert_eq!(module.entry, "fill");
}

#[test]
fn compiles_vertex_fragment_pair_to_msl() {
    let compiler = SlangcCompiler::new(toolchain());
    for (entry, stage, marker) in [
        ("vs", Stage::Vertex, "[[vertex]]"),
        ("fs", Stage::Fragment, "[[fragment]]"),
    ] {
        let module = compiler
            .compile(&request("triangle.slang", entry, stage, Target::Msl))
            .unwrap();
        let ShaderCode::Msl(msl) = &module.code else {
            panic!("expected MSL")
        };
        assert!(msl.contains(marker), "{entry}: {msl}");
    }
}

#[test]
fn compiles_to_spirv() {
    let compiler = SlangcCompiler::new(toolchain());
    let module = compiler
        .compile(&request(
            "fill.slang",
            "fill",
            Stage::Compute,
            Target::SpirV,
        ))
        .unwrap();
    let ShaderCode::SpirV(words) = &module.code else {
        panic!("expected SPIR-V")
    };
    assert_eq!(words.first(), Some(&0x0723_0203), "SPIR-V magic number");
}

/// The execution models of a SPIR-V module's entry points.
fn execution_models(words: &[u32]) -> Vec<u32> {
    let (mut models, mut at) = (Vec::new(), 5);
    while at < words.len() {
        let (count, op) = ((words[at] >> 16) as usize, words[at] & 0xffff);
        if op == 15 {
            models.push(words[at + 1]);
        }
        at += count.max(1);
    }
    models
}

#[test]
fn compiles_mesh_and_amplification_entries_to_spirv() {
    let compiler = SlangcCompiler::new(toolchain());
    // SPIR-V's MeshEXT and TaskEXT execution models (VK_EXT_mesh_shader).
    for (entry, stage, model) in [
        ("ms_main", Stage::Mesh, 5365),
        ("as_main", Stage::Amplification, 5364),
    ] {
        let module = compiler
            .compile(&request("mesh.slang", entry, stage, Target::SpirV))
            .unwrap();
        let ShaderCode::SpirV(words) = &module.code else {
            panic!("expected SPIR-V")
        };
        assert_eq!(execution_models(words), vec![model], "{entry}");
    }
}

#[test]
fn bindless_abi_lands_on_the_metal_slots() {
    let compiler = SlangcCompiler::new(toolchain());
    let module = compiler
        .compile(&request("fill.slang", "fill", Stage::Compute, Target::Msl))
        .unwrap();
    let slots: Vec<(&str, u32)> = module
        .reflection
        .bindings
        .iter()
        .map(|b| (b.name.as_str(), b.index))
        .collect();
    assert_eq!(
        slots,
        [("g_buffers", 0), ("g_textures", 1), ("g_storage", 2)]
    );
    let ShaderCode::Msl(msl) = &module.code else {
        panic!("expected MSL")
    };
    assert!(
        msl.contains("entryPointParams_1 [[buffer(3)]]"),
        "push constants at slot 3: {msl}"
    );
}

#[test]
fn reflection_lists_push_constant_size_and_fields() {
    let compiler = SlangcCompiler::new(toolchain());
    let module = compiler
        .compile(&request("fill.slang", "fill", Stage::Compute, Target::Msl))
        .unwrap();
    insta::assert_debug_snapshot!(module.reflection.push);
}

#[test]
fn compile_error_reports_file_line_and_message() {
    let compiler = SlangcCompiler::new(toolchain());
    let err = compiler
        .compile(&request(
            "broken.slang",
            "broken",
            Stage::Compute,
            Target::Msl,
        ))
        .unwrap_err();
    let ShaderError::Compile { diagnostics, .. } = err else {
        panic!("expected Compile, got {err}")
    };
    let first = diagnostics.first().expect("a diagnostic");
    assert!(first.file.ends_with("broken.slang"), "{first:?}");
    assert_eq!(first.line, 4, "{first:?}");
    assert!(first.col > 0 && !first.message.is_empty(), "{first:?}");
}

#[test]
fn missing_entry_point_and_file_are_errors() {
    let compiler = SlangcCompiler::new(toolchain());
    assert!(
        compiler
            .compile(&request("fill.slang", "nope", Stage::Compute, Target::Msl))
            .is_err()
    );
    assert!(matches!(
        compiler.compile(&request("absent.slang", "x", Stage::Compute, Target::Msl)),
        Err(ShaderError::Io(_))
    ));
}
