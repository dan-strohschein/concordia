//! The UI pass (ADR-0060): draw-list encoding (no GPU), the push constants' layout against
//! `ui.slang`, and goldens per feature (macOS GPU; skipped elsewhere unless required).
//!
//! Each quad is 40 floats: rect 0–3, clip 4–7, radii 8–11, border color 12–15, color/tint
//! 16–19, uv 20–23, gradient 24–27 (from, to | center, radius), mask 28–31 (center, start, cw),
//! 32 ccw, 33 border width, 34 opacity, 35 paint kind (0 none, 1 solid, 2 linear, 3 radial,
//! 4 texture, 5 glyph), 36 bindless texture, 37 first stop, 38 stop count, 39 mask kind (0 none,
//! 1 angular). Each stop is 8 floats: offset, three unused, then linear RGBA.

mod common;

use concordia_api::{UiDrawList, UiMask, UiPaint, UiQuad, UiStop, UiTexture, UiTextureFormat};
use concordia_render::{UI_QUAD_FLOATS, UI_STOP_FLOATS, UiPush, encode_ui, shader_dir};
use concordia_shaders::{
    CompileRequest, ShaderCompiler, SlangToolchain, SlangcCompiler, Stage, Target, check_layout,
};

fn texture(format: UiTextureFormat) -> UiTexture {
    UiTexture {
        key: 1,
        generation: 0,
        width: 2,
        height: 2,
        format,
        pixels: vec![255; 16].into(),
    }
}

#[test]
fn ui_quads_encode_to_forty_floats_and_stops_to_eight() {
    let mut solid = UiQuad::new([1.0, 2.0, 3.0, 4.0], UiPaint::Solid([0.1, 0.2, 0.3, 0.4]));
    solid.clip = [5.0, 6.0, 7.0, 8.0];
    solid.radius = [1.0, 2.0, 3.0, 4.0];
    solid.border = 2.0;
    solid.border_color = [0.5, 0.6, 0.7, 0.8];
    solid.opacity = 0.75;
    let stops = vec![
        UiStop {
            offset: 0.0,
            color: [1.0, 0.0, 0.0, 1.0],
        },
        UiStop {
            offset: 1.0,
            color: [0.0, 0.0, 1.0, 1.0],
        },
    ];
    let linear = UiQuad::new(
        [0.0; 4],
        UiPaint::Linear {
            from: [1.0, 2.0],
            to: [3.0, 4.0],
            stops: stops.clone(),
        },
    );
    let radial = UiQuad::new(
        [0.0; 4],
        UiPaint::Radial {
            center: [5.0, 6.0],
            radius: [7.0, 8.0],
            stops,
        },
    );
    let mut tex = UiQuad::new(
        [0.0; 4],
        UiPaint::Texture {
            texture: 1,
            uv: [0.1, 0.2, 0.3, 0.4],
            tint: [1.0; 4],
        },
    );
    tex.mask = UiMask::Angular {
        center: [9.0, 10.0],
        start: 0.5,
        cw: 1.0,
        ccw: 2.0,
    };
    let glyph = UiQuad::new(
        [0.0; 4],
        UiPaint::Glyph {
            texture: 0,
            uv: [0.0, 0.0, 0.5, 0.5],
            color: [0.9; 4],
        },
    );
    let list = UiDrawList {
        quads: vec![solid, linear, radial, tex, glyph],
        textures: vec![
            texture(UiTextureFormat::R8),
            texture(UiTextureFormat::Rgba8Srgb),
        ],
    };
    let (quads, stops) = encode_ui(&list, &[17, 42]);
    assert_eq!(quads.len(), 5 * UI_QUAD_FLOATS);
    assert_eq!(stops.len(), 4 * UI_STOP_FLOATS);
    let q = |i: usize| &quads[i * UI_QUAD_FLOATS..(i + 1) * UI_QUAD_FLOATS];
    assert_eq!(
        &q(0)[0..20],
        &[
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 1.0, 2.0, 3.0, 4.0, 0.5, 0.6, 0.7, 0.8, 0.1,
            0.2, 0.3, 0.4
        ]
    );
    assert_eq!(&q(0)[33..40], &[2.0, 0.75, 1.0, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(&q(1)[24..28], &[1.0, 2.0, 3.0, 4.0]);
    assert_eq!(&q(1)[35..39], &[2.0, 0.0, 0.0, 2.0]);
    assert_eq!(&q(2)[24..28], &[5.0, 6.0, 7.0, 8.0]);
    assert_eq!(&q(2)[35..39], &[3.0, 0.0, 2.0, 2.0]);
    assert_eq!(&q(3)[16..24], &[1.0, 1.0, 1.0, 1.0, 0.1, 0.2, 0.3, 0.4]);
    assert_eq!(&q(3)[28..33], &[9.0, 10.0, 0.5, 1.0, 2.0]);
    assert_eq!((q(3)[35], q(3)[36], q(3)[39]), (4.0, 42.0, 1.0));
    assert_eq!(&q(4)[16..20], &[0.9; 4]);
    assert_eq!((q(4)[35], q(4)[36]), (5.0, 17.0));
    assert_eq!(&stops[0..8], &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]);
    assert_eq!(&stops[8..16], &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn ui_push_matches_ui_slang() {
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    for target in [Target::Msl, Target::SpirV] {
        let module = compiler
            .compile(&CompileRequest {
                path: shader_dir().join("ui.slang"),
                entry: "ui_vs".into(),
                stage: Stage::Vertex,
                target,
                defines: Default::default(),
                include_dirs: vec![concordia_shaders::builtin_dir()],
            })
            .unwrap();
        check_layout::<UiPush>(&module.reflection.push.expect("push constants")).unwrap();
    }
}
