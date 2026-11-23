//! `glyph_atlas_packs_and_reuses_glyphs`: swash coverage masks packed into shelves.
//!
//! Each glyph rasterizes once per (font, glyph, size in quarter pixels, quarter-pixel x offset);
//! asking again returns the same slot without touching the page (its generation stays). Slots on
//! a page never overlap and hold coverage. A glyph with no outline (a space) has no slot; one
//! bigger than a page has none either.

use concordia_ui::*;

fn glyphs(text: &str, size: f32) -> TextBlock {
    let style = ResolvedStyle {
        font_size: size,
        ..ResolvedStyle::default()
    };
    shape(
        &mut UiFonts::new(),
        &mut NoFonts,
        &UiText::plain(text),
        &style,
        None,
        None,
    )
}

#[test]
fn glyph_atlas_packs_and_reuses_glyphs() {
    let block = glyphs("AB g", 32.0);
    let mut atlas = GlyphAtlas::new(256);
    let font = &block.fonts[0];
    let slots: Vec<Option<GlyphSlot>> = block
        .glyphs
        .iter()
        .map(|g| atlas.glyph(font, g.id, 32.0, 0.0))
        .collect();
    assert!(slots[0].is_some() && slots[1].is_some() && slots[3].is_some());
    assert!(slots[2].is_none(), "the space has no outline");
    assert_eq!(atlas.len(), 4, "the space is cached as empty too");
    let generation = atlas.pages()[0].generation;
    // Asking again is a hit: same slot, page untouched.
    assert_eq!(atlas.glyph(font, block.glyphs[0].id, 32.0, 0.0), slots[0]);
    assert_eq!(atlas.pages()[0].generation, generation);
    // A quarter-pixel offset is a different raster.
    assert_ne!(atlas.glyph(font, block.glyphs[0].id, 32.0, 0.25), slots[0]);
    assert_eq!(atlas.len(), 5);
    let page = &atlas.pages()[0];
    let placed: Vec<GlyphSlot> = slots.iter().flatten().copied().collect();
    for (i, a) in placed.iter().enumerate() {
        assert!(a.w > 0 && a.h > 0 && a.x + a.w <= page.width && a.y + a.h <= page.height);
        let ink: u32 = (a.y..a.y + a.h)
            .flat_map(|y| (a.x..a.x + a.w).map(move |x| (x, y)))
            .map(|(x, y)| u32::from(page.pixels[(y * page.width + x) as usize]))
            .sum();
        assert!(ink > 0, "slot {i} holds coverage");
        for b in &placed[i + 1..] {
            let apart =
                a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
            assert!(apart, "{a:?} overlaps {b:?}");
        }
    }
    // 'A' sits on the baseline: its top is above it and its left near the pen.
    let a = slots[0].unwrap();
    assert!(a.top > 0 && a.left.abs() < 4);
}

#[test]
fn glyphs_larger_than_a_page_are_skipped_and_full_pages_add_pages() {
    let big = glyphs("W", 600.0);
    let mut atlas = GlyphAtlas::new(128);
    assert_eq!(
        atlas.glyph(&big.fonts[0], big.glyphs[0].id, 600.0, 0.0),
        None
    );
    let many = glyphs("ABCDEFGHIJKLMNOPQRSTUVWXYZ", 48.0);
    for g in &many.glyphs {
        atlas.glyph(&many.fonts[0], g.id, 48.0, 0.0).unwrap();
    }
    assert!(
        atlas.pages().len() > 1,
        "26 48 px glyphs overflow one 128 × 128 page"
    );
}
