//! The draw list: UI space mapped to target pixels through the root's canvas map and the frame's
//! scale, with styles, clips, opacity, textures, glyphs and fills. Values derived by hand.

mod common;

use common::{Tree, close};
use concordia_api::{UiDrawList, UiFrame, UiMask, UiPaint, UiQuad, UiTextureFormat};
use concordia_assets::AssetRef;
use concordia_core::id::EntityId;
use concordia_ui::*;

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// A node pinned at UI (10, 10, 100 × 50) by offsets.
fn pinned(x: f32, y: f32, w: f32, h: f32) -> UiNode {
    UiNode {
        position: Position::Anchored(Anchored {
            offsets: Edges::new(x, y, x + w, y + h),
            ..Anchored::default()
        }),
        ..UiNode::default()
    }
}

fn boxed(color: [f32; 4]) -> UiStyle {
    UiStyle {
        background: Some(Background::Solid(color)),
        ..UiStyle::default()
    }
}

fn draw(t: &Tree, assets: &mut MemoryAssets, frame: UiFrame) -> UiDrawList {
    UiContext::default().draw(&t.world, assets, &frame)
}

fn quad_of(list: &UiDrawList, paint: &UiPaint) -> UiQuad {
    list.quads
        .iter()
        .find(|q| &q.paint == paint)
        .cloned()
        .unwrap_or_else(|| panic!("no {paint:?} in {list:#?}"))
}

#[track_caller]
fn rect4(got: [f32; 4], want: [f32; 4]) {
    assert!(
        got.iter().zip(want).all(|(a, b)| close(*a, b)),
        "{got:?} != {want:?}"
    );
}

fn keep_root(t: &mut Tree) -> EntityId {
    t.root(
        UiRoot {
            canvas: [800.0, 400.0],
            stretch: Stretch::Scale,
            ..UiRoot::default()
        },
        UiNode::default(),
    )
}

/// Canvas 800 × 400 (Keep). In a 1600 × 900 frame at scale 1 the map is ×2 with offset (0, 50):
/// UI (10, 10, 100 × 50) → (20, 70, 200 × 100); radius 4 → 8; border 2 → 4. At DPI scale 2 the
/// logical viewport is 800 × 450, so the map is ×1 with offset (0, 25): (10, 35, 100 × 50)
/// logical, ×2 → the same pixels.
#[test]
fn draw_maps_ui_space_to_pixels_with_stretch_and_dpi() {
    let mut t = Tree::new();
    let root = keep_root(&mut t);
    let node = t.child(root, pinned(10.0, 10.0, 100.0, 50.0));
    let style = UiStyle {
        radius: Some([4.0; 4]),
        border: Some(Border {
            width: 2.0,
            color: BLUE,
        }),
        ..boxed(RED)
    };
    t.world.insert(node, style).unwrap();
    for scale in [1.0, 2.0] {
        let frame = UiFrame {
            scale,
            ..UiFrame::new(1600, 900)
        };
        let list = draw(&t, &mut MemoryAssets::default(), frame);
        assert_eq!(list.quads.len(), 1, "the root draws nothing");
        let q = &list.quads[0];
        rect4(q.rect, [20.0, 70.0, 200.0, 100.0]);
        assert_eq!(q.radius, [8.0; 4]);
        assert_eq!((q.border, q.border_color), (4.0, BLUE));
        assert_eq!(q.paint, UiPaint::Solid(RED));
    }
    assert!(
        draw(
            &Tree::new(),
            &mut MemoryAssets::default(),
            UiFrame::new(64, 64)
        )
        .is_empty()
    );
}

/// A clipping parent at (10, 10, 100 × 50) with opacity 0.5 over a child of opacity 0.5: the
/// child's quad has opacity 0.25 and clip (10, 10, 110, 60). The parent itself is unclipped.
#[test]
fn opacity_multiplies_and_clips_reach_children() {
    let mut t = Tree::new();
    let root = t.root(
        UiRoot {
            canvas: [400.0, 300.0],
            ..UiRoot::default()
        },
        UiNode::default(),
    );
    let parent = t.child(
        root,
        UiNode {
            clip: true,
            ..pinned(10.0, 10.0, 100.0, 50.0)
        },
    );
    t.world
        .insert(
            parent,
            UiStyle {
                opacity: Some(0.5),
                ..boxed(RED)
            },
        )
        .unwrap();
    let child = t.child(parent, pinned(50.0, 20.0, 200.0, 200.0));
    t.world
        .insert(
            child,
            UiStyle {
                opacity: Some(0.5),
                ..boxed(BLUE)
            },
        )
        .unwrap();
    let list = draw(&t, &mut MemoryAssets::default(), UiFrame::new(400, 300));
    let p = quad_of(&list, &UiPaint::Solid(RED));
    let c = quad_of(&list, &UiPaint::Solid(BLUE));
    assert_eq!(p.opacity, 0.5);
    assert!(p.clip[0] < -1.0e6 && p.clip[2] > 1.0e6);
    assert_eq!(c.opacity, 0.25);
    rect4(c.clip, [10.0, 10.0, 110.0, 60.0]);
    rect4(c.rect, [60.0, 30.0, 200.0, 200.0]);
}

/// Two images of one 50 × 40 asset share a texture; one samples the region (10, 8, 20 × 16), so
/// u 10/50..30/50 = 0.2..0.6, v 8/40..24/40 = 0.2..0.6, each pulled in half a texel (0.5/50 =
/// 0.01 and 0.5/40 = 0.0125) so filtering stays inside: u 0.21..0.59, v 0.2125..0.5875.
#[test]
fn images_share_textures_and_sample_their_region() {
    let image: AssetRef = "builtin:texture/test_50x40".parse().unwrap();
    let mut assets = MemoryAssets::default();
    assets.insert_image(&image, ImageData::solid(50, 40, [255, 0, 0, 255]));
    let mut t = Tree::new();
    let root = t.root(
        UiRoot {
            canvas: [400.0, 300.0],
            ..UiRoot::default()
        },
        UiNode::default(),
    );
    let a = t.child(root, pinned(0.0, 0.0, 50.0, 40.0));
    let b = t.child(root, pinned(100.0, 0.0, 20.0, 16.0));
    let pic = |region| UiImage {
        image: image.clone(),
        region,
        mode: ImageMode::Scale,
        tint: [1.0; 4],
        flip_h: false,
        flip_v: false,
    };
    t.world.insert(a, pic(None)).unwrap();
    t.world.insert(b, pic(Some([10, 8, 20, 16]))).unwrap();
    let list = draw(&t, &mut assets, UiFrame::new(400, 300));
    assert_eq!(list.textures.len(), 1);
    assert_eq!(list.textures[0].format, UiTextureFormat::Rgba8Srgb);
    assert_eq!((list.textures[0].width, list.textures[0].height), (50, 40));
    let uvs: Vec<[f32; 4]> = list
        .quads
        .iter()
        .map(|q| match q.paint {
            UiPaint::Texture { texture: 0, uv, .. } => uv,
            ref other => panic!("{other:?}"),
        })
        .collect();
    rect4(uvs[0], [0.0, 0.0, 1.0, 1.0]);
    rect4(uvs[1], [0.21, 0.2125, 0.59, 0.5875]);
    // A missing image draws nothing.
    let missing = draw(&t, &mut MemoryAssets::default(), UiFrame::new(400, 300));
    assert!(missing.is_empty());
}

/// "A B" draws two glyph quads (the space has no ink) on one `R8` atlas page, in the style's
/// color; each quad's size is its atlas slot's and its UV covers that slot.
#[test]
fn text_draws_one_glyph_quad_per_inked_glyph() {
    let mut t = Tree::new();
    let root = t.root(
        UiRoot {
            canvas: [400.0, 300.0],
            ..UiRoot::default()
        },
        UiNode::default(),
    );
    let label = t.child(root, pinned(10.0, 10.0, 200.0, 40.0));
    t.world.insert(label, UiText::plain("A B")).unwrap();
    t.world
        .insert(
            label,
            UiStyle {
                color: Some(RED),
                font_size: Some(24.0),
                ..UiStyle::default()
            },
        )
        .unwrap();
    let list = draw(&t, &mut MemoryAssets::default(), UiFrame::new(400, 300));
    let glyphs: Vec<&UiQuad> = list
        .quads
        .iter()
        .filter(|q| matches!(q.paint, UiPaint::Glyph { .. }))
        .collect();
    assert_eq!(glyphs.len(), 2);
    for q in &glyphs {
        let UiPaint::Glyph { texture, uv, color } = q.paint else {
            unreachable!()
        };
        assert_eq!(color, RED);
        let page = &list.textures[texture as usize];
        assert_eq!(page.format, UiTextureFormat::R8);
        assert!(close((uv[2] - uv[0]) * page.width as f32, q.rect[2]));
        assert!(close((uv[3] - uv[1]) * page.height as f32, q.rect[3]));
        assert!(
            q.rect[0] >= 10.0 - 2.0 && q.rect[1] >= 10.0 - 2.0,
            "{:?} inside the label",
            q.rect
        );
    }
    assert!(glyphs[1].rect[0] > glyphs[0].rect[0]);
}

/// 25 % of a 200 × 20 bar: the track over the whole bar, then the fill over (0, 0, 50 × 20).
/// Textured: under, then the progress texture cut to u 0..0.25, then over. Clockwise: the whole
/// rect, masked about (100, 10).
#[test]
fn progress_draws_track_then_fill() {
    let mut t = Tree::new();
    let root = t.root(
        UiRoot {
            canvas: [400.0, 300.0],
            ..UiRoot::default()
        },
        UiNode::default(),
    );
    let bar = t.child(root, pinned(0.0, 0.0, 200.0, 20.0));
    let progress = UiProgress {
        value: 25.0,
        track: Some(Background::Solid(BLUE)),
        bar: Some(Background::Solid(RED)),
        ..UiProgress::default()
    };
    t.world.insert(bar, progress.clone()).unwrap();
    let list = draw(&t, &mut MemoryAssets::default(), UiFrame::new(400, 300));
    assert_eq!(list.quads.len(), 2);
    assert_eq!(list.quads[0].paint, UiPaint::Solid(BLUE));
    rect4(list.quads[0].rect, [0.0, 0.0, 200.0, 20.0]);
    assert_eq!(list.quads[1].paint, UiPaint::Solid(RED));
    rect4(list.quads[1].rect, [0.0, 0.0, 50.0, 20.0]);

    let tex = |n: &str| -> AssetRef { format!("builtin:texture/{n}").parse().unwrap() };
    let mut assets = MemoryAssets::default();
    for n in ["under", "fill", "over"] {
        assets.insert_image(&tex(n), ImageData::solid(8, 8, [255; 4]));
    }
    let textured = UiProgress {
        under: Some(tex("under")),
        progress: Some(tex("fill")),
        over: Some(tex("over")),
        ..progress.clone()
    };
    t.world.insert(bar, textured).unwrap();
    let list = draw(&t, &mut assets, UiFrame::new(400, 300));
    let uv = |q: &UiQuad| match q.paint {
        UiPaint::Texture { uv, .. } => uv,
        _ => panic!(),
    };
    assert_eq!(list.quads.len(), 3);
    rect4(list.quads[1].rect, [0.0, 0.0, 50.0, 20.0]);
    rect4(uv(&list.quads[1]), [0.0, 0.0, 0.25, 1.0]);
    rect4(list.quads[2].rect, [0.0, 0.0, 200.0, 20.0]);

    t.world
        .insert(
            bar,
            UiProgress {
                fill: FillMode::Clockwise,
                ..progress
            },
        )
        .unwrap();
    let list = draw(&t, &mut MemoryAssets::default(), UiFrame::new(400, 300));
    rect4(list.quads[1].rect, [0.0, 0.0, 200.0, 20.0]);
    assert!(matches!(
        list.quads[1].mask,
        UiMask::Angular {
            center: [100.0, 10.0],
            ..
        }
    ));
}

/// The `ui_bounds` layer outlines every node, root included, after everything else: borderline
/// quads with no paint and a 1 px border.
#[test]
fn ui_bounds_layer_outlines_nodes() {
    let mut t = Tree::new();
    let root = t.root(
        UiRoot {
            canvas: [400.0, 300.0],
            ..UiRoot::default()
        },
        UiNode::default(),
    );
    let node = t.child(root, pinned(10.0, 10.0, 100.0, 50.0));
    t.world.insert(node, boxed(RED)).unwrap();
    let frame = UiFrame {
        bounds: true,
        ..UiFrame::new(400, 300)
    };
    let list = draw(&t, &mut MemoryAssets::default(), frame);
    assert_eq!(list.quads.len(), 3);
    let outlines: Vec<&UiQuad> = list.quads[1..].iter().collect();
    assert!(
        outlines
            .iter()
            .all(|q| q.paint == UiPaint::None && q.border == 1.0)
    );
    rect4(outlines[0].rect, [0.0, 0.0, 400.0, 300.0]);
    rect4(outlines[1].rect, [10.0, 10.0, 100.0, 50.0]);
}

/// A HUD fixture's draw list, summarized (kind, rect, clip, opacity), as an insta snapshot.
#[test]
fn draw_list_snapshot_for_the_hud_fixture() {
    let (t, mut assets) = common::hud_fixture(0.6);
    let list = draw(&t, &mut assets, UiFrame::new(640, 360));
    insta::assert_snapshot!(common::summary(&list));
}

/// Half a texel of a 30 × 30 texture is 1/60: the corner slice [0, ⅓] samples [1/60, ⅓ − 1/60].
/// A mirrored rect [1, 0] of a 2-texel axis becomes [0.75, 0.25]; one under a texel wide is kept.
#[test]
fn half_texel_inset_keeps_sampling_inside_the_slice() {
    let third = 1.0 / 3.0;
    let h = 1.0 / 60.0;
    rect4(
        half_texel_inset([0.0, 0.0, third, third], [30.0, 30.0]),
        [h, h, third - h, third - h],
    );
    rect4(
        half_texel_inset([1.0, 0.0, 0.0, 1.0], [2.0, 2.0]),
        [0.75, 0.25, 0.25, 0.75],
    );
    rect4(
        half_texel_inset([0.5, 0.5, 0.51, 0.51], [10.0, 10.0]),
        [0.5, 0.5, 0.51, 0.51],
    );
}
