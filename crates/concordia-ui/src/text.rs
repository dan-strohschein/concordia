//! Shaping a `UiText` with parley: styled runs, UAX #14 line breaking, alignment, line limits
//! and ellipsis. The result is presentation data, in logical pixels relative to the text box.

use parley::{
    Alignment, AlignmentOptions, FontData, FontFamily, FontFamilyName, FontStyle, FontWeight,
    LineHeight, OverflowWrap, PositionedLayoutItem, StyleProperty, TextWrapMode,
};

use crate::fonts::{FontSource, UiFonts};
use crate::style::ResolvedStyle;
use crate::widgets::{Overflow, TextAlign, UiText, Wrap};

/// The ellipsis appended to a truncated line.
pub const ELLIPSIS: &str = "…";

/// A shaped text block.
#[derive(Debug, Clone, Default)]
pub struct TextBlock {
    /// Lines, top to bottom.
    pub lines: Vec<TextLine>,
    /// Glyphs, in visual order per line.
    pub glyphs: Vec<PlacedGlyph>,
    /// Underlines `[x, y, w, h]` with their run.
    pub underlines: Vec<([f32; 4], u32)>,
    /// Each run's resolved color (linear RGBA).
    pub colors: Vec<[f32; 4]>,
    /// The fonts glyphs name by index.
    pub fonts: Vec<FontData>,
    /// The widest line.
    pub width: f32,
    /// All lines' height.
    pub height: f32,
}

/// One line's text and metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    /// The source text the line holds.
    pub text: String,
    /// Where it starts after alignment.
    pub x: f32,
    /// Its box's top.
    pub top: f32,
    /// Its baseline.
    pub baseline: f32,
    /// Its box's height.
    pub height: f32,
    /// Its content's width, not counting trailing spaces.
    pub width: f32,
    /// Its first byte in the source text.
    pub start: usize,
    /// One past its last byte.
    pub end: usize,
    /// Caret stops: (byte offset, x) at every cluster edge, ascending by byte.
    pub stops: Vec<(usize, f32)>,
}

/// A glyph at its pen position (on the baseline).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    /// Index into [`TextBlock::fonts`].
    pub font: u32,
    /// The glyph id.
    pub id: u32,
    /// Pen x.
    pub x: f32,
    /// Baseline y.
    pub y: f32,
    /// Font size in logical pixels.
    pub size: f32,
    /// The run it came from.
    pub run: u32,
}

/// Shapes `text` in `style`, wrapped at `max_width` and cut at `max_height` (logical pixels).
pub fn shape(
    fonts: &mut UiFonts,
    source: &mut dyn FontSource,
    text: &UiText,
    style: &ResolvedStyle,
    max_width: Option<f32>,
    max_height: Option<f32>,
) -> TextBlock {
    let runs = resolve_runs(fonts, source, text, style);
    let block = layout(fonts, text, &runs, max_width);
    let keep = lines_that_fit(text, &block, max_height);
    if keep >= block.lines.len() {
        return block;
    }
    match text.overflow {
        Overflow::Ellipsis => ellipsize(fonts, text, &runs, max_width, &block, keep),
        Overflow::Clip | Overflow::Visible => truncate(block, keep),
    }
}

/// One run with every style decided.
#[derive(Debug, Clone)]
struct Run {
    text: String,
    family: String,
    size: f32,
    weight: f32,
    italic: bool,
    color: [f32; 4],
    underline: bool,
}

fn resolve_runs(
    fonts: &mut UiFonts,
    source: &mut dyn FontSource,
    text: &UiText,
    style: &ResolvedStyle,
) -> Vec<Run> {
    let fallback = fonts
        .family(&style.font, source)
        .or_else(|| fonts.family(&crate::style::sans(), source))
        .unwrap_or_default();
    text.runs
        .iter()
        .map(|run| Run {
            text: run.text.clone(),
            family: run
                .font
                .as_ref()
                .and_then(|f| fonts.family(f, source))
                .unwrap_or_else(|| fallback.clone()),
            size: run.size.unwrap_or(style.font_size),
            weight: run.weight.unwrap_or(style.weight),
            italic: run.italic.unwrap_or(style.italic),
            color: run.color.unwrap_or(style.color),
            underline: run.underline,
        })
        .collect()
}

fn alignment(align: TextAlign) -> Alignment {
    match align {
        TextAlign::Start => Alignment::Start,
        TextAlign::Center => Alignment::Center,
        TextAlign::End => Alignment::End,
        TextAlign::Justify => Alignment::Justify,
    }
}

/// Shapes and breaks `runs` into a block.
fn layout(fonts: &mut UiFonts, text: &UiText, runs: &[Run], max_width: Option<f32>) -> TextBlock {
    let joined: String = runs.iter().map(|r| r.text.as_str()).collect();
    let UiFonts { cx, layout, .. } = fonts;
    let mut builder = layout.ranged_builder(cx, &joined, 1.0, false);
    builder.push_default(StyleProperty::LineHeight(LineHeight::MetricsRelative(
        text.line_height,
    )));
    builder.push_default(StyleProperty::Brush(0u32));
    match text.wrap {
        Wrap::None => builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap)),
        Wrap::Word => {}
        Wrap::Anywhere => builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere)),
    }
    let mut start = 0;
    for (i, run) in runs.iter().enumerate() {
        let range = start..start + run.text.len();
        start = range.end;
        let family = FontFamily::Single(FontFamilyName::Named(run.family.clone().into()));
        builder.push(StyleProperty::FontFamily(family), range.clone());
        builder.push(StyleProperty::FontSize(run.size), range.clone());
        builder.push(
            StyleProperty::FontWeight(FontWeight::new(run.weight)),
            range.clone(),
        );
        if run.italic {
            builder.push(StyleProperty::FontStyle(FontStyle::Italic), range.clone());
        }
        builder.push(StyleProperty::Brush(i as u32), range);
    }
    let mut shaped: parley::Layout<u32> = builder.build(&joined);
    shaped.set_alignment(alignment(text.align), AlignmentOptions::default());
    shaped.break_all_lines(max_width);
    collect(&shaped, &joined, runs)
}

/// Reads parley's lines and glyph runs into a block.
fn collect(shaped: &parley::Layout<u32>, joined: &str, runs: &[Run]) -> TextBlock {
    let mut block = TextBlock {
        colors: runs.iter().map(|r| r.color).collect(),
        ..TextBlock::default()
    };
    for line in shaped.lines() {
        let m = line.metrics();
        let width = m.advance - m.hanging_advance;
        let range = line.text_range();
        block.lines.push(TextLine {
            text: joined.get(range.clone()).unwrap_or_default().to_owned(),
            x: m.offset,
            top: m.block_min_coord,
            baseline: m.baseline,
            height: m.block_max_coord - m.block_min_coord,
            width,
            start: range.start,
            end: range.end,
            stops: stops(&line, range.clone(), m.offset),
        });
        block.width = block.width.max(width);
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run_index = glyph_run.style().brush;
            let font = font_index(&mut block.fonts, glyph_run.run().font());
            let size = glyph_run.run().font_size();
            for g in glyph_run.positioned_glyphs() {
                block.glyphs.push(PlacedGlyph {
                    font,
                    id: g.id,
                    x: g.x,
                    y: g.y,
                    size,
                    run: run_index,
                });
            }
            if runs.get(run_index as usize).is_some_and(|r| r.underline) {
                let thickness = (size / 14.0).max(1.0);
                let y = glyph_run.baseline() + size * 0.1;
                block.underlines.push((
                    [glyph_run.offset(), y, glyph_run.advance(), thickness],
                    run_index,
                ));
            }
        }
    }
    block.height = shaped.height();
    block
}

/// A line's caret stops: each cluster's leading edge, then the line's end.
fn stops(
    line: &parley::Line<'_, u32>,
    range: std::ops::Range<usize>,
    offset: f32,
) -> Vec<(usize, f32)> {
    let mut out = Vec::new();
    let mut end_x = offset;
    for item in line.items() {
        let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
            continue;
        };
        let mut x = glyph_run.offset();
        for cluster in glyph_run.run().visual_clusters() {
            out.push((cluster.text_range().start, x));
            x += cluster.advance();
        }
        end_x = end_x.max(x);
    }
    out.retain(|(b, _)| *b < range.end || range.is_empty());
    out.push((range.end, end_x));
    out.sort_by_key(|(b, _)| *b);
    out.dedup_by_key(|(b, _)| *b);
    if out.first().is_none_or(|(b, _)| *b > range.start) {
        out.insert(0, (range.start, offset));
    }
    out
}

/// The x of byte offset `at` (the nearest stop at or before it) and its line's index.
pub fn caret_x(block: &TextBlock, at: usize) -> Option<(f32, usize)> {
    let i = block
        .lines
        .iter()
        .position(|l| at < l.end || (at == l.end && !l.text.ends_with('\n')))
        .or_else(|| block.lines.len().checked_sub(1))?;
    let line = &block.lines[i];
    let x = line
        .stops
        .iter()
        .rev()
        .find(|(b, _)| *b <= at)
        .or(line.stops.first())
        .map_or(line.x, |(_, x)| *x);
    Some((x, i))
}

/// The byte offset of the stop nearest `p` (block space): the line under y (clamped), then the
/// closest x.
pub fn byte_at(block: &TextBlock, p: [f32; 2]) -> usize {
    let Some(line) = block
        .lines
        .iter()
        .find(|l| p[1] < l.top + l.height)
        .or(block.lines.last())
    else {
        return 0;
    };
    line.stops
        .iter()
        .filter(|(b, _)| *b < line.end || !line.text.ends_with('\n'))
        .min_by(|a, b| (a.1 - p[0]).abs().total_cmp(&(b.1 - p[0]).abs()))
        .map_or(line.start, |(b, _)| *b)
}

fn font_index(fonts: &mut Vec<FontData>, font: &FontData) -> u32 {
    let at = fonts
        .iter()
        .position(|f| f.data.id() == font.data.id() && f.index == font.index)
        .unwrap_or_else(|| {
            fonts.push(font.clone());
            fonts.len() - 1
        });
    at as u32
}

/// How many lines `max_lines` and `max_height` leave (at least one).
fn lines_that_fit(text: &UiText, block: &TextBlock, max_height: Option<f32>) -> usize {
    let by_count = text.max_lines.map_or(usize::MAX, |n| n as usize);
    let by_height = match (text.overflow, max_height) {
        (Overflow::Visible, _) | (_, None) => usize::MAX,
        (_, Some(h)) => block
            .lines
            .iter()
            .take_while(|l| l.top + l.height <= h + 1e-3)
            .count(),
    };
    by_count.min(by_height).max(1)
}

/// Keeps the first `keep` lines and their glyphs.
fn truncate(mut block: TextBlock, keep: usize) -> TextBlock {
    let bottom = block.lines[keep - 1].top + block.lines[keep - 1].height;
    block.lines.truncate(keep);
    block.glyphs.retain(|g| g.y <= bottom);
    block.underlines.retain(|(r, _)| r[1] <= bottom);
    block.height = bottom;
    block.width = block.lines.iter().map(|l| l.width).fold(0.0, f32::max);
    block
}

/// The longest prefix of the text through line `keep − 1` that, with "…" appended, still lays
/// out in `keep` lines.
fn ellipsize(
    fonts: &mut UiFonts,
    text: &UiText,
    runs: &[Run],
    max_width: Option<f32>,
    full: &TextBlock,
    keep: usize,
) -> TextBlock {
    let joined: String = runs.iter().map(|r| r.text.as_str()).collect();
    let first: usize = full.lines[..keep - 1].iter().map(|l| l.text.len()).sum();
    let end = first + full.lines[keep - 1].text.len();
    let cuts: Vec<usize> = joined[first..end]
        .char_indices()
        .map(|(i, _)| first + i)
        .chain(std::iter::once(end))
        .rev()
        .collect();
    for cut in cuts {
        let trimmed = cut_runs(runs, cut);
        let block = layout(fonts, text, &trimmed, max_width);
        if block.lines.len() <= keep {
            return block;
        }
    }
    truncate(full.clone(), keep)
}

/// `runs` cut to their first `bytes` bytes, with the ellipsis appended to the last run kept.
fn cut_runs(runs: &[Run], bytes: usize) -> Vec<Run> {
    let mut out = Vec::new();
    let mut left = bytes;
    for run in runs {
        if left == 0 && !out.is_empty() {
            break;
        }
        let take = left.min(run.text.len());
        out.push(Run {
            text: run.text[..take].to_owned(),
            ..run.clone()
        });
        left -= take;
    }
    if let Some(last) = out.last_mut() {
        last.text.push_str(ELLIPSIS);
    }
    out
}
