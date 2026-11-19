//! CPU reference tonemappers; `shaders/tonemap.slang` implements the same curves and a test
//! keeps them equal.
//!
//! References: Khronos PBR Neutral Tone Mapper specification; AgX after Troy Sobotka's
//! description, as the minimal analytic fit (inset/outset matrices, log2 range
//! [-12.47393, 4.026069], 6th-order contrast polynomial) described by Benjamin Wrensch (2023)
//! and used by Godot 4.4 (MIT); ACES fitted curve by Krzysztof Narkowicz (2015).

use crate::Tonemap;

/// AgX inset matrix (rows).
const AGX_INSET: [[f32; 3]; 3] = [
    [0.842_479_06, 0.078_433_6, 0.079_223_745],
    [0.042_328_242, 0.878_468_6, 0.079_166_13],
    [0.042_375_655, 0.078_433_6, 0.879_143],
];
/// AgX outset matrix (rows; the inverse transform back to display primaries).
const AGX_OUTSET: [[f32; 3]; 3] = [
    [1.196_879, -0.098_020_88, -0.099_029_74],
    [-0.052_896_85, 1.151_903_1, -0.098_961_18],
    [-0.052_971_635, -0.098_043_45, 1.151_073_7],
];
const AGX_MIN_EV: f32 = -12.473_93;
const AGX_MAX_EV: f32 = 4.026_069;

/// Maps a linear HDR color to linear display values in [0, 1] (before sRGB encoding).
pub fn apply(tonemap: Tonemap, color: [f32; 3]) -> [f32; 3] {
    match tonemap {
        Tonemap::AgX => agx(color),
        Tonemap::KhronosPbrNeutral => pbr_neutral(color),
        Tonemap::AcesFitted => color.map(|x| aces(x.max(0.0))),
        Tonemap::Linear => color.map(|x| x.clamp(0.0, 1.0)),
    }
}

/// The sRGB transfer function (linear → encoded), as the GPU applies when writing sRGB targets.
pub fn srgb_encode(linear: f32) -> f32 {
    let x = linear.clamp(0.0, 1.0);
    if x <= 0.003_130_8 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

fn mul(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    m.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

fn agx_contrast(x: f32) -> f32 {
    let (x2, x4) = (x * x, x * x * x * x);
    15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x
        - 0.002_32
}

fn agx(color: [f32; 3]) -> [f32; 3] {
    let encoded = mul(&AGX_INSET, color).map(|v| {
        let ev = v.max(1e-10).log2().clamp(AGX_MIN_EV, AGX_MAX_EV);
        agx_contrast((ev - AGX_MIN_EV) / (AGX_MAX_EV - AGX_MIN_EV))
    });
    mul(&AGX_OUTSET, encoded).map(|v| v.max(0.0).powf(2.2).min(1.0))
}

fn pbr_neutral(mut color: [f32; 3]) -> [f32; 3] {
    const START: f32 = 0.8 - 0.04;
    const DESATURATION: f32 = 0.15;
    let x = color[0].min(color[1]).min(color[2]);
    let offset = if x < 0.08 { x - 6.25 * x * x } else { 0.04 };
    color = color.map(|c| c - offset);
    let peak = color[0].max(color[1]).max(color[2]);
    if peak < START {
        return color.map(|c| c.clamp(0.0, 1.0));
    }
    let d = 1.0 - START;
    let new_peak = 1.0 - d * d / (peak + d - START);
    let g = 1.0 - 1.0 / (DESATURATION * (peak - new_peak) + 1.0);
    color.map(|c| (c * new_peak / peak * (1.0 - g) + new_peak * g).clamp(0.0, 1.0))
}

fn aces(x: f32) -> f32 {
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}
