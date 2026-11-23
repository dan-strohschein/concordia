//! Concordia in-game UI (M2 phase 2.7b, ADR-0060): retained widgets as ECS components, laid out
//! by taffy (flex, grid, stack) plus Godot-style anchors, text shaped by parley and rasterized by
//! swash into a glyph atlas, `.theme.ron` themes, and the draw list the renderer's UI pass draws.
//! Layout, text and atlases are derived presentation data; only the components are hashed.
#![forbid(unsafe_code)]

mod anchor;
mod assets;
mod atlas;
mod components;
mod context;
mod controls;
mod draw;
mod error;
mod focus;
mod fonts;
mod geom;
mod hit;
mod interact;
mod layout;
mod methods;
mod module;
mod node;
mod paint;
mod parts;
mod play;
mod root;
mod scene;
mod style;
mod text;
mod theme;
mod widgets;

pub use anchor::{PRESETS, Preset, anchored_rect};
pub use assets::{AssetCache, DbAssets, ImageData, MemoryAssets, UiAssets};
pub use atlas::{AtlasPage, GlyphAtlas, GlyphSlot, PAGE_SIZE};
pub use components::register_types;
pub use context::UiContext;
pub use controls::{
    ActionOn, Grab, MAX_LABEL_BYTES, MAX_OPTIONS, MAX_TITLES, MAX_TOOLTIP_BYTES, ScrollBar,
    ToggleLook, UiButton, UiDropdown, UiFocus, UiFocusChanged, UiPressed, UiScroll, UiSelected,
    UiSlider, UiSounds, UiState, UiTabs, UiToggle, UiToggled, UiTooltip, UiValueChanged,
};
pub use draw::{
    BOUNDS_COLOR, FOCUS_COLOR, KNOB_DARK, LIST_BG, TIP_BG, draw_list, half_texel_inset,
};
pub use error::UiError;
pub use focus::{Dir, arrow, focusable, step, tab_order};
pub use fonts::{Font, FontImporter, FontSource, NoFonts, UiFonts, font_families, read_font};
pub use geom::{Edges, Rect};
pub use hit::{Hit, HitPart, Index, hit_test, hit_with, stops};
pub use interact::{
    CLICK, KEYS, Keys, POINTER, Press, SCROLL, UiInput, VIEWPORT, interact, sim_viewport,
};
pub use layout::{NodeLayout, RootLayout, UiLayout, compute_layout, widget_state};
pub use methods::{
    LineReport, NodeReport, ProblemReport, RootReport, UiHitTest, UiHitTestParams, UiHitTestResult,
    UiInspect, UiInspectParams, UiInspectResult,
};
pub use module::{UI_BOUNDS_LAYER, UiCore, UiModule, UiShared};
pub use node::{
    Align, Anchored, Direction, Flex, Grid, Grow, Justify, Layout, Length, MAX_TRACKS,
    PointerFilter, Position, Track, UiNode,
};
pub use paint::{
    Piece, background_paint, image_quads, nine_patch_quads, progress_fill, separator_rect,
};
pub use parts::{
    Bar, GRABBER, Label, List, MIN_THUMB, Parts, ROW_PAD, Row, SLIDER_SIZE, TAB_PAD_X, TAB_PAD_Y,
    TIP_OFFSET, TIP_PAD, TRACK, Tip, clamp_offset, dropdown_arrow, first_row, list_rect,
    offset_for_thumb, scroll_bars, slider_parts, slider_value_at, tip_rect, toggle_knob,
};
pub use play::{INTERACT, add_interact, preload};
pub use root::{Aspect, CanvasMap, MAX_CANVAS, RootSpace, Stretch, UiRoot, canvas_map};
pub use scene::{UiElement, UiScene, UiTree};
pub use style::{
    Background, Border, Cursor, GradientStop, MAX_FONT_SIZE, MAX_STOPS, MIN_STOPS, MONO,
    ResolvedStyle, SANS, UiStyle, UiThemed, WidgetState,
};
pub use text::{ELLIPSIS, PlacedGlyph, TextBlock, TextLine, shape};
pub use theme::{Theme, ThemeClass, ThemeImporter, read_theme, resolve, write_theme};
pub use widgets::{
    AxisStretch, FillMode, ImageMode, MAX_TEXT_BYTES, MAX_TILES, Orientation, Overflow, RadialFill,
    TextAlign, TextRun, UiImage, UiNinePatch, UiProgress, UiSeparator, UiText, VAlign, Wrap,
};

/// The importers every project registers for UI (`font`: ttf, otf; `theme`: .theme.ron).
pub fn importers() -> Vec<std::sync::Arc<dyn concordia_assets::Importer>> {
    vec![
        std::sync::Arc::new(FontImporter),
        std::sync::Arc::new(ThemeImporter),
    ]
}
