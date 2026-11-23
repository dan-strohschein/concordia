//! `ui_components_validate_and_reject_bad_input`: one row per rule, each naming its field.
//!
//! Rules (ADR-0060, the 2.7b refinement): finite values; `Px` ≥ 0; `Percent` in 0..100;
//! min ≤ max; padding ≥ 0; anchors in 0..1 with left ≤ right and top ≤ bottom; anchored nodes
//! leave width/height `Auto`; at most 64 grid tracks; 2–8 ascending stops in 0..1; opacity in
//! 0..1; border width ≥ 0; radii ≥ 0; font size in 1..1024; weight in 100..900; text ≤ 64 KiB;
//! line height in 0.5..4; `max_lines` ≥ 1; progress min < max and sweep in (0, 360]; a
//! non-empty theme class; a positive, at most 16384 canvas; a positive world pixel size.

use concordia_ui::*;

fn field<T>(r: Result<T, UiError>) -> String {
    match r {
        Ok(_) => "ok".into(),
        Err(e) => e.field_name().unwrap_or("?").to_owned(),
    }
}

fn anchored(anchors: Edges) -> UiNode {
    UiNode {
        position: Position::Anchored(Anchored {
            anchors,
            ..Anchored::default()
        }),
        ..UiNode::default()
    }
}

fn stops(offsets: &[f32]) -> Vec<GradientStop> {
    offsets
        .iter()
        .map(|&offset| GradientStop {
            offset,
            color: [1.0; 4],
        })
        .collect()
}

fn style_bg(background: Background) -> UiStyle {
    UiStyle {
        background: Some(background),
        ..UiStyle::default()
    }
}

#[test]
fn defaults_are_valid() {
    assert_eq!(field(UiNode::default().check()), "ok");
    assert_eq!(field(UiRoot::default().check()), "ok");
    assert_eq!(field(UiStyle::default().check()), "ok");
    assert_eq!(field(UiText::plain("hi").check()), "ok");
    assert_eq!(field(UiProgress::default().check()), "ok");
    assert_eq!(field(UiSeparator::default().check()), "ok");
}

#[test]
fn ui_node_rules_name_their_field() {
    let rows: Vec<(UiNode, &str)> = vec![
        (
            UiNode {
                width: Length::Px(-1.0),
                ..UiNode::default()
            },
            "width",
        ),
        (
            UiNode {
                height: Length::Percent(101.0),
                ..UiNode::default()
            },
            "height",
        ),
        (
            UiNode {
                min_width: Length::Px(f32::NAN),
                ..UiNode::default()
            },
            "min_width",
        ),
        (
            UiNode {
                min_width: Length::Px(50.0),
                max_width: Length::Px(40.0),
                ..UiNode::default()
            },
            "min_width",
        ),
        (
            UiNode {
                min_height: Length::Px(9.0),
                max_height: Length::Px(8.0),
                ..UiNode::default()
            },
            "min_height",
        ),
        (
            UiNode {
                padding: Edges::new(0.0, -1.0, 0.0, 0.0),
                ..UiNode::default()
            },
            "padding/top",
        ),
        (
            UiNode {
                margin: Edges::new(f32::INFINITY, 0.0, 0.0, 0.0),
                ..UiNode::default()
            },
            "margin",
        ),
        (
            UiNode {
                gap: [-2.0, 0.0],
                ..UiNode::default()
            },
            "gap/0",
        ),
        (
            UiNode {
                grow: -1.0,
                ..UiNode::default()
            },
            "grow",
        ),
        (
            anchored(Edges::new(0.0, 0.0, 1.5, 1.0)),
            "position/anchors/right",
        ),
        (anchored(Edges::new(0.8, 0.0, 0.2, 1.0)), "position/anchors"),
        (anchored(Edges::new(0.0, 0.9, 1.0, 0.1)), "position/anchors"),
        (
            UiNode {
                width: Length::Px(10.0),
                ..anchored(Edges::new(0.0, 0.0, 1.0, 1.0))
            },
            "width",
        ),
        (
            UiNode {
                layout: Layout::Grid(Grid {
                    columns: vec![Track::Fr(1.0); MAX_TRACKS + 1],
                    ..Grid::default()
                }),
                ..UiNode::default()
            },
            "layout/columns",
        ),
        (
            UiNode {
                layout: Layout::Grid(Grid {
                    rows: vec![Track::Px(10.0), Track::Percent(120.0)],
                    ..Grid::default()
                }),
                ..UiNode::default()
            },
            "layout/rows/1",
        ),
    ];
    for (node, want) in rows {
        assert_eq!(field(node.check()), want, "{node:?}");
    }
}

#[test]
fn ui_style_rules_name_their_field() {
    let rows: Vec<(UiStyle, &str)> = vec![
        (
            style_bg(Background::Linear {
                angle_deg: 0.0,
                stops: stops(&[0.0]),
            }),
            "background/stops",
        ),
        (
            style_bg(Background::Linear {
                angle_deg: 0.0,
                stops: stops(&[0.0; 9]),
            }),
            "background/stops",
        ),
        (
            style_bg(Background::Linear {
                angle_deg: 0.0,
                stops: stops(&[0.0, 0.6, 0.4]),
            }),
            "background/stops/2",
        ),
        (
            style_bg(Background::Radial {
                center: [0.5, 0.5],
                radius: [0.5, 0.5],
                stops: stops(&[0.0, 1.2]),
            }),
            "background/stops/1",
        ),
        (
            style_bg(Background::Radial {
                center: [0.5, 0.5],
                radius: [-0.1, 0.5],
                stops: stops(&[0.0, 1.0]),
            }),
            "background/radius",
        ),
        (
            style_bg(Background::Linear {
                angle_deg: f32::NAN,
                stops: stops(&[0.0, 1.0]),
            }),
            "background/angle_deg",
        ),
        (
            style_bg(Background::Solid([f32::NAN, 0.0, 0.0, 1.0])),
            "background",
        ),
        (
            UiStyle {
                opacity: Some(1.5),
                ..UiStyle::default()
            },
            "opacity",
        ),
        (
            UiStyle {
                border: Some(Border {
                    width: -1.0,
                    color: [1.0; 4],
                }),
                ..UiStyle::default()
            },
            "border/width",
        ),
        (
            UiStyle {
                radius: Some([0.0, -3.0, 0.0, 0.0]),
                ..UiStyle::default()
            },
            "radius",
        ),
        (
            UiStyle {
                font_size: Some(0.5),
                ..UiStyle::default()
            },
            "font_size",
        ),
        (
            UiStyle {
                weight: Some(950.0),
                ..UiStyle::default()
            },
            "weight",
        ),
        (
            UiStyle {
                color: Some([0.0, f32::INFINITY, 0.0, 1.0]),
                ..UiStyle::default()
            },
            "color",
        ),
    ];
    for (style, want) in rows {
        assert_eq!(field(style.check()), want, "{style:?}");
    }
}

#[test]
fn text_progress_root_and_theme_rules_name_their_field() {
    let big = UiText::plain("x".repeat(MAX_TEXT_BYTES + 1));
    assert_eq!(field(big.check()), "runs");
    let mut t = UiText::plain("a");
    t.line_height = 0.25;
    assert_eq!(field(t.check()), "line_height");
    let mut t = UiText::plain("a");
    t.max_lines = Some(0);
    assert_eq!(field(t.check()), "max_lines");
    let mut t = UiText::plain("a");
    t.runs[0].size = Some(2000.0);
    assert_eq!(field(t.check()), "runs/0/size");
    let mut t = UiText::plain("a");
    t.runs.push(TextRun {
        weight: Some(50.0),
        ..TextRun::default()
    });
    assert_eq!(field(t.check()), "runs/1/weight");

    let p = UiProgress {
        min: 5.0,
        max: 5.0,
        ..UiProgress::default()
    };
    assert_eq!(field(p.check()), "max");
    let p = UiProgress {
        value: f32::NAN,
        ..UiProgress::default()
    };
    assert_eq!(field(p.check()), "value");
    let mut p = UiProgress::default();
    p.radial.sweep_deg = 0.0;
    assert_eq!(field(p.check()), "radial/sweep_deg");

    let r = UiRoot {
        canvas: [0.0, 720.0],
        ..UiRoot::default()
    };
    assert_eq!(field(r.check()), "canvas");
    let r = UiRoot {
        canvas: [20_000.0, 720.0],
        ..UiRoot::default()
    };
    assert_eq!(field(r.check()), "canvas");
    let r = UiRoot {
        space: RootSpace::World {
            offset: [0.0; 3],
            pixel_size: 0.0,
        },
        ..UiRoot::default()
    };
    assert_eq!(field(r.check()), "space/pixel_size");

    let s = UiSeparator {
        thickness: -1.0,
        ..UiSeparator::default()
    };
    assert_eq!(field(s.check()), "thickness");
    let themed = UiThemed {
        theme: "builtin:theme/none".parse().unwrap(),
        class: " ".into(),
    };
    assert_eq!(field(themed.check()), "class");

    let pic = UiImage {
        image: "builtin:texture/x".parse().unwrap(),
        region: Some([0, 0, 0, 4]),
        mode: ImageMode::Scale,
        tint: [1.0; 4],
        flip_h: false,
        flip_v: false,
    };
    assert_eq!(field(pic.check()), "region");
    let pic = UiImage {
        region: None,
        tint: [f32::NAN, 1.0, 1.0, 1.0],
        ..pic
    };
    assert_eq!(field(pic.check()), "tint");
    let nine = UiNinePatch {
        image: "builtin:texture/x".parse().unwrap(),
        region: Some([0, 0, 8, 0]),
        margins: [2; 4],
        axis_h: AxisStretch::Stretch,
        axis_v: AxisStretch::Stretch,
        draw_center: true,
        tint: [1.0; 4],
    };
    assert_eq!(field(nine.check()), "region");
    let nine = UiNinePatch {
        region: None,
        tint: [1.0, 1.0, f32::INFINITY, 1.0],
        ..nine
    };
    assert_eq!(field(nine.check()), "tint");
}
