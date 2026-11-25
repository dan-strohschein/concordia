//! The RHI conformance suite's Slang provider, shared by every backend (M4 4.1v(a)).

use concordia_rhi::ShaderSource;
use concordia_rhi::conformance::{ShaderProvider, ShaderStage};
use concordia_shaders::Target;
use concordia_shaders::conformance::SlangShaders;

#[test]
fn compiles_conformance_kernels_for_metal_with_their_group_size() {
    let shaders = SlangShaders::new(Target::Msl);
    let (source, threads) = shaders.shader("sum", "sum", ShaderStage::Compute);
    assert!(matches!(source, ShaderSource::Msl { ref entry, .. } if !entry.is_empty()));
    assert_eq!(threads, Some([1, 1, 1]));
}

#[test]
fn compiles_conformance_shaders_to_spirv() {
    let shaders = SlangShaders::new(Target::SpirV);
    let (source, _) = shaders.shader("triangle", "vs", ShaderStage::Vertex);
    let ShaderSource::SpirV { words, .. } = source else {
        panic!("expected SPIR-V")
    };
    assert_eq!(words.first(), Some(&0x0723_0203), "SPIR-V magic");
}

#[test]
fn repeated_requests_reuse_the_compiled_module() {
    let shaders = SlangShaders::new(Target::SpirV);
    let first = shaders.shader("fill", "fill", ShaderStage::Compute);
    let second = shaders.shader("fill", "fill", ShaderStage::Compute);
    assert_eq!(first, second);
    assert_eq!(shaders.compiled_count(), 1);
}
