//! Pure paint geometry in UI space: image stretch modes, nine-patch slices, progress fills,
//! separators and gradients. Every function returns rects and UVs; `draw` maps them to pixels.

use concordia_api::{UiMask, UiPaint, UiStop};

use crate::geom::Rect;
use crate::style::Background;
use crate::widgets::{
    AxisStretch, FillMode, ImageMode, MAX_TILES, Orientation, UiProgress, UiSeparator,
};

/// A rect and the `[u0, v0, u1, v1]` it samples.
pub type Piece = (Rect, [f32; 4]);

/// How an image of `size` pixels (sampling `uv`) fills `rect` in `mode`, mirrored as asked.
pub fn image_quads(
    mode: ImageMode,
    rect: Rect,
    size: [f32; 2],
    uv: [f32; 4],
    flip: [bool; 2],
) -> Vec<Piece> {
    let [w, h] = size;
    if w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }
    let fit = (rect.w / w).min(rect.h / h);
    let pieces = match mode {
        ImageMode::Scale => vec![(rect, uv)],
        ImageMode::Tile => tiles(rect, size, uv),
        ImageMode::Keep => vec![(Rect::new(rect.x, rect.y, w, h), uv)],
        ImageMode::KeepCentered => vec![(centered(rect, w, h), uv)],
        ImageMode::KeepAspect => vec![(Rect::new(rect.x, rect.y, w * fit, h * fit), uv)],
        ImageMode::KeepAspectCentered => vec![(centered(rect, w * fit, h * fit), uv)],
        ImageMode::KeepAspectCovered => vec![(rect, covered(rect, size, uv))],
    };
    pieces
        .into_iter()
        .map(|(r, uv)| (r, flipped(uv, flip)))
        .collect()
}

fn centered(rect: Rect, w: f32, h: f32) -> Rect {
    Rect::new(
        rect.x + (rect.w - w) / 2.0,
        rect.y + (rect.h - h) / 2.0,
        w,
        h,
    )
}

fn flipped(uv: [f32; 4], [h, v]: [bool; 2]) -> [f32; 4] {
    let [mut u0, mut v0, mut u1, mut v1] = uv;
    if h {
        std::mem::swap(&mut u0, &mut u1);
    }
    if v {
        std::mem::swap(&mut v0, &mut v1);
    }
    [u0, v0, u1, v1]
}

/// The part of `uv` an image scaled to cover `rect` (centered) shows.
fn covered(rect: Rect, [w, h]: [f32; 2], uv: [f32; 4]) -> [f32; 4] {
    let s = (rect.w / w).max(rect.h / h);
    let (dw, dh) = (w * s, h * s);
    let (ox, oy) = ((rect.w - dw) / 2.0, (rect.h - dh) / 2.0);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    [
        lerp(uv[0], uv[2], -ox / dw),
        lerp(uv[1], uv[3], -oy / dh),
        lerp(uv[0], uv[2], (rect.w - ox) / dw),
        lerp(uv[1], uv[3], (rect.h - oy) / dh),
    ]
}

/// Tiles of `size` from `rect`'s corner, row by row, the last row and column cut.
fn tiles(rect: Rect, size: [f32; 2], uv: [f32; 4]) -> Vec<Piece> {
    let cols = spans(rect.x, rect.w, size[0]);
    let rows = spans(rect.y, rect.h, size[1]);
    let mut out = Vec::new();
    for &(y, h, fv) in &rows {
        for &(x, w, fu) in &cols {
            if out.len() == MAX_TILES {
                return out;
            }
            let u1 = uv[0] + (uv[2] - uv[0]) * fu;
            let v1 = uv[1] + (uv[3] - uv[1]) * fv;
            out.push((Rect::new(x, y, w, h), [uv[0], uv[1], u1, v1]));
        }
    }
    out
}

/// Whole tiles of `tile` along `[start, start + len)`, then a cut one: (start, size, fraction).
fn spans(start: f32, len: f32, tile: f32) -> Vec<(f32, f32, f32)> {
    let mut out = Vec::new();
    let mut at = 0.0;
    while at < len - 1e-4 && out.len() < MAX_TILES {
        let size = tile.min(len - at);
        out.push((start + at, size, size / tile));
        at += tile;
    }
    out
}

/// One axis of a nine-patch cell: (dest start, dest size, source start, source size).
type Seg = (f32, f32, f32, f32);

/// The middle cell's segments along one axis.
fn middle(dest: (f32, f32), src: (f32, f32), mode: AxisStretch, f: f32) -> Vec<Seg> {
    let (d0, span) = dest;
    let (s0, slen) = src;
    let tile = slen * f;
    if tile <= 0.0 || mode == AxisStretch::Stretch {
        return vec![(d0, span, s0, slen)];
    }
    match mode {
        AxisStretch::TileFit => {
            let n = (span / tile).round().max(1.0);
            let size = span / n;
            (0..n as usize)
                .map(|i| (d0 + i as f32 * size, size, s0, slen))
                .collect()
        }
        _ => spans(d0, span, tile)
            .into_iter()
            .map(|(at, size, frac)| (at, size, s0, slen * frac))
            .collect(),
    }
}

/// The three cells along one axis: begin edge, middle, end edge.
fn axis_cells(
    start: f32,
    len: f32,
    src: (f32, f32),
    margins: (f32, f32),
    f: f32,
    mode: AxisStretch,
) -> [Vec<Seg>; 3] {
    let (a, b) = (margins.0 * f, margins.1 * f);
    let (s0, slen) = src;
    [
        vec![(start, a, s0, margins.0)],
        middle(
            (start + a, len - a - b),
            (s0 + margins.0, slen - margins.0 - margins.1),
            mode,
            f,
        ),
        vec![(start + len - b, b, s0 + slen - margins.1, margins.1)],
    ]
}

/// A nine-patch's pieces: `region` (texture pixels `[x, y, w, h]`) of a `texture`-sized image,
/// with fixed `margins` (left, top, right, bottom), into `rect`.
pub fn nine_patch_quads(
    rect: Rect,
    region: [f32; 4],
    texture: [f32; 2],
    margins: [f32; 4],
    axes: [AxisStretch; 2],
    draw_center: bool,
) -> Vec<Piece> {
    let [l, t, r, b] = margins;
    if texture[0] <= 0.0 || texture[1] <= 0.0 {
        return Vec::new();
    }
    let ratio = |space: f32, need: f32| if need > 0.0 { space / need } else { f32::MAX };
    let f = 1f32
        .min(ratio(rect.w, l + r))
        .min(ratio(rect.h, t + b))
        .max(0.0);
    let cols = axis_cells(rect.x, rect.w, (region[0], region[2]), (l, r), f, axes[0]);
    let rows = axis_cells(rect.y, rect.h, (region[1], region[3]), (t, b), f, axes[1]);
    let mut out = Vec::new();
    for (ri, row) in rows.iter().enumerate() {
        for (ci, col) in cols.iter().enumerate() {
            if ri == 1 && ci == 1 && !draw_center {
                continue;
            }
            for &(y, h, sy, sh) in row {
                for &(x, w, sx, sw) in col {
                    if w <= 1e-6 || h <= 1e-6 || out.len() == MAX_TILES {
                        continue;
                    }
                    let uv = [
                        sx / texture[0],
                        sy / texture[1],
                        (sx + sw) / texture[0],
                        (sy + sh) / texture[1],
                    ];
                    out.push((Rect::new(x, y, w, h), uv));
                }
            }
        }
    }
    out
}

/// The filled part of a progress bar in `rect`: its rect and UV, plus a mask for radial modes
/// (`None` when nothing is filled).
pub fn progress_fill(progress: &UiProgress, rect: Rect) -> Option<(Rect, [f32; 4], UiMask)> {
    let f = progress.fraction();
    if f <= 0.0 {
        return None;
    }
    let Rect { x, y, w, h } = rect;
    let linear = |r: Rect, uv: [f32; 4]| Some((r, uv, UiMask::None));
    let lo = (1.0 - f) / 2.0;
    let radial = |cw: f32, ccw: f32| {
        let center = [
            x + progress.radial.center[0] * w,
            y + progress.radial.center[1] * h,
        ];
        let start = progress.radial.start_deg.to_radians();
        Some((
            rect,
            [0.0, 0.0, 1.0, 1.0],
            UiMask::Angular {
                center,
                start,
                cw,
                ccw,
            },
        ))
    };
    let sweep = progress.radial.sweep_deg.to_radians() * f;
    match progress.fill {
        FillMode::LeftToRight => linear(Rect::new(x, y, w * f, h), [0.0, 0.0, f, 1.0]),
        FillMode::RightToLeft => linear(
            Rect::new(x + w * (1.0 - f), y, w * f, h),
            [1.0 - f, 0.0, 1.0, 1.0],
        ),
        FillMode::TopToBottom => linear(Rect::new(x, y, w, h * f), [0.0, 0.0, 1.0, f]),
        FillMode::BottomToTop => linear(
            Rect::new(x, y + h * (1.0 - f), w, h * f),
            [0.0, 1.0 - f, 1.0, 1.0],
        ),
        FillMode::BilinearH => linear(Rect::new(x + w * lo, y, w * f, h), [lo, 0.0, lo + f, 1.0]),
        FillMode::BilinearV => linear(Rect::new(x, y + h * lo, w, h * f), [0.0, lo, 1.0, lo + f]),
        FillMode::Clockwise => radial(sweep, 0.0),
        FillMode::CounterClockwise => radial(0.0, sweep),
        FillMode::ClockwiseAndCounter => radial(sweep / 2.0, sweep / 2.0),
    }
}

/// The line a separator draws in `rect`.
pub fn separator_rect(separator: &UiSeparator, rect: Rect) -> Rect {
    let t = separator.thickness;
    match separator.orientation {
        Orientation::Horizontal => Rect::new(rect.x, rect.y + (rect.h - t) / 2.0, rect.w, t),
        Orientation::Vertical => Rect::new(rect.x + (rect.w - t) / 2.0, rect.y, t, rect.h),
    }
}

/// A background's paint over `rect` (pixels).
pub fn background_paint(background: &Background, rect: Rect) -> UiPaint {
    let stops = |s: &[crate::style::GradientStop]| -> Vec<UiStop> {
        s.iter()
            .map(|g| UiStop {
                offset: g.offset,
                color: g.color,
            })
            .collect()
    };
    let center = [rect.x + rect.w / 2.0, rect.y + rect.h / 2.0];
    match background {
        Background::Solid(color) => UiPaint::Solid(*color),
        Background::Linear {
            angle_deg,
            stops: s,
        } => {
            let (sin, cos) = angle_deg.to_radians().sin_cos();
            let half = ((rect.w * sin).abs() + (rect.h * cos).abs()) / 2.0;
            let (dx, dy) = (sin * half, -cos * half);
            UiPaint::Linear {
                from: [center[0] - dx, center[1] - dy],
                to: [center[0] + dx, center[1] + dy],
                stops: stops(s),
            }
        }
        Background::Radial {
            center: c,
            radius,
            stops: s,
        } => UiPaint::Radial {
            center: [rect.x + c[0] * rect.w, rect.y + c[1] * rect.h],
            radius: [radius[0] * rect.w, radius[1] * rect.h],
            stops: stops(s),
        },
    }
}
