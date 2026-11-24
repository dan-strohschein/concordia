//! Textures decode to linear RGB the same way for every consumer (the renderer's sky, the path
//! tracer's), M4 4.1b2.

use concordia_assets::{TextureData, TextureFormat};

fn texture(format: TextureFormat, bytes: Vec<u8>) -> TextureData {
    TextureData {
        width: 2,
        height: 1,
        format,
        mips: vec![bytes],
    }
}

#[test]
fn every_color_format_decodes_to_linear_rgb() {
    let half = |v: f32| half::f16::from_f32(v).to_le_bytes();
    let hdr: Vec<u8> = [[2.5f32, 0.5, 0.0, 1.0], [0.0, 100.0, 0.25, 1.0]]
        .iter()
        .flat_map(|p| p.iter().flat_map(|c| half(*c)))
        .collect();
    let got = texture(TextureFormat::Rgba16Float, hdr)
        .linear_rgb()
        .unwrap();
    assert_eq!(got, vec![[2.5, 0.5, 0.0], [0.0, 100.0, 0.25]]);

    let linear = texture(
        TextureFormat::Rgba8Linear,
        vec![255, 0, 51, 255, 0, 255, 0, 0],
    );
    assert_eq!(
        linear.linear_rgb().unwrap(),
        vec![[1.0, 0.0, 0.2], [0.0, 1.0, 0.0]]
    );

    let srgb = texture(
        TextureFormat::Rgba8Srgb,
        vec![255, 188, 0, 255, 10, 0, 0, 255],
    );
    let got = srgb.linear_rgb().unwrap();
    assert!(
        (got[0][1] - 0.502886).abs() < 1e-4,
        "sRGB 188 is about 0.5 linear: {got:?}"
    );
    assert!(
        (got[1][0] - 10.0 / 255.0 / 12.92).abs() < 1e-6,
        "the linear toe"
    );

    assert!(
        texture(TextureFormat::R8, vec![1, 2]).linear_rgb().is_err(),
        "one channel isn't color"
    );
    assert!(
        texture(TextureFormat::Rgba8Linear, vec![1, 2, 3])
            .linear_rgb()
            .is_err(),
        "short data"
    );
}

#[test]
fn every_format_decodes_to_linear_rgba_as_the_gpu_samples_it() {
    let srgb = texture(
        TextureFormat::Rgba8Srgb,
        vec![255, 0, 188, 51, 0, 0, 0, 255],
    );
    let got = srgb.linear_rgba().unwrap();
    assert!(
        (got[0][2] - 0.5029).abs() < 1e-3,
        "color decoded: {:?}",
        got[0]
    );
    assert_eq!(got[0][3], 0.2, "alpha stays linear");
    let linear = texture(
        TextureFormat::Rgba8Linear,
        vec![255, 0, 51, 102, 0, 0, 0, 0],
    );
    assert_eq!(linear.linear_rgba().unwrap()[0], [1.0, 0.0, 0.2, 0.4]);
    let mask = TextureData {
        width: 2,
        height: 1,
        format: TextureFormat::R8,
        mips: vec![vec![51, 255]],
    };
    assert_eq!(
        mask.linear_rgba().unwrap(),
        vec![[0.2, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0]],
        "R8 reads as (r, 0, 0, 1)"
    );
    assert_eq!(
        srgb.linear_rgb().unwrap()[0],
        [got[0][0], got[0][1], got[0][2]],
        "linear_rgb is its rgb"
    );
}
