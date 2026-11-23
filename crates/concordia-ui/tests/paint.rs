//! Paint geometry, derived by hand: image stretch modes (Godot `TextureRect`), nine-patches
//! (Godot `NinePatchRect`, CSS `border-image` for shrinking and `round`), progress fills (Godot
//! `TextureProgressBar`), separators and gradients (CSS `linear-gradient` / `radial-gradient`).

mod common;

use common::close;
use concordia_api::{UiMask, UiPaint, UiStop};
use concordia_ui::*;

const FULL: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn same(a: &[(Rect, [f32; 4])], b: &[(Rect, [f32; 4])]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|((ra, ua), (rb, ub))| {
            [ra.x, ra.y, ra.w, ra.h]
                .iter()
                .zip([rb.x, rb.y, rb.w, rb.h])
                .all(|(p, q)| close(*p, q))
                && ua.iter().zip(ub).all(|(p, q)| close(*p, *q))
        })
}

/// Rect (10, 20, 200 × 100), image 50 × 40:
/// - Scale fills the rect. Keep: (10, 20, 50 × 40). KeepCentered: (10 + 75, 20 + 30) = (85, 50).
/// - KeepAspect: s = min(4, 2.5) = 2.5, so 125 × 100 at the corner; centered: x = 10 + 37.5.
/// - KeepAspectCovered: s = max(4, 2.5) = 4, drawn 200 × 160, centered, so 30 px are cut top and
///   bottom: v from 30/160 = 0.1875 to 130/160 = 0.8125.
#[test]
fn image_modes_produce_expected_quads() {
    let rect = Rect::new(10.0, 20.0, 200.0, 100.0);
    let q = |mode| image_quads(mode, rect, [50.0, 40.0], FULL, [false, false]);
    let one = |r: Rect, uv| vec![(r, uv)];
    assert!(same(&q(ImageMode::Scale), &one(rect, FULL)));
    assert!(same(
        &q(ImageMode::Keep),
        &one(Rect::new(10.0, 20.0, 50.0, 40.0), FULL)
    ));
    assert!(same(
        &q(ImageMode::KeepCentered),
        &one(Rect::new(85.0, 50.0, 50.0, 40.0), FULL)
    ));
    assert!(same(
        &q(ImageMode::KeepAspect),
        &one(Rect::new(10.0, 20.0, 125.0, 100.0), FULL)
    ));
    assert!(same(
        &q(ImageMode::KeepAspectCentered),
        &one(Rect::new(47.5, 20.0, 125.0, 100.0), FULL)
    ));
    assert!(same(
        &q(ImageMode::KeepAspectCovered),
        &one(rect, [0.0, 0.1875, 1.0, 0.8125])
    ));
    // Mirrored left to right: u runs 1 → 0.
    let flipped = image_quads(ImageMode::Scale, rect, [50.0, 40.0], FULL, [true, false]);
    assert!(same(&flipped, &one(rect, [1.0, 0.0, 0.0, 1.0])));
}

/// Tiling 50 × 40 over 200 × 100: 4 whole columns; rows of 40, 40 and a cut 20 (v to 0.5), so 12
/// tiles. With the region v 0..0.5, the cut row samples v 0..0.25.
#[test]
fn image_tile_cuts_last_row_and_column() {
    let rect = Rect::new(10.0, 20.0, 200.0, 100.0);
    let tiles = image_quads(ImageMode::Tile, rect, [50.0, 40.0], FULL, [false, false]);
    assert_eq!(tiles.len(), 12);
    assert!(same(
        &tiles[..1],
        &[(Rect::new(10.0, 20.0, 50.0, 40.0), FULL)]
    ));
    assert!(same(
        &tiles[11..],
        &[(Rect::new(160.0, 100.0, 50.0, 20.0), [0.0, 0.0, 1.0, 0.5])]
    ));
    let half = image_quads(
        ImageMode::Tile,
        rect,
        [50.0, 40.0],
        [0.5, 0.0, 1.0, 0.5],
        [false, false],
    );
    assert!(same(
        &half[11..],
        &[(Rect::new(160.0, 100.0, 50.0, 20.0), [0.5, 0.0, 1.0, 0.25])]
    ));
    // A huge tiling is capped.
    let many = image_quads(
        ImageMode::Tile,
        Rect::new(0.0, 0.0, 10_000.0, 10_000.0),
        [1.0, 1.0],
        FULL,
        [false, false],
    );
    assert_eq!(many.len(), MAX_TILES);
}

/// 30 × 30 image, 10 px margins, into (0, 0, 100 × 60): columns at 0, 10, 90, 100; rows at 0, 10,
/// 50, 60; u and v at 0, ⅓, ⅔, 1.
#[test]
fn nine_patch_slices_into_nine_quads_with_expected_uvs() {
    let t = 1.0 / 3.0;
    let s = 2.0 / 3.0;
    let stretch = [AxisStretch::Stretch; 2];
    let pieces = nine_patch_quads(
        Rect::new(0.0, 0.0, 100.0, 60.0),
        [0.0, 0.0, 30.0, 30.0],
        [30.0, 30.0],
        [10.0; 4],
        stretch,
        true,
    );
    let want = vec![
        (Rect::new(0.0, 0.0, 10.0, 10.0), [0.0, 0.0, t, t]),
        (Rect::new(10.0, 0.0, 80.0, 10.0), [t, 0.0, s, t]),
        (Rect::new(90.0, 0.0, 10.0, 10.0), [s, 0.0, 1.0, t]),
        (Rect::new(0.0, 10.0, 10.0, 40.0), [0.0, t, t, s]),
        (Rect::new(10.0, 10.0, 80.0, 40.0), [t, t, s, s]),
        (Rect::new(90.0, 10.0, 10.0, 40.0), [s, t, 1.0, s]),
        (Rect::new(0.0, 50.0, 10.0, 10.0), [0.0, s, t, 1.0]),
        (Rect::new(10.0, 50.0, 80.0, 10.0), [t, s, s, 1.0]),
        (Rect::new(90.0, 50.0, 10.0, 10.0), [s, s, 1.0, 1.0]),
    ];
    assert!(same(&pieces, &want), "{pieces:?}");
    let hollow = nine_patch_quads(
        Rect::new(0.0, 0.0, 100.0, 60.0),
        [0.0, 0.0, 30.0, 30.0],
        [30.0, 30.0],
        [10.0; 4],
        stretch,
        false,
    );
    assert_eq!(hollow.len(), 8);
    // A region of a 60 × 60 texture: u and v in sixths.
    let sub = nine_patch_quads(
        Rect::new(0.0, 0.0, 100.0, 60.0),
        [30.0, 30.0, 30.0, 30.0],
        [60.0, 60.0],
        [10.0; 4],
        stretch,
        true,
    );
    assert!(same(
        &sub[..1],
        &[(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            [0.5, 0.5, 4.0 / 6.0, 4.0 / 6.0]
        )]
    ));
}

/// Into (0, 0, 10 × 60) the left and right margins need 20 px: f = min(1, 10/20, 60/20) = 0.5
/// (CSS `border-image`), so columns at 0, 5, 5, 10 and rows at 0, 5, 55, 60; the empty center
/// column drops out, leaving 6 pieces.
#[test]
fn nine_patch_margins_shrink_by_border_image_rule() {
    let pieces = nine_patch_quads(
        Rect::new(0.0, 0.0, 10.0, 60.0),
        [0.0, 0.0, 30.0, 30.0],
        [30.0, 30.0],
        [10.0; 4],
        [AxisStretch::Stretch; 2],
        true,
    );
    assert_eq!(pieces.len(), 6);
    assert!(same(
        &pieces[..1],
        &[(
            Rect::new(0.0, 0.0, 5.0, 5.0),
            [0.0, 0.0, 1.0 / 3.0, 1.0 / 3.0]
        )]
    ));
    assert!(same(
        &pieces[2..3],
        &[(
            Rect::new(0.0, 5.0, 5.0, 50.0),
            [0.0, 1.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0]
        )]
    ));
}

/// Horizontal tiling in (0, 0, 65 × 30): the center span is 45 and the source center 10 wide, so
/// four whole tiles and a cut 5 (u ⅓ to ⅓ + 5/30 = 0.5). TileFit rounds 4.5 to 5 tiles of 9.
#[test]
fn nine_patch_tiles_and_tile_fits_the_center_span() {
    let tile = nine_patch_quads(
        Rect::new(0.0, 0.0, 65.0, 30.0),
        [0.0, 0.0, 30.0, 30.0],
        [30.0, 30.0],
        [10.0; 4],
        [AxisStretch::Tile, AxisStretch::Stretch],
        true,
    );
    let top: Vec<_> = tile
        .iter()
        .filter(|(r, _)| r.y == 0.0 && r.h == 10.0)
        .collect();
    assert_eq!(top.len(), 7, "corner, 5 edge tiles, corner");
    let cut = top[5];
    assert!(same(
        &[*cut],
        &[(
            Rect::new(50.0, 0.0, 5.0, 10.0),
            [1.0 / 3.0, 0.0, 0.5, 1.0 / 3.0]
        )]
    ));
    let fit = nine_patch_quads(
        Rect::new(0.0, 0.0, 65.0, 30.0),
        [0.0, 0.0, 30.0, 30.0],
        [30.0, 30.0],
        [10.0; 4],
        [AxisStretch::TileFit, AxisStretch::Stretch],
        true,
    );
    let top: Vec<_> = fit
        .iter()
        .filter(|(r, _)| r.y == 0.0 && r.h == 10.0)
        .collect();
    assert_eq!(top.len(), 7);
    assert!(close(top[1].0.w, 9.0) && close(top[5].0.x, 46.0));
    assert!(close(top[5].1[2], 2.0 / 3.0));
}

/// Value 25 of 0..100 is f = 0.25. In (0, 0, 200 × 20): LTR (0, 0, 50, 20), u to 0.25; RTL
/// (150, 0, 50, 20), u from 0.75; TTB (0, 0, 200, 5); BTT (0, 15, 200, 5); BilinearH
/// (75, 0, 50, 20), u 0.375..0.625; BilinearV (0, 7.5, 200, 5). Radial modes fill the rect with
/// an angular mask about (100, 10): 360° × 0.25 = π/2 clockwise, counterclockwise, or π/4 each.
#[test]
fn progress_fill_fraction_and_quad_for_each_mode() {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
    let rect = Rect::new(0.0, 0.0, 200.0, 20.0);
    let p = |fill| UiProgress {
        value: 25.0,
        fill,
        ..UiProgress::default()
    };
    assert_eq!(p(FillMode::LeftToRight).fraction(), 0.25);
    let cut = |fill, r: Rect, uv: [f32; 4]| {
        let (got, got_uv, mask) = progress_fill(&p(fill), rect).unwrap();
        assert!(
            same(&[(got, got_uv)], &[(r, uv)]),
            "{fill:?}: {got:?} {got_uv:?}"
        );
        assert_eq!(mask, UiMask::None);
    };
    cut(
        FillMode::LeftToRight,
        Rect::new(0.0, 0.0, 50.0, 20.0),
        [0.0, 0.0, 0.25, 1.0],
    );
    cut(
        FillMode::RightToLeft,
        Rect::new(150.0, 0.0, 50.0, 20.0),
        [0.75, 0.0, 1.0, 1.0],
    );
    cut(
        FillMode::TopToBottom,
        Rect::new(0.0, 0.0, 200.0, 5.0),
        [0.0, 0.0, 1.0, 0.25],
    );
    cut(
        FillMode::BottomToTop,
        Rect::new(0.0, 15.0, 200.0, 5.0),
        [0.0, 0.75, 1.0, 1.0],
    );
    cut(
        FillMode::BilinearH,
        Rect::new(75.0, 0.0, 50.0, 20.0),
        [0.375, 0.0, 0.625, 1.0],
    );
    cut(
        FillMode::BilinearV,
        Rect::new(0.0, 7.5, 200.0, 5.0),
        [0.0, 0.375, 1.0, 0.625],
    );
    let radial = |fill| progress_fill(&p(fill), rect).unwrap();
    let mask = |cw: f32, ccw: f32| UiMask::Angular {
        center: [100.0, 10.0],
        start: 0.0,
        cw,
        ccw,
    };
    for (fill, want) in [
        (FillMode::Clockwise, mask(FRAC_PI_2, 0.0)),
        (FillMode::CounterClockwise, mask(0.0, FRAC_PI_2)),
        (FillMode::ClockwiseAndCounter, mask(FRAC_PI_4, FRAC_PI_4)),
    ] {
        let (r, uv, m) = radial(fill);
        assert_eq!((r, uv), (rect, FULL), "{fill:?}");
        let (
            UiMask::Angular {
                center,
                start,
                cw,
                ccw,
            },
            UiMask::Angular {
                cw: wcw, ccw: wccw, ..
            },
        ) = (m, want)
        else {
            panic!("{fill:?}: {m:?}");
        };
        assert!(close(center[0], 100.0) && close(center[1], 10.0) && close(start, 0.0));
        assert!(close(cw, wcw) && close(ccw, wccw), "{fill:?}: {m:?}");
    }
    // Start 90°, sweep 180°: a quarter fill turns π/4 from π/2.
    let mut half = p(FillMode::Clockwise);
    half.radial = RadialFill {
        start_deg: 90.0,
        sweep_deg: 180.0,
        center: [0.5, 0.5],
    };
    let (_, _, m) = progress_fill(&half, rect).unwrap();
    let UiMask::Angular { start, cw, .. } = m else {
        panic!()
    };
    assert!(close(start, FRAC_PI_2) && close(cw, FRAC_PI_4));
    // Empty bars draw no fill.
    assert!(
        progress_fill(
            &UiProgress {
                value: 0.0,
                ..UiProgress::default()
            },
            rect
        )
        .is_none()
    );
    assert_eq!(
        UiProgress {
            value: 250.0,
            ..UiProgress::default()
        }
        .fraction(),
        1.0
    );
}

/// (0, 0, 100 × 20), thickness 2: horizontal (0, 9, 100, 2); vertical (49, 0, 2, 20).
#[test]
fn separator_draws_centred_line() {
    let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
    let h = UiSeparator {
        thickness: 2.0,
        ..UiSeparator::default()
    };
    assert_eq!(separator_rect(&h, rect), Rect::new(0.0, 9.0, 100.0, 2.0));
    let v = UiSeparator {
        orientation: Orientation::Vertical,
        ..h
    };
    assert_eq!(separator_rect(&v, rect), Rect::new(49.0, 0.0, 2.0, 20.0));
}

/// (10, 20, 200 × 100), center (110, 70). CSS gradient line: direction (sin θ, −cos θ) (y down),
/// length |w sin θ| + |h cos θ|. 90°: (10, 70) → (210, 70). 0°: (110, 120) → (110, 20).
/// 45°: length 300·√½ = 212.13, half 106.07 along (√½, −√½) = ±75 per axis: (35, 145) →
/// (185, −5). Radial (½, ½) radius (½, ¼): center (110, 70), radii (100, 25).
#[test]
fn gradient_stops_and_colors_reach_the_draw_list() {
    let rect = Rect::new(10.0, 20.0, 200.0, 100.0);
    let stops = vec![
        GradientStop {
            offset: 0.0,
            color: [1.0, 0.0, 0.0, 1.0],
        },
        GradientStop {
            offset: 1.0,
            color: [0.0, 0.0, 1.0, 1.0],
        },
    ];
    let want_stops = vec![
        UiStop {
            offset: 0.0,
            color: [1.0, 0.0, 0.0, 1.0],
        },
        UiStop {
            offset: 1.0,
            color: [0.0, 0.0, 1.0, 1.0],
        },
    ];
    assert_eq!(
        background_paint(&Background::Solid([0.5; 4]), rect),
        UiPaint::Solid([0.5; 4])
    );
    for (angle, from, to) in [
        (90.0, [10.0, 70.0], [210.0, 70.0]),
        (0.0, [110.0, 120.0], [110.0, 20.0]),
        (45.0, [35.0, 145.0], [185.0, -5.0]),
    ] {
        let paint = background_paint(
            &Background::Linear {
                angle_deg: angle,
                stops: stops.clone(),
            },
            rect,
        );
        let UiPaint::Linear {
            from: f,
            to: t,
            stops: s,
        } = paint
        else {
            panic!("{paint:?}")
        };
        assert!(
            f.iter()
                .zip(from)
                .chain(t.iter().zip(to))
                .all(|(a, b)| close(*a, b)),
            "{angle}: {f:?} {t:?}"
        );
        assert_eq!(s, want_stops);
    }
    let radial = background_paint(
        &Background::Radial {
            center: [0.5, 0.5],
            radius: [0.5, 0.25],
            stops,
        },
        rect,
    );
    assert_eq!(
        radial,
        UiPaint::Radial {
            center: [110.0, 70.0],
            radius: [100.0, 25.0],
            stops: want_stops
        }
    );
}
