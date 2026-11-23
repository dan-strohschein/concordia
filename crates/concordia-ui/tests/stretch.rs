//! `canvas_stretch_for_each_aspect_mode`: Godot's `canvas_items` stretch with each aspect
//! (docs: "Multiple resolutions"), computed by hand.
//!
//! Base B = 800 × 400 (2:1); s = min(Vw/Bw, Vh/Bh).
//! - 1600 × 900 (16:9): Vw/Bw = 2, Vh/Bh = 2.25, s = 2. Taller than 2:1.
//!   Ignore (2, 2.25), size B. Keep: size B, offset (0, (900 − 800)/2 = 50). KeepWidth: taller, so
//!   size (800, 900/2 = 450), offset 0. KeepHeight: not wider, so like Keep. Expand (800, 450).
//! - 1200 × 900 (4:3): 1.5 and 2.25, s = 1.5. Keep offset (0, (900 − 600)/2 = 150). KeepWidth
//!   (800, 600). KeepHeight like Keep. Expand (800, 600).
//! - 2000 × 800 (5:2): 2.5 and 2, s = 2. Wider. Keep offset ((2000 − 1600)/2 = 200, 0).
//!   KeepWidth: not taller, so like Keep. KeepHeight (2000/2 = 1000, 400). Expand (1000, 400).
//! - Disabled ignores the aspect: scale 1, size = the viewport.

use concordia_ui::*;

fn map(stretch: Stretch, aspect: Aspect, viewport: [f32; 2]) -> CanvasMap {
    let root = UiRoot {
        canvas: [800.0, 400.0],
        stretch,
        aspect,
        ..UiRoot::default()
    };
    canvas_map(&root, viewport)
}

fn m(scale: [f32; 2], offset: [f32; 2], size: [f32; 2]) -> CanvasMap {
    CanvasMap {
        scale,
        offset,
        size,
    }
}

#[test]
fn canvas_stretch_for_each_aspect_mode() {
    use Aspect::*;
    let rows = [
        (
            [1600.0, 900.0],
            Ignore,
            m([2.0, 2.25], [0.0, 0.0], [800.0, 400.0]),
        ),
        (
            [1600.0, 900.0],
            Keep,
            m([2.0, 2.0], [0.0, 50.0], [800.0, 400.0]),
        ),
        (
            [1600.0, 900.0],
            KeepWidth,
            m([2.0, 2.0], [0.0, 0.0], [800.0, 450.0]),
        ),
        (
            [1600.0, 900.0],
            KeepHeight,
            m([2.0, 2.0], [0.0, 50.0], [800.0, 400.0]),
        ),
        (
            [1600.0, 900.0],
            Expand,
            m([2.0, 2.0], [0.0, 0.0], [800.0, 450.0]),
        ),
        (
            [1200.0, 900.0],
            Ignore,
            m([1.5, 2.25], [0.0, 0.0], [800.0, 400.0]),
        ),
        (
            [1200.0, 900.0],
            Keep,
            m([1.5, 1.5], [0.0, 150.0], [800.0, 400.0]),
        ),
        (
            [1200.0, 900.0],
            KeepWidth,
            m([1.5, 1.5], [0.0, 0.0], [800.0, 600.0]),
        ),
        (
            [1200.0, 900.0],
            KeepHeight,
            m([1.5, 1.5], [0.0, 150.0], [800.0, 400.0]),
        ),
        (
            [1200.0, 900.0],
            Expand,
            m([1.5, 1.5], [0.0, 0.0], [800.0, 600.0]),
        ),
        (
            [2000.0, 800.0],
            Keep,
            m([2.0, 2.0], [200.0, 0.0], [800.0, 400.0]),
        ),
        (
            [2000.0, 800.0],
            KeepWidth,
            m([2.0, 2.0], [200.0, 0.0], [800.0, 400.0]),
        ),
        (
            [2000.0, 800.0],
            KeepHeight,
            m([2.0, 2.0], [0.0, 0.0], [1000.0, 400.0]),
        ),
        (
            [2000.0, 800.0],
            Expand,
            m([2.0, 2.0], [0.0, 0.0], [1000.0, 400.0]),
        ),
    ];
    for (viewport, aspect, want) in rows {
        assert_eq!(
            map(Stretch::Scale, aspect, viewport),
            want,
            "{viewport:?} {aspect:?}"
        );
    }
    assert_eq!(
        map(Stretch::Disabled, Keep, [1600.0, 900.0]),
        m([1.0, 1.0], [0.0, 0.0], [1600.0, 900.0])
    );
}

/// `apply`: UI (100, 10) under Keep at 1600 × 900 lands at (100·2 + 0, 10·2 + 50) = (200, 70).
#[test]
fn canvas_map_applies_scale_then_offset() {
    let keep = map(Stretch::Scale, Aspect::Keep, [1600.0, 900.0]);
    assert_eq!(keep.apply([100.0, 10.0]), [200.0, 70.0]);
}
