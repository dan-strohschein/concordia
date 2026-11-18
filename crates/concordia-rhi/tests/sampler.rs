//! Static samplers: the ABI's fixed sampler set (ADR-0040).

use concordia_rhi::StaticSampler;

#[test]
fn static_sampler_indices_are_dense_and_stable() {
    let indices: Vec<u32> = StaticSampler::ALL.iter().map(|s| s.index()).collect();
    assert_eq!(
        indices,
        (0..StaticSampler::ALL.len() as u32).collect::<Vec<_>>()
    );
    assert_eq!(StaticSampler::LinearRepeat.index(), 0);
}

#[test]
fn bindless_slang_declares_matching_sampler_constants() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../concordia-shaders/shaders/bindless.slang"
    ))
    .unwrap();
    assert!(
        source.contains(&format!(
            "SamplerState samplers[{}]",
            StaticSampler::ALL.len()
        )),
        "sampler array size"
    );
    for sampler in StaticSampler::ALL {
        let line = format!("{} = {};", sampler.slang_name(), sampler.index());
        assert!(source.contains(&line), "bindless.slang is missing `{line}`");
    }
}
