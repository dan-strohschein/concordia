//! Rust and Slang struct layouts agree (M1 phase 1.3).

mod common;

use common::{request, toolchain};
use concordia_shaders::{
    ShaderCompiler, ShaderStruct, SlangcCompiler, Stage, Target, check_layout, shader_struct,
};

/// The fill fixture's push constants. Metal aligns `float3` to 16 bytes and makes it 16 bytes long.
#[repr(C)]
struct FillPush {
    out: u32,
    count: u32,
    scale: f32,
    _pad: u32,
    tint: [f32; 4],
}
shader_struct!(FillPush {
    out,
    count,
    scale,
    tint
});

/// The naive translation: `[f32; 3]` at offset 12 disagrees with Slang's layout.
#[repr(C)]
struct Naive {
    out: u32,
    count: u32,
    scale: f32,
    tint: [f32; 3],
}
shader_struct!(Naive {
    out,
    count,
    scale,
    tint
});

/// A field missing on the Rust side.
#[repr(C)]
struct Drifted {
    out: u32,
    count: u32,
    scale: f32,
}
shader_struct!(Drifted { out, count, scale });

fn fill_push() -> concordia_shaders::StructLayout {
    let module = SlangcCompiler::new(toolchain())
        .compile(&request("fill.slang", "fill", Stage::Compute, Target::Msl))
        .unwrap();
    module.reflection.push.expect("push constants")
}

#[test]
fn rust_and_slang_struct_layouts_agree() {
    let push = fill_push();
    check_layout::<FillPush>(&push).unwrap();
    assert_eq!(FillPush::layout().size, push.size);
}

#[test]
fn layout_mismatches_name_the_field() {
    let push = fill_push();
    let naive = check_layout::<Naive>(&push).unwrap_err().to_string();
    assert!(
        naive.contains("tint") && naive.contains("offset"),
        "{naive}"
    );
    let drifted = check_layout::<Drifted>(&push).unwrap_err().to_string();
    assert!(
        drifted.contains("tint") && drifted.contains("missing"),
        "{drifted}"
    );
}
