//! D4's spike (M4 4.1v(c)): does Slang's SPIR-V for a mesh shader (`VK_EXT_mesh_shader`) and an
//! inline ray query (`VK_KHR_ray_query`) compile, pass the backend's checks and run on this
//! machine's Vulkan under validation? Test-only: production mesh pipelines are 4.3b and
//! acceleration structures 4.8. Device cases skip without Vulkan unless
//! `CONCORDIA_REQUIRE_VULKAN=1`.

mod gpu;

use std::collections::BTreeMap;
use std::path::PathBuf;

use concordia_shaders::{
    CompileRequest, ShaderCode, ShaderCompiler, SlangToolchain, SlangcCompiler, Stage, Target,
};

use crate::spirv::{self, Model};

/// A spike shader file next to this module.
fn source(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/spike")
        .join(file)
}

/// `entry` of `file` as SPIR-V words, checked as pipeline creation checks it.
fn spirv(file: &str, entry: &str, stage: Stage, model: Model) -> Vec<u32> {
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    let request = CompileRequest {
        path: source(file),
        entry: entry.into(),
        stage,
        target: Target::SpirV,
        defines: BTreeMap::from([(concordia_shaders::SPIRV_DEFINE.into(), "1".into())]),
        include_dirs: vec![concordia_shaders::builtin_dir()],
    };
    let module = compiler
        .compile(&request)
        .unwrap_or_else(|e| panic!("{file}:{entry} does not compile: {e}"));
    let ShaderCode::SpirV(words) = module.code else {
        panic!("{file}:{entry}: expected SPIR-V");
    };
    if let Err(log) = spirv::check(&words, entry, model) {
        panic!("{file}:{entry} fails the backend's SPIR-V checks: {log}");
    }
    words
}

/// The mesh spike's stages: (amplification, mesh, fragment) entries with their models.
const MESH_ENTRIES: [(&str, Stage, Model); 4] = [
    ("ms_main", Stage::Mesh, Model::Mesh),
    ("as_main", Stage::Amplification, Model::Task),
    ("ms_tasked", Stage::Mesh, Model::Mesh),
    ("fs_main", Stage::Fragment, Model::Fragment),
];

#[test]
fn spike_shaders_compile_to_spirv_that_passes_the_backends_checks() {
    for (entry, stage, model) in MESH_ENTRIES {
        spirv("mesh.slang", entry, stage, model);
    }
    spirv("ray_query.slang", "cs_main", Stage::Compute, Model::Compute);
}

#[test]
fn a_mesh_shader_draws_a_triangle() {
    let Some(gpu) = gpu::Gpu::open(gpu::Want::Mesh) else {
        return;
    };
    let ms = spirv("mesh.slang", "ms_main", Stage::Mesh, Model::Mesh);
    let fs = spirv("mesh.slang", "fs_main", Stage::Fragment, Model::Fragment);
    let pixels = gpu.draw_mesh(None, &ms, "ms_main", &fs, "fs_main");
    assert_triangle(&pixels, [255, 128, 64, 255]);
    gpu.assert_clean();
}

#[test]
fn an_amplification_shader_launches_the_mesh_shader() {
    let Some(gpu) = gpu::Gpu::open(gpu::Want::Mesh) else {
        return;
    };
    if !gpu.task_shaders() {
        assert!(
            !concordia_testkit::vulkan_required(),
            "the device has VK_EXT_mesh_shader without taskShader"
        );
        return;
    }
    let task = spirv("mesh.slang", "as_main", Stage::Amplification, Model::Task);
    let ms = spirv("mesh.slang", "ms_tasked", Stage::Mesh, Model::Mesh);
    let fs = spirv("mesh.slang", "fs_main", Stage::Fragment, Model::Fragment);
    let pixels = gpu.draw_mesh(Some((&task, "as_main")), &ms, "ms_tasked", &fs, "fs_main");
    assert_triangle(&pixels, [64, 255, 128, 255]);
    gpu.assert_clean();
}

#[test]
fn an_inline_ray_query_hits_and_misses_a_triangle() {
    let Some(gpu) = gpu::Gpu::open(gpu::Want::RayQuery) else {
        return;
    };
    let cs = spirv("ray_query.slang", "cs_main", Stage::Compute, Model::Compute);
    let hits = gpu.trace(&cs, "cs_main");
    let t = |i: usize| f32::from_bits(hits[i * 2 + 1]);
    assert_eq!(
        [hits[0], hits[2], hits[4], hits[6]],
        [1, 1, 0, 0],
        "hits {hits:?}"
    );
    assert!(
        (t(0) - 1.0).abs() < 1e-4 && (t(1) - 1.0).abs() < 1e-4,
        "t {} {}",
        t(0),
        t(1)
    );
    gpu.assert_clean();
}

/// The triangle covers the centre in `color` (±1 per channel) and leaves the corner clear.
fn assert_triangle(pixels: &[u8], color: [u8; 4]) {
    let at = |x: usize, y: usize| {
        let i = (y * gpu::SIZE as usize + x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };
    let centre = at(gpu::SIZE as usize / 2, gpu::SIZE as usize / 2);
    assert!(
        centre.iter().zip(color).all(|(a, b)| a.abs_diff(b) <= 1),
        "centre {centre:?}, expected {color:?}"
    );
    assert_eq!(at(0, 0), [0, 0, 0, 0], "corner");
}
