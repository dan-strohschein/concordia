//! Every render entry point compiles to SPIR-V the Vulkan backend accepts (M4 4.1v(b)), checked
//! without a device, so a Mac catches what lavapipe's validation layer would refuse.

use std::collections::BTreeMap;

use concordia_render::{include_dirs, shader_dir};
use concordia_shaders::{
    CompileRequest, ShaderCode, ShaderCompiler, SlangToolchain, SlangcCompiler, Stage, Target,
};

/// `(file, entry, stage)` for every `[shader("…")]` in the render crate's shaders.
fn entry_points() -> Vec<(String, String, Stage)> {
    let mut found = Vec::new();
    for file in std::fs::read_dir(shader_dir()).unwrap() {
        let path = file.unwrap().path();
        if path.extension().is_none_or(|e| e != "slang") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            let Some(stage) = line.trim().strip_prefix("[shader(\"") else {
                continue;
            };
            let stage = match stage.trim_end_matches("\")]") {
                "vertex" => Stage::Vertex,
                "fragment" => Stage::Fragment,
                "compute" => Stage::Compute,
                other => panic!("{}: stage {other}", path.display()),
            };
            let signature = lines
                .find(|l| l.contains('(') && !l.trim_start().starts_with('['))
                .unwrap();
            let entry = signature
                .split('(')
                .next()
                .unwrap()
                .split_whitespace()
                .last()
                .unwrap();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            found.push((name, entry.to_owned(), stage));
        }
    }
    found.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    found
}

#[test]
fn every_render_entry_point_compiles_to_spirv_the_vulkan_backend_accepts() {
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    let entries = entry_points();
    assert!(entries.len() >= 10, "found {entries:?}");
    let mut refused = Vec::new();
    for (file, entry, stage) in &entries {
        let request = CompileRequest {
            path: shader_dir().join(file),
            entry: entry.clone(),
            stage: *stage,
            target: Target::SpirV,
            defines: BTreeMap::new(),
            include_dirs: include_dirs(),
        };
        let module = compiler
            .compile(&request)
            .unwrap_or_else(|e| panic!("{file} {entry}: {e}"));
        let ShaderCode::SpirV(words) = module.code else {
            panic!("{file} {entry}: not SPIR-V")
        };
        if let Err(log) = concordia_rhi_vulkan::check_spirv_module(&words) {
            refused.push(format!("{file} {entry}: {log}"));
        }
    }
    assert!(refused.is_empty(), "{}", refused.join("\n"));
}
