//! Font table readers for hand-derived expectations (OpenType spec: the table directory,
//! `head`, `hhea`, `OS/2`), and shared fixtures.
#![allow(dead_code, missing_docs)]

use std::path::PathBuf;

use concordia_core::id::{DeterministicIdGen, EntityId};
use concordia_scene::builtin::Parent;
use concordia_terra::World;
use concordia_ui::*;

/// A world of UI entities.
pub struct Tree {
    pub world: World,
    pub ids: DeterministicIdGen,
}

impl Tree {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            ids: DeterministicIdGen::new(3),
        }
    }

    pub fn root(&mut self, root: UiRoot, node: UiNode) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        self.world.insert(id, root).unwrap();
        self.world.insert(id, node).unwrap();
        id
    }

    pub fn child(&mut self, parent: EntityId, node: UiNode) -> EntityId {
        let id = self.world.spawn(&mut self.ids).unwrap();
        self.world.insert(id, node).unwrap();
        self.world.insert(id, Parent(parent)).unwrap();
        id
    }

    pub fn layout(&self, viewport: [f32; 2]) -> UiLayout {
        self.layout_with(viewport, &mut MemoryAssets::default())
    }

    pub fn layout_with(&self, viewport: [f32; 2], assets: &mut MemoryAssets) -> UiLayout {
        let scene = UiScene::gather(&self.world);
        compute_layout(&scene, &mut UiFonts::new(), assets, viewport)
    }
}

pub fn font_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fonts")
        .join(file)
}

pub fn font_bytes(file: &str) -> Vec<u8> {
    std::fs::read(font_path(file)).unwrap()
}

/// The metrics a layout engine derives from a font's tables.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub units_per_em: f32,
    pub ascender: f32,
    pub descender: f32,
    pub line_gap: f32,
    pub advance_width_max: f32,
    /// `M`'s advance (from `cmap` and `hmtx`).
    pub advance_m: f32,
}

impl Metrics {
    /// Ascender − descender + line gap, at `size` pixels.
    pub fn line_height(&self, size: f32) -> f32 {
        (self.ascender - self.descender + self.line_gap) * size / self.units_per_em
    }

    pub fn ascent(&self, size: f32) -> f32 {
        self.ascender * size / self.units_per_em
    }

    /// A monospaced font's advance at `size` pixels: its text glyphs all advance as `M` does
    /// (`advanceWidthMax` is no guide; wide symbols exceed it).
    pub fn mono_advance(&self, size: f32) -> f32 {
        self.advance_m * size / self.units_per_em
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

fn i16_at(b: &[u8], at: usize) -> i16 {
    i16::from_be_bytes([b[at], b[at + 1]])
}

fn table<'a>(b: &'a [u8], tag: &[u8; 4]) -> &'a [u8] {
    let count = u16_at(b, 4) as usize;
    for i in 0..count {
        let rec = 12 + i * 16;
        if &b[rec..rec + 4] == tag {
            let off = u32::from_be_bytes(b[rec + 8..rec + 12].try_into().unwrap()) as usize;
            let len = u32::from_be_bytes(b[rec + 12..rec + 16].try_into().unwrap()) as usize;
            return &b[off..off + len];
        }
    }
    panic!("no {tag:?} table");
}

/// The glyph id `ch` maps to through the `cmap` format 4 subtable (platform 3, encoding 1).
pub fn glyph_id(bytes: &[u8], ch: char) -> u16 {
    let cmap = table(bytes, b"cmap");
    let c = ch as u32 as u16;
    for i in 0..u16_at(cmap, 2) as usize {
        let rec = 4 + i * 8;
        let (platform, encoding) = (u16_at(cmap, rec), u16_at(cmap, rec + 2));
        let off = u32::from_be_bytes(cmap[rec + 4..rec + 8].try_into().unwrap()) as usize;
        let sub = &cmap[off..];
        if platform != 3 || encoding != 1 || u16_at(sub, 0) != 4 {
            continue;
        }
        let segs = u16_at(sub, 6) as usize / 2;
        let (ends, starts, deltas, ranges) = (14, 16 + segs * 2, 16 + segs * 4, 16 + segs * 6);
        for s in 0..segs {
            let (end, start) = (u16_at(sub, ends + s * 2), u16_at(sub, starts + s * 2));
            if c < start || c > end {
                continue;
            }
            let delta = u16_at(sub, deltas + s * 2);
            let range = u16_at(sub, ranges + s * 2) as usize;
            if range == 0 {
                return c.wrapping_add(delta);
            }
            let at = ranges + s * 2 + range + (c - start) as usize * 2;
            let g = u16_at(sub, at);
            return if g == 0 { 0 } else { g.wrapping_add(delta) };
        }
    }
    0
}

/// Glyph `gid`'s advance in font units (`hmtx`; ids past `numberOfHMetrics` take the last one).
pub fn advance(bytes: &[u8], gid: u16) -> u16 {
    let hhea = table(bytes, b"hhea");
    let hmtx = table(bytes, b"hmtx");
    let count = u16_at(hhea, 34) as usize;
    let i = (gid as usize).min(count - 1);
    u16_at(hmtx, i * 4)
}

/// Reads `head`, `hhea` and `OS/2`. The vertical metrics follow the OpenType spec's rule: the
/// `OS/2` typo metrics when `fsSelection` bit 7 (USE_TYPO_METRICS) is set, else `hhea`'s.
pub fn metrics(bytes: &[u8]) -> Metrics {
    let head = table(bytes, b"head");
    let hhea = table(bytes, b"hhea");
    let os2 = table(bytes, b"OS/2");
    let use_typo = u16_at(os2, 62) & (1 << 7) != 0;
    let (ascender, descender, line_gap) = match use_typo {
        true => (i16_at(os2, 68), i16_at(os2, 70), i16_at(os2, 72)),
        false => (i16_at(hhea, 4), i16_at(hhea, 6), i16_at(hhea, 8)),
    };
    Metrics {
        units_per_em: f32::from(u16_at(head, 18)),
        ascender: f32::from(ascender),
        descender: f32::from(descender),
        line_gap: f32::from(line_gap),
        advance_width_max: f32::from(u16_at(hhea, 10)),
        advance_m: f32::from(advance(bytes, glyph_id(bytes, 'M'))),
    }
}

pub fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[track_caller]
pub fn assert_close(a: f32, b: f32) {
    assert!(close(a, b), "{a} != {b}");
}

/// A two-color checker image (sRGB), `cell` pixels per square.
pub fn checker(w: u32, h: u32, cell: u32, a: [u8; 4], b: [u8; 4]) -> ImageData {
    let pixels: Vec<u8> = (0..h)
        .flat_map(|y| {
            (0..w).map(move |x| {
                if ((x / cell) + (y / cell)).is_multiple_of(2) {
                    a
                } else {
                    b
                }
            })
        })
        .flatten()
        .collect();
    ImageData {
        width: w,
        height: h,
        srgb: true,
        pixels: pixels.into(),
    }
}

/// A frame image for nine-patches: a 4 px light border, a 4 px dark band, a mid center.
pub fn frame_image(side: u32) -> ImageData {
    let px = |x: u32, y: u32| {
        let edge = x.min(y).min(side - 1 - x).min(side - 1 - y);
        match edge {
            0..=3 => [220, 200, 140, 255],
            4..=7 => [60, 50, 30, 255],
            _ => [30, 30, 40, 230],
        }
    };
    let pixels: Vec<u8> = (0..side)
        .flat_map(|y| (0..side).map(move |x| px(x, y)))
        .flatten()
        .collect();
    ImageData {
        width: side,
        height: side,
        srgb: true,
        pixels: pixels.into(),
    }
}

fn anchored(preset: Preset, size: [f32; 2], margin: f32) -> UiNode {
    UiNode {
        position: Position::Anchored(Anchored::preset(preset, size, margin)),
        ..UiNode::default()
    }
}

fn stops(a: [f32; 4], b: [f32; 4]) -> Vec<GradientStop> {
    vec![
        GradientStop {
            offset: 0.0,
            color: a,
        },
        GradientStop {
            offset: 1.0,
            color: b,
        },
    ]
}

/// An original HUD fixture on a 640 × 360 canvas (Keep): a health panel (icon, label, bar), a
/// score label, a radial gauge, a separator and a nine-patch frame; `fill` is the bar's and the
/// gauge's fraction.
pub fn hud_fixture(fill: f32) -> (Tree, MemoryAssets) {
    let icon: concordia_assets::AssetRef = "builtin:texture/hud_icon".parse().unwrap();
    let frame: concordia_assets::AssetRef = "builtin:texture/hud_frame".parse().unwrap();
    let mut assets = MemoryAssets::default();
    assets.insert_image(
        &icon,
        checker(16, 16, 4, [240, 80, 80, 255], [250, 220, 220, 255]),
    );
    assets.insert_image(&frame, frame_image(24));
    let mut t = Tree::new();
    let root = t.root(
        UiRoot {
            canvas: [640.0, 360.0],
            stretch: Stretch::Scale,
            ..UiRoot::default()
        },
        UiNode::default(),
    );
    let panel = t.child(
        root,
        UiNode {
            padding: Edges::all(8.0),
            gap: [6.0, 6.0],
            layout: Layout::Flex(Flex {
                direction: Direction::Column,
                ..Flex::default()
            }),
            ..anchored(Preset::TopLeft, [240.0, 64.0], 12.0)
        },
    );
    t.world
        .insert(
            panel,
            UiStyle {
                background: Some(Background::Linear {
                    angle_deg: 180.0,
                    stops: stops([0.05, 0.08, 0.2, 0.9], [0.01, 0.02, 0.06, 0.9]),
                }),
                border: Some(Border {
                    width: 2.0,
                    color: [0.6, 0.65, 0.8, 1.0],
                }),
                radius: Some([8.0; 4]),
                color: Some([1.0; 4]),
                ..UiStyle::default()
            },
        )
        .unwrap();
    let row = t.child(
        panel,
        UiNode {
            gap: [6.0, 0.0],
            layout: Layout::Flex(Flex {
                align_items: Align::Center,
                ..Flex::default()
            }),
            ..UiNode::default()
        },
    );
    let pic = t.child(
        row,
        UiNode {
            width: Length::Px(16.0),
            height: Length::Px(16.0),
            ..UiNode::default()
        },
    );
    t.world
        .insert(
            pic,
            UiImage {
                image: icon,
                region: None,
                mode: ImageMode::Scale,
                tint: [1.0; 4],
                flip_h: false,
                flip_v: false,
            },
        )
        .unwrap();
    let label = t.child(row, UiNode::default());
    t.world
        .insert(
            label,
            UiText::plain(format!("Health {}%", (fill * 100.0).round())),
        )
        .unwrap();
    t.world
        .insert(
            label,
            UiStyle {
                font_size: Some(16.0),
                weight: Some(700.0),
                ..UiStyle::default()
            },
        )
        .unwrap();
    let bar = t.child(
        panel,
        UiNode {
            height: Length::Px(14.0),
            ..UiNode::default()
        },
    );
    t.world
        .insert(
            bar,
            UiProgress {
                value: fill * 100.0,
                track: Some(Background::Solid([0.1, 0.1, 0.12, 1.0])),
                bar: Some(Background::Linear {
                    angle_deg: 90.0,
                    stops: stops([0.8, 0.1, 0.1, 1.0], [1.0, 0.55, 0.1, 1.0]),
                }),
                ..UiProgress::default()
            },
        )
        .unwrap();
    t.world
        .insert(
            bar,
            UiStyle {
                radius: Some([4.0; 4]),
                ..UiStyle::default()
            },
        )
        .unwrap();
    let score = t.child(root, anchored(Preset::TopRight, [160.0, 28.0], 12.0));
    t.world
        .insert(
            score,
            UiText {
                align: TextAlign::End,
                ..UiText::plain("Score 1200")
            },
        )
        .unwrap();
    t.world
        .insert(
            score,
            UiStyle {
                font_size: Some(20.0),
                color: Some([1.0, 0.9, 0.4, 1.0]),
                ..UiStyle::default()
            },
        )
        .unwrap();
    let gauge = t.child(root, anchored(Preset::CenterBottom, [64.0, 64.0], 16.0));
    t.world
        .insert(
            gauge,
            UiProgress {
                value: fill * 100.0,
                fill: FillMode::Clockwise,
                track: Some(Background::Solid([0.1, 0.1, 0.12, 0.8])),
                bar: Some(Background::Radial {
                    center: [0.5, 0.5],
                    radius: [0.5, 0.5],
                    stops: stops([0.4, 1.0, 0.5, 1.0], [0.1, 0.5, 0.2, 1.0]),
                }),
                ..UiProgress::default()
            },
        )
        .unwrap();
    t.world
        .insert(
            gauge,
            UiStyle {
                radius: Some([32.0; 4]),
                ..UiStyle::default()
            },
        )
        .unwrap();
    let line = t.child(root, anchored(Preset::BottomWide, [0.0, 4.0], 96.0));
    t.world
        .insert(
            line,
            UiSeparator {
                thickness: 2.0,
                color: [0.7, 0.7, 0.8, 0.8],
                ..UiSeparator::default()
            },
        )
        .unwrap();
    let boxy = t.child(
        root,
        UiNode {
            padding: Edges::all(10.0),
            ..anchored(Preset::BottomLeft, [120.0, 48.0], 12.0)
        },
    );
    t.world
        .insert(
            boxy,
            UiNinePatch {
                image: frame,
                region: None,
                margins: [8; 4],
                axis_h: AxisStretch::Stretch,
                axis_v: AxisStretch::Stretch,
                draw_center: true,
                tint: [1.0; 4],
            },
        )
        .unwrap();
    let inside = t.child(boxy, UiNode::default());
    t.world.insert(inside, UiText::plain("Ready")).unwrap();
    t.world
        .insert(
            inside,
            UiStyle {
                font_size: Some(14.0),
                ..UiStyle::default()
            },
        )
        .unwrap();
    (t, assets)
}

fn r(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

/// One line per quad: its kind, rect, clip (when bounded), radius, border and opacity.
pub fn summary(list: &concordia_api::UiDrawList) -> String {
    use concordia_api::UiPaint;
    let mut out = String::new();
    for q in &list.quads {
        let kind = match &q.paint {
            UiPaint::None => "outline".to_owned(),
            UiPaint::Solid(c) => format!("solid {:?}", c.map(r)),
            UiPaint::Linear { from, to, stops } => format!(
                "linear {:?}->{:?} stops {}",
                from.map(r),
                to.map(r),
                stops.len()
            ),
            UiPaint::Radial {
                center,
                radius,
                stops,
            } => format!(
                "radial {:?} r {:?} stops {}",
                center.map(r),
                radius.map(r),
                stops.len()
            ),
            UiPaint::Texture { texture, uv, .. } => format!("texture {texture} uv {:?}", uv.map(r)),
            UiPaint::Glyph { texture, .. } => format!("glyph page {texture}"),
        };
        let mut line = format!("{kind} @ {:?}", q.rect.map(r));
        if q.clip[0] > -1.0e6 {
            line += &format!(" clip {:?}", q.clip.map(r));
        }
        if q.radius != [0.0; 4] {
            line += &format!(" radius {:?}", q.radius.map(r));
        }
        if q.border > 0.0 {
            line += &format!(" border {}", r(q.border));
        }
        if q.opacity != 1.0 {
            line += &format!(" opacity {}", r(q.opacity));
        }
        if q.mask != concordia_api::UiMask::None {
            line += " masked";
        }
        out += &line;
        out.push('\n');
    }
    for t in &list.textures {
        out += &format!("texture {}x{} {:?}\n", t.width, t.height, t.format);
    }
    out
}

pub mod controls;
pub mod play;

#[allow(unused_imports)]
pub use controls::controls_fixture;
