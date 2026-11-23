//! Widget parts: the sub-rects of interactive widgets (a toggle's box, a slider's track and
//! grabber, a dropdown's list, tab titles, scroll bars, tooltips) in UI space. Layout computes
//! them once; drawing and hit tests both read them. Pure geometry.

use crate::controls::{ToggleLook, UiScroll, UiSlider};
use crate::geom::Rect;
use crate::text::TextBlock;
use crate::widgets::Orientation;

/// The grabber's largest side.
pub const GRABBER: f32 = 16.0;
/// The slider track's thickness.
pub const TRACK: f32 = 4.0;
/// A horizontal slider's size when `Auto`.
pub const SLIDER_SIZE: [f32; 2] = [160.0, 20.0];
/// Space above and below a dropdown row's text.
pub const ROW_PAD: f32 = 4.0;
/// Space above and below a tab title.
pub const TAB_PAD_Y: f32 = 6.0;
/// Space left and right of a tab title.
pub const TAB_PAD_X: f32 = 12.0;
/// A tooltip's offset from the pointer.
pub const TIP_OFFSET: [f32; 2] = [12.0, 18.0];
/// Space around a tooltip's text.
pub const TIP_PAD: f32 = 6.0;
/// The smallest scroll thumb.
pub const MIN_THUMB: f32 = 16.0;

/// A label at a rect (its text drawn at the rect's padded origin).
#[derive(Debug, Clone)]
pub struct Label {
    /// Where it sits.
    pub rect: Rect,
    /// Its shaped text.
    pub text: TextBlock,
}

/// One row of an open dropdown.
#[derive(Debug, Clone)]
pub struct Row {
    /// The option's index.
    pub index: u32,
    /// Its box.
    pub rect: Rect,
    /// Its label.
    pub text: TextBlock,
}

/// An open dropdown's list (drawn above everything).
#[derive(Debug, Clone)]
pub struct List {
    /// Its box.
    pub rect: Rect,
    /// The rows showing.
    pub rows: Vec<Row>,
    /// The highlighted option.
    pub highlight: u32,
}

/// A scroll bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    /// The whole bar.
    pub track: Rect,
    /// The draggable part.
    pub thumb: Rect,
}

/// The width of a spin box's arrow column.
pub const SPIN_W: f32 = 20.0;
/// The caret's width.
pub const CARET_W: f32 = 1.5;
/// How far inside a single-line box the caret is kept when scrolling.
pub const CARET_MARGIN: f32 = 2.0;
/// A single-line text box's size when `Auto`.
pub const TEXT_BOX_SIZE: [f32; 2] = [160.0, 0.0];

/// A text box's parts (UI space).
#[derive(Debug, Clone)]
pub struct TextBoxParts {
    /// What it draws: the (masked) text, or the placeholder.
    pub text: TextBlock,
    /// Where the text block's origin sits (scroll applied).
    pub origin: [f32; 2],
    /// The text is the placeholder.
    pub placeholder: bool,
    /// The caret, when focused.
    pub caret: Option<Rect>,
    /// The selection, one rect per line.
    pub selection: Vec<Rect>,
    /// How far single-line text is scrolled left.
    pub scroll: f32,
    /// A spin box's up and down arrows.
    pub spin: Option<(Rect, Rect)>,
}

/// A widget's parts.
#[derive(Debug, Clone)]
pub enum Parts {
    /// A checkbox's box or a switch's track.
    Toggle {
        /// The box.
        knob: Rect,
    },
    /// A slider.
    Slider {
        /// The track.
        track: Rect,
        /// The grabber.
        grabber: Rect,
    },
    /// A dropdown.
    Dropdown {
        /// The selected option's label (drawn at the content's left).
        face: Option<TextBlock>,
        /// The arrow's box (a downward triangle).
        arrow: Rect,
        /// The list, when open.
        list: Option<List>,
    },
    /// A text box (and a spin box's arrows).
    TextBox(TextBoxParts),
    /// Tab titles, left to right.
    Tabs {
        /// One per page.
        tabs: Vec<Label>,
    },
    /// A scroll container.
    Scroll {
        /// The clamped offset.
        offset: [f32; 2],
        /// The content's size.
        content: [f32; 2],
        /// The vertical bar, when shown.
        vertical: Option<Bar>,
        /// The horizontal bar, when shown.
        horizontal: Option<Bar>,
    },
}

/// A visible tooltip.
#[derive(Debug, Clone)]
pub struct Tip {
    /// Its box (UI space of its root).
    pub rect: Rect,
    /// Its text, drawn `TIP_PAD` inside.
    pub text: TextBlock,
}

/// The box a toggle draws, at `content`'s left and centered: side `size` (twice as wide for a
/// switch). `None` for the `Button` look.
pub fn toggle_knob(look: ToggleLook, content: Rect, size: f32) -> Option<Rect> {
    let w = match look {
        ToggleLook::Checkbox => size,
        ToggleLook::Switch => 2.0 * size,
        ToggleLook::Button => return None,
    };
    Some(Rect::new(
        content.x,
        content.y + (content.h - size) / 2.0,
        w,
        size,
    ))
}

/// The track and grabber of `slider` in `content`.
pub fn slider_parts(slider: &UiSlider, content: Rect) -> (Rect, Rect) {
    let f = slider.fraction();
    match slider.orientation {
        Orientation::Horizontal => {
            let g = content.h.min(GRABBER);
            let cy = content.y + content.h / 2.0;
            let track = Rect::new(
                content.x + g / 2.0,
                cy - TRACK / 2.0,
                (content.w - g).max(0.0),
                TRACK,
            );
            let grabber = Rect::new(track.x + f * track.w - g / 2.0, cy - g / 2.0, g, g);
            (track, grabber)
        }
        Orientation::Vertical => {
            let g = content.w.min(GRABBER);
            let cx = content.x + content.w / 2.0;
            let track = Rect::new(
                cx - TRACK / 2.0,
                content.y + g / 2.0,
                TRACK,
                (content.h - g).max(0.0),
            );
            let bottom = track.y + track.h;
            let grabber = Rect::new(cx - g / 2.0, bottom - f * track.h - g / 2.0, g, g);
            (track, grabber)
        }
    }
}

/// The slider value at a point on its track (snapped, clamped).
pub fn slider_value_at(slider: &UiSlider, track: Rect, p: [f32; 2]) -> f32 {
    let f = match slider.orientation {
        Orientation::Horizontal if track.w > 0.0 => (p[0] - track.x) / track.w,
        Orientation::Vertical if track.h > 0.0 => (track.y + track.h - p[1]) / track.h,
        _ => 0.0,
    };
    slider.snap(slider.min + f.clamp(0.0, 1.0) * (slider.max - slider.min))
}

/// The arrow's box: half the font size wide, a quarter tall, at `content`'s right, centered.
pub fn dropdown_arrow(content: Rect, size: f32) -> Rect {
    let a = size * 0.5;
    Rect::new(
        content.x + content.w - a,
        content.y + (content.h - a / 2.0) / 2.0,
        a,
        a / 2.0,
    )
}

/// The first row shown so `highlight` is visible in `visible` rows of `count`.
pub fn first_row(highlight: u32, visible: u32, count: u32) -> u32 {
    let first = (highlight + 1).saturating_sub(visible);
    first.min(count.saturating_sub(visible))
}

/// The list's box under `face` (above it when it would leave a `space_h`-tall root).
pub fn list_rect(face: Rect, rows: u32, row_h: f32, space_h: f32) -> Rect {
    let h = rows as f32 * row_h;
    let below = face.y + face.h;
    let y = if below + h > space_h && face.y - h >= 0.0 {
        face.y - h
    } else {
        below
    };
    Rect::new(face.x, y, face.w, h)
}

/// The offset clamped to 0..(content − view) on scrolling axes (0 on the others).
pub fn clamp_offset(scroll: &UiScroll, view: [f32; 2], content: [f32; 2]) -> [f32; 2] {
    let axis = |i: usize| match scroll.scrolls(i) {
        true => scroll.offset[i].clamp(0.0, (content[i] - view[i]).max(0.0)),
        false => 0.0,
    };
    [axis(0), axis(1)]
}

/// The bars of a scroll at `rect` with `content` scrolled by `offset` (overlaying its edges).
pub fn scroll_bars(
    scroll: &UiScroll,
    rect: Rect,
    content: [f32; 2],
    offset: [f32; 2],
) -> (Option<Bar>, Option<Bar>) {
    use crate::controls::ScrollBar::{Always, Auto};
    let view = [rect.w, rect.h];
    let shows = |i: usize, bar| match bar {
        Always => true,
        Auto => content[i] > view[i] + 0.5,
        _ => false,
    };
    let (h, v) = (shows(0, scroll.horizontal), shows(1, scroll.vertical));
    let bw = scroll.bar_width;
    let vertical = v.then(|| {
        let track = Rect::new(
            rect.x + rect.w - bw,
            rect.y,
            bw,
            rect.h - if h { bw } else { 0.0 },
        );
        bar(track, 1, view[1], content[1], offset[1])
    });
    let horizontal = h.then(|| {
        let track = Rect::new(
            rect.x,
            rect.y + rect.h - bw,
            rect.w - if v { bw } else { 0.0 },
            bw,
        );
        bar(track, 0, view[0], content[0], offset[0])
    });
    (vertical, horizontal)
}

/// A bar along `axis`: the thumb is view / content of the track, placed by offset / range.
fn bar(track: Rect, axis: usize, view: f32, content: f32, offset: f32) -> Bar {
    let len = if axis == 0 { track.w } else { track.h };
    let thumb_len = (len * (view / content.max(view))).clamp(MIN_THUMB.min(len), len);
    let range = (content - view).max(0.0);
    let at = if range > 0.0 {
        offset / range * (len - thumb_len)
    } else {
        0.0
    };
    let thumb = if axis == 0 {
        Rect::new(track.x + at, track.y, thumb_len, track.h)
    } else {
        Rect::new(track.x, track.y + at, track.w, thumb_len)
    };
    Bar { track, thumb }
}

/// The offset a thumb dragged so its start is at `start` (along its track) gives.
pub fn offset_for_thumb(bar: &Bar, axis: usize, start: f32, view: f32, content: f32) -> f32 {
    let (origin, len, thumb) = if axis == 0 {
        (bar.track.x, bar.track.w, bar.thumb.w)
    } else {
        (bar.track.y, bar.track.h, bar.thumb.h)
    };
    let free = len - thumb;
    let range = (content - view).max(0.0);
    match free > 0.0 {
        true => ((start - origin) / free).clamp(0.0, 1.0) * range,
        false => 0.0,
    }
}

/// A tooltip's box: `TIP_OFFSET` from the point, padded, kept inside `space`.
pub fn tip_rect(at: [f32; 2], text: [f32; 2], space: [f32; 2]) -> Rect {
    let w = text[0] + 2.0 * TIP_PAD;
    let h = text[1] + 2.0 * TIP_PAD;
    let x = (at[0] + TIP_OFFSET[0]).min(space[0] - w).max(0.0);
    let y = (at[1] + TIP_OFFSET[1]).min(space[1] - h).max(0.0);
    Rect::new(x, y, w, h)
}
