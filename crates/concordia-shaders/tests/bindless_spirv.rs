//! The bindless ABI on SPIR-V (M4 4.1v(a), ADR-0088): sets 0, 1 and 2, with the buffer table a
//! storage buffer of addresses (a std140 uniform block would be 256 KiB).

use std::collections::BTreeMap;

use concordia_rhi::conformance::shader_dir;
use concordia_shaders::{
    CompileRequest, ShaderCode, ShaderCompiler, SlangToolchain, SlangcCompiler, Stage, Target,
};

const UNIFORM_CONSTANT: u32 = 0;
const UNIFORM: u32 = 2;
const STORAGE_BUFFER: u32 = 12;

fn compile(name: &str, entry: &str, stage: Stage, target: Target) -> ShaderCode {
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    let request = CompileRequest {
        path: shader_dir().join(format!("{name}.slang")),
        entry: entry.into(),
        stage,
        target,
        defines: BTreeMap::new(),
        include_dirs: vec![concordia_shaders::builtin_dir()],
    };
    compiler.compile(&request).expect("compiles").code
}

/// (set, binding) → storage class of each decorated global variable.
fn descriptors(words: &[u32]) -> BTreeMap<(u32, u32), u32> {
    let (mut sets, mut bindings, mut classes) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let mut i = 5;
    while i < words.len() {
        let (count, op) = ((words[i] >> 16) as usize, words[i] & 0xffff);
        let args = &words[i + 1..i + count.max(1)];
        match op {
            71 if args[1] == 34 => _ = sets.insert(args[0], args[2]),
            71 if args[1] == 33 => _ = bindings.insert(args[0], args[2]),
            59 => _ = classes.insert(args[1], args[2]),
            _ => {}
        }
        i += count.max(1);
    }
    sets.iter()
        .map(|(id, set)| ((*set, bindings[id]), classes[id]))
        .collect()
}

fn spirv(name: &str, entry: &str, stage: Stage) -> BTreeMap<(u32, u32), u32> {
    match compile(name, entry, stage, Target::SpirV) {
        ShaderCode::SpirV(words) => descriptors(&words),
        ShaderCode::Msl(_) => panic!("expected SPIR-V"),
    }
}

#[test]
fn spirv_bindless_tables_use_sets_0_1_2() {
    let buffers = spirv("sum", "sum", Stage::Compute);
    assert_eq!(buffers.get(&(0, 0)), Some(&STORAGE_BUFFER), "{buffers:?}");
    assert!(!buffers.values().any(|c| *c == UNIFORM), "{buffers:?}");
    let textures = spirv("sample_texture", "sample_with_sampler", Stage::Fragment);
    assert_eq!(
        textures.get(&(1, 0)),
        Some(&UNIFORM_CONSTANT),
        "{textures:?}"
    );
    assert_eq!(
        textures.get(&(1, 1)),
        Some(&UNIFORM_CONSTANT),
        "{textures:?}"
    );
    let storage = spirv("write_texture", "write_texture", Stage::Compute);
    assert_eq!(storage.get(&(2, 0)), Some(&UNIFORM_CONSTANT), "{storage:?}");
}

#[test]
fn msl_bindless_layout_is_unchanged() {
    let ShaderCode::Msl(msl) = compile("sum", "sum", Stage::Compute, Target::Msl) else {
        panic!("expected MSL")
    };
    assert!(
        msl.contains("array<uint device*, int(16384)> ptr_0;"),
        "{msl}"
    );
}

/// The `OpEntryPoint` names in a module.
fn entry_names(words: &[u32]) -> Vec<String> {
    let mut names = Vec::new();
    let mut i = 5;
    while i < words.len() {
        let count = (words[i] >> 16) as usize;
        if words[i] & 0xffff == 15 {
            let bytes: Vec<u8> = words[i + 3..i + count]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect();
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            names.push(String::from_utf8_lossy(&bytes[..end]).into_owned());
        }
        i += count.max(1);
    }
    names
}

#[test]
fn spirv_entry_points_keep_their_source_names() {
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    let request = CompileRequest {
        path: shader_dir().join("sum.slang"),
        entry: "sum".into(),
        stage: Stage::Compute,
        target: Target::SpirV,
        defines: BTreeMap::new(),
        include_dirs: vec![concordia_shaders::builtin_dir()],
    };
    let module = compiler.compile(&request).expect("compiles");
    let ShaderCode::SpirV(words) = &module.code else {
        panic!("expected SPIR-V")
    };
    assert_eq!(
        entry_names(words),
        std::slice::from_ref(&module.entry),
        "the module names its entry as the code does"
    );
}
