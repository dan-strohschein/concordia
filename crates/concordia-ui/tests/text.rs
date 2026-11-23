//! Text shaping, wrapping, alignment, runs and overflow, against metrics read from the fonts' own
//! tables (see `common::metrics`).
//!
//! Noto Sans Mono: its Latin glyphs all advance as `M` does (read through `cmap` and `hmtx`), so
//! at size s one glyph is u = advance(M) × s / unitsPerEm wide and a string of n glyphs is n·u (no
//! kerning in a monospaced font). `advanceWidthMax` is no guide: some symbols are wider. A line is (ascender − descender + lineGap) × s / unitsPerEm tall, and with
//! line height 1 the first baseline sits at ascender × s / unitsPerEm.

mod common;

use common::{assert_close, font_bytes, metrics};
use concordia_ui::*;

const SIZE: f32 = 20.0;

fn mono_style() -> ResolvedStyle {
    ResolvedStyle {
        font: MONO.parse().unwrap(),
        font_size: SIZE,
        ..ResolvedStyle::default()
    }
}

fn u() -> f32 {
    metrics(&font_bytes("NotoSansMono-Regular.ttf")).mono_advance(SIZE)
}

fn shape_mono(text: &UiText, width: Option<f32>, height: Option<f32>) -> TextBlock {
    let mut fonts = UiFonts::new();
    shape(&mut fonts, &mut NoFonts, text, &mono_style(), width, height)
}

#[test]
fn text_metrics_come_from_the_font_tables() {
    let m = metrics(&font_bytes("NotoSansMono-Regular.ttf"));
    let block = shape_mono(&UiText::plain("MMMM"), None, None);
    assert_eq!(block.lines.len(), 1);
    let line = &block.lines[0];
    assert_close(line.width, 4.0 * m.mono_advance(SIZE));
    assert_close(line.height, m.line_height(SIZE));
    assert_close(line.baseline, m.ascent(SIZE));
    assert_close(block.width, 4.0 * m.mono_advance(SIZE));
    assert_close(block.height, m.line_height(SIZE));
    assert_eq!(block.glyphs.len(), 4);
    for (i, g) in block.glyphs.iter().enumerate() {
        assert_close(g.x, i as f32 * m.mono_advance(SIZE));
        assert_close(g.y, m.ascent(SIZE));
        assert_eq!(g.size, SIZE);
    }
}

/// "aaa bbb ccc" in 7.5u wraps after "bbb": "aaa bbb " (its trailing space hangs, so the line is
/// 7u wide) and "ccc" (3u). Centered in 7.5u: x = 0.25u and 2.25u. End: 0.5u and 4.5u. The
/// second line's top is one line height down.
#[test]
fn text_wraps_at_word_boundaries_and_aligns() {
    let u = u();
    let m = metrics(&font_bytes("NotoSansMono-Regular.ttf"));
    for (align, x0, x1) in [
        (TextAlign::Start, 0.0, 0.0),
        (TextAlign::Center, 0.25, 2.25),
        (TextAlign::End, 0.5, 4.5),
    ] {
        let text = UiText {
            align,
            ..UiText::plain("aaa bbb ccc")
        };
        let block = shape_mono(&text, Some(7.5 * u), None);
        let lines: Vec<&str> = block.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(lines, ["aaa bbb ", "ccc"], "{align:?}");
        assert_close(block.lines[0].width, 7.0 * u);
        assert_close(block.lines[1].width, 3.0 * u);
        assert_close(block.lines[0].x, x0 * u);
        assert_close(block.lines[1].x, x1 * u);
        assert_close(block.lines[1].top, m.line_height(SIZE));
        assert_close(block.height, 2.0 * m.line_height(SIZE));
    }
    // No wrapping: one 11u line.
    let none = UiText {
        wrap: Wrap::None,
        ..UiText::plain("aaa bbb ccc")
    };
    let block = shape_mono(&none, Some(7.5 * u), None);
    assert_eq!(block.lines.len(), 1);
    assert_close(block.lines[0].width, 11.0 * u);
}

/// Two runs: "ab" in the mono family at 20 px, then "cd" in Noto Sans, bold, 30 px, red. The
/// glyphs carry their run's index, size and font; the colors are the runs'.
#[test]
fn styled_runs_change_font_and_color_per_run() {
    let red = [1.0, 0.0, 0.0, 1.0];
    let text = UiText {
        runs: vec![
            TextRun {
                text: "ab".into(),
                ..TextRun::default()
            },
            TextRun {
                text: "cd".into(),
                font: Some(SANS.parse().unwrap()),
                size: Some(30.0),
                weight: Some(700.0),
                color: Some(red),
                ..TextRun::default()
            },
        ],
        ..UiText::default()
    };
    let block = shape_mono(&text, None, None);
    let runs: Vec<u32> = block.glyphs.iter().map(|g| g.run).collect();
    assert_eq!(runs, [0, 0, 1, 1]);
    let sizes: Vec<f32> = block.glyphs.iter().map(|g| g.size).collect();
    assert_eq!(sizes, [20.0, 20.0, 30.0, 30.0]);
    assert_eq!(block.glyphs[0].font, block.glyphs[1].font);
    assert_ne!(block.glyphs[1].font, block.glyphs[2].font);
    assert_eq!(block.colors, vec![[1.0; 4], red]);
    // The mono glyphs advance u each.
    assert_close(block.glyphs[1].x, u());
}

/// "aaaa bbbb cccc" in 9.5u, one line. Clip keeps "aaaa bbbb ". Ellipsis needs prefix + "…" to
/// fit: "aaaa bbbb…" is 10u, too wide; "aaaa bbb…" is 9u.
#[test]
fn text_overflow_ellipsis_truncates_last_line() {
    let u = u();
    let base = UiText {
        max_lines: Some(1),
        ..UiText::plain("aaaa bbbb cccc")
    };
    let clip = UiText {
        overflow: Overflow::Clip,
        ..base.clone()
    };
    let block = shape_mono(&clip, Some(9.5 * u), None);
    assert_eq!(block.lines.len(), 1);
    assert_eq!(block.lines[0].text, "aaaa bbbb ");
    let ellipsis = UiText {
        overflow: Overflow::Ellipsis,
        ..base
    };
    let block = shape_mono(&ellipsis, Some(9.5 * u), None);
    assert_eq!(block.lines.len(), 1);
    assert_eq!(block.lines[0].text, "aaaa bbb…");
    assert_close(block.lines[0].width, 9.0 * u);
    // A height limit works like max_lines: 1.5 lines tall keeps one line.
    let m = metrics(&font_bytes("NotoSansMono-Regular.ttf"));
    let tall = UiText {
        overflow: Overflow::Ellipsis,
        ..UiText::plain("aaaa bbbb cccc")
    };
    let block = shape_mono(&tall, Some(9.5 * u), Some(1.5 * m.line_height(SIZE)));
    assert_eq!(block.lines.len(), 1);
    assert_eq!(block.lines[0].text, "aaaa bbb…");
}

/// Line height 2 doubles each line: two lines are 4 font line heights tall.
#[test]
fn line_height_multiplies_the_font_line() {
    let u = u();
    let m = metrics(&font_bytes("NotoSansMono-Regular.ttf"));
    let text = UiText {
        line_height: 2.0,
        ..UiText::plain("aaa bbb ccc")
    };
    let block = shape_mono(&text, Some(7.5 * u), None);
    assert_eq!(block.lines.len(), 2);
    assert_close(block.height, 4.0 * m.line_height(SIZE));
}

#[test]
fn builtin_fonts_resolve_by_name_and_imports_name_their_family() {
    let mut fonts = UiFonts::new();
    assert_eq!(
        fonts
            .family(&SANS.parse().unwrap(), &mut NoFonts)
            .as_deref(),
        Some("Noto Sans")
    );
    assert_eq!(
        fonts
            .family(&MONO.parse().unwrap(), &mut NoFonts)
            .as_deref(),
        Some("Noto Sans Mono")
    );
    assert_eq!(
        fonts.family(&"builtin:font/nope".parse().unwrap(), &mut NoFonts),
        None
    );
    let font = read_font(&font_bytes("NotoSansMono-Regular.ttf")).unwrap();
    assert_eq!(font.family, "Noto Sans Mono");
    assert!(read_font(b"not a font").is_err());
}

#[test]
fn font_importer_imports_ttf_into_the_ddc() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("fonts")).unwrap();
    std::fs::write(
        dir.path().join("fonts/mono.ttf"),
        font_bytes("NotoSansMono-Regular.ttf"),
    )
    .unwrap();
    let ids = Box::new(concordia_core::id::DeterministicIdGen::new(7));
    let mut db = concordia_assets::AssetDb::new(dir.path(), ids).unwrap();
    db.register(std::sync::Arc::new(FontImporter));
    let id = db
        .import(std::path::Path::new("fonts/mono.ttf"))
        .unwrap()
        .id;
    let reference: concordia_assets::AssetRef = format!("asset:{id}").parse().unwrap();
    let font: Font = db
        .load(&reference, concordia_assets::AssetKind::Font)
        .unwrap();
    assert_eq!(font.family, "Noto Sans Mono");
    assert_eq!(font.bytes, font_bytes("NotoSansMono-Regular.ttf"));
    std::fs::write(dir.path().join("fonts/bad.ttf"), b"nope").unwrap();
    assert!(db.import(std::path::Path::new("fonts/bad.ttf")).is_err());
}
