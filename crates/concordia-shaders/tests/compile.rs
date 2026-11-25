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

/// `entry` of `file` compiled to MSL, with its reflected thread group size.
fn msl(file: &str, entry: &str, stage: Stage) -> (String, Option<[u32; 3]>) {
    let module = SlangcCompiler::new(toolchain())
        .compile(&request(file, entry, stage, Target::Msl))
        .unwrap_or_else(|e| panic!("{entry}: {e}"));
    let ShaderCode::Msl(msl) = module.code else {
        panic!("expected MSL")
    };
    (msl, module.reflection.threads_per_group)
}

#[test]
fn object_and_mesh_entries_compile_to_metal_stages() {
    // 4.3b's spike: Metal's object stage takes the bindless table at 0 and push constants at 3,
    // like every other stage; the mesh stage declares its 128/128 output limits.
    let (object, threads) = msl("mesh_pipeline.slang", "object_main", Stage::Amplification);
    assert!(
        object.contains("[[object]]") && object.contains("[[payload]]"),
        "{object}"
    );
    assert!(object.contains("mesh_grid_properties"), "{object}");
    assert!(
        object.contains("[[buffer(0)]]") && object.contains("[[buffer(3)]]"),
        "{object}"
    );
    assert_eq!(threads, Some([32, 1, 1]));
    let (mesh, threads) = msl("mesh_pipeline.slang", "mesh_main", Stage::Mesh);
    assert!(
        mesh.contains("[[mesh]]") && mesh.contains("[[payload]]"),
        "{mesh}"
    );
    assert!(
        mesh.contains("int(128), int(128), metal::topology::triangle"),
        "{mesh}"
    );
    assert_eq!(threads, Some([128, 1, 1]));
    let (fragment, _) = msl("mesh_pipeline.slang", "fragment_main", Stage::Fragment);
    assert!(fragment.contains("[[flat]]"), "{fragment}");
}

#[test]
fn mesh_outputs_written_field_by_field_are_refused_for_metal() {
    // Metal's rule (4.3b): a mesh stage writes each output vertex as a whole struct. `mesh.slang`
    // writes fields, which SPIR-V allows; if Slang lifts the rule this test says so.
    let err = SlangcCompiler::new(toolchain())
        .compile(&request("mesh.slang", "ms_main", Stage::Mesh, Target::Msl))
        .expect_err("field-wise mesh output writes compile for Metal");
    assert!(err.to_string().contains("whole struct"), "{err}");
}

#[test]
fn mesh_pipeline_entries_compile_to_spirv() {
    let compiler = SlangcCompiler::new(toolchain());
    for (entry, stage, model) in [
        ("object_main", Stage::Amplification, 5364),
        ("mesh_main", Stage::Mesh, 5365),
        ("fragment_main", Stage::Fragment, 4),
    ] {
        let mut req = request("mesh_pipeline.slang", entry, stage, Target::SpirV);
        req.defines
            .insert(concordia_shaders::SPIRV_DEFINE.into(), "1".into());
        let module = compiler
            .compile(&req)
            .unwrap_or_else(|e| panic!("{entry}: {e}"));
        let ShaderCode::SpirV(words) = &module.code else {
            panic!("expected SPIR-V")
        };
        assert_eq!(execution_models(words), vec![model], "{entry}");
    }
}

/// SPIR-V's `OpDecorate <id> PerPrimitiveEXT` (decoration 5271).
fn has_per_primitive_decoration(words: &[u32]) -> bool {
    let mut at = 5;
    while at < words.len() {
        let (count, op) = ((words[at] >> 16) as usize, words[at] & 0xffff);
        if op == 71 && words.get(at + 2) == Some(&5271) {
            return true;
        }
        at += count.max(1);
    }
    false
}

#[test]
fn per_primitive_fragment_inputs_lack_their_spirv_decoration() {
    // Why 4.3b's mesh path has no per-primitive outputs: Vulkan needs the fragment input
    // decorated like the mesh output, and Slang 2026.22 has no way to say so. If Slang adds it,
    // this test fails and the triangle view can return to the mesh path.
    let decorated = |entry: &str, stage| {
        let mut req = request("per_primitive.slang", entry, stage, Target::SpirV);
        req.defines
            .insert(concordia_shaders::SPIRV_DEFINE.into(), "1".into());
        let module = SlangcCompiler::new(toolchain()).compile(&req).unwrap();
        let ShaderCode::SpirV(words) = &module.code else {
            panic!("expected SPIR-V")
        };
        has_per_primitive_decoration(words)
    };
    assert!(
        decorated("mesh_main", Stage::Mesh),
        "the mesh output is decorated"
    );
    assert!(!decorated("fragment_main", Stage::Fragment));
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
