//! Godot's `Control` anchor rules (docs: "Size and anchors", `Control.anchor_*`, `offset_*`,
//! `grow_*`, layout presets), computed by hand.
//!
//! Each edge: `left = parent.x + anchor_left × parent.w + offset_left`, `right = parent.x +
//! anchor_right × parent.w + offset_right`, and the same for top/bottom. A size below the minimum
//! (or negative) becomes the minimum, moving the side its grow direction names.
//!
//! Presets (size w × h, margin m), per axis:
//! - Begin: anchors 0, 0; offsets m, m + w.     Grow End.
//! - End: anchors 1, 1; offsets −m − w, −m.      Grow Begin.
//! - Middle: anchors ½, ½; offsets −w/2, w/2.    Grow Both.
//! - Wide: anchors 0, 1; offsets m, −m.          Grow End.
//!
//! Parent (10, 20, 400 × 300), size 100 × 50, margin 8:
//! - x: Begin 10 + 8 = 18; End 10 + 400 − 108 = 302; Middle 10 + 200 − 50 = 160;
//!   Wide 18 .. 10 + 400 − 8 = 402, so w = 384.
//! - y: Begin 20 + 8 = 28; End 20 + 300 − 58 = 262; Middle 20 + 150 − 25 = 145;
//!   Wide 28 .. 312, so h = 284.

use concordia_ui::*;

const PARENT: Rect = Rect::new(10.0, 20.0, 400.0, 300.0);

#[test]
fn anchor_presets_and_offsets_match_godot_rules() {
    let want: [(Preset, Rect, Grow, Grow); 16] = [
        (
            Preset::TopLeft,
            Rect::new(18.0, 28.0, 100.0, 50.0),
            Grow::End,
            Grow::End,
        ),
        (
            Preset::TopRight,
            Rect::new(302.0, 28.0, 100.0, 50.0),
            Grow::Begin,
            Grow::End,
        ),
        (
            Preset::BottomLeft,
            Rect::new(18.0, 262.0, 100.0, 50.0),
            Grow::End,
            Grow::Begin,
        ),
        (
            Preset::BottomRight,
            Rect::new(302.0, 262.0, 100.0, 50.0),
            Grow::Begin,
            Grow::Begin,
        ),
        (
            Preset::CenterLeft,
            Rect::new(18.0, 145.0, 100.0, 50.0),
            Grow::End,
            Grow::Both,
        ),
        (
            Preset::CenterTop,
            Rect::new(160.0, 28.0, 100.0, 50.0),
            Grow::Both,
            Grow::End,
        ),
        (
            Preset::CenterRight,
            Rect::new(302.0, 145.0, 100.0, 50.0),
            Grow::Begin,
            Grow::Both,
        ),
        (
            Preset::CenterBottom,
            Rect::new(160.0, 262.0, 100.0, 50.0),
            Grow::Both,
            Grow::Begin,
        ),
        (
            Preset::Center,
            Rect::new(160.0, 145.0, 100.0, 50.0),
            Grow::Both,
            Grow::Both,
        ),
        (
            Preset::LeftWide,
            Rect::new(18.0, 28.0, 100.0, 284.0),
            Grow::End,
            Grow::End,
        ),
        (
            Preset::TopWide,
            Rect::new(18.0, 28.0, 384.0, 50.0),
            Grow::End,
            Grow::End,
        ),
        (
            Preset::RightWide,
            Rect::new(302.0, 28.0, 100.0, 284.0),
            Grow::Begin,
            Grow::End,
        ),
        (
            Preset::BottomWide,
            Rect::new(18.0, 262.0, 384.0, 50.0),
            Grow::End,
            Grow::Begin,
        ),
        (
            Preset::VCenterWide,
            Rect::new(160.0, 28.0, 100.0, 284.0),
            Grow::Both,
            Grow::End,
        ),
        (
            Preset::HCenterWide,
            Rect::new(18.0, 145.0, 384.0, 50.0),
            Grow::End,
            Grow::Both,
        ),
        (
            Preset::FullRect,
            Rect::new(18.0, 28.0, 384.0, 284.0),
            Grow::End,
            Grow::End,
        ),
    ];
    assert_eq!(want.map(|w| w.0), PRESETS);
    for (preset, rect, grow_h, grow_v) in want {
        let a = Anchored::preset(preset, [100.0, 50.0], 8.0);
        assert_eq!(anchored_rect(PARENT, &a, [0.0, 0.0]), rect, "{preset:?}");
        assert_eq!((a.grow_h, a.grow_v), (grow_h, grow_v), "{preset:?}");
    }
}

/// Full anchors, offsets (0, 0, −390, −290) in (0, 0, 400 × 300): a 10 × 10 rect at the origin.
/// With a 30 × 30 minimum: End keeps the left/top (x = 0), Begin keeps the right/bottom
/// (x = 10 − 30 = −20), Both keeps the center (5 − 15 = −10).
#[test]
fn anchored_child_grows_by_grow_direction() {
    let parent = Rect::new(0.0, 0.0, 400.0, 300.0);
    let base = Anchored {
        anchors: Edges::new(0.0, 0.0, 1.0, 1.0),
        offsets: Edges::new(0.0, 0.0, -390.0, -290.0),
        ..Anchored::default()
    };
    assert_eq!(
        anchored_rect(parent, &base, [0.0, 0.0]),
        Rect::new(0.0, 0.0, 10.0, 10.0)
    );
    for (grow, at) in [(Grow::End, 0.0), (Grow::Begin, -20.0), (Grow::Both, -10.0)] {
        let a = Anchored {
            grow_h: grow,
            grow_v: grow,
            ..base
        };
        assert_eq!(
            anchored_rect(parent, &a, [30.0, 30.0]),
            Rect::new(at, at, 30.0, 30.0),
            "{grow:?}"
        );
    }
}

/// Offsets that cross (left 50, right 20) give −30, which becomes the zero minimum; growing End
/// keeps x = 50.
#[test]
fn crossed_offsets_collapse_to_the_minimum() {
    let a = Anchored {
        offsets: Edges::new(50.0, 0.0, 20.0, 10.0),
        ..Anchored::default()
    };
    assert_eq!(
        anchored_rect(Rect::new(0.0, 0.0, 400.0, 300.0), &a, [0.0, 0.0]),
        Rect::new(50.0, 0.0, 0.0, 10.0)
    );
}
