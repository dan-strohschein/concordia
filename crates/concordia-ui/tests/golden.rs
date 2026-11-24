//! Goldens per UI feature, drawn through the renderer's UI pass on the GPU (macOS; skipped where
//! there is no device unless `CONCORDIA_REQUIRE_GPU=1`). A missing golden saves its candidate to
//! `target/golden-candidates/ui/<name>.png` for review before blessing.

mod common;

use std::sync::Arc;

use common::{Tree, checker, frame_image, hud_fixture};
use concordia_api::UiFrame;
use concordia_assets::AssetRef;
use concordia_core::id::EntityId;
use concordia_render::{Camera, RenderRequest, RenderWorld, Renderer, Tonemap};
use concordia_rhi::RhiError;
use concordia_scene::builtin::Transform;
use concordia_shaders::{SlangToolchain, SlangcCompiler};
use concordia_testkit::golden::{Golden, GoldenError, RgbaImage};
use concordia_testkit::gpu_required;
use concordia_ui::*;

fn renderer() -> Option<Renderer> {
    let device = match concordia_rhi_metal::new_headless() {
        Ok(device) => Arc::<dyn concordia_rhi::Device>::from(device),
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            return None;
        }
        Err(other) => panic!("unexpected device error: {other}"),
    };
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    Some(Renderer::new(device, Box::new(compiler)).expect("renderer"))
}

/// Draws `t`'s UI over a dark, untonemapped background and compares it with `ui/<name>`.
fn golden(name: &str, t: &mut Tree, assets: &mut MemoryAssets, size: [u32; 2]) {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let camera = t.world.spawn(&mut t.ids).unwrap();
    t.world.insert(camera, Transform::default()).unwrap();
    let cam = Camera {
        clear: [0.02, 0.02, 0.03, 1.0],
        tonemap: Tonemap::Linear,
        ..Camera::default()
    };
    t.world.insert(camera, cam).unwrap();
    let frame = UiFrame {
        clip_from_world: concordia_render::scene_camera(&t.world, None)
            .ok()
            .and_then(|c| {
                concordia_render::clip_from_world(&c, size[0] as f32 / size[1] as f32).ok()
            }),
        ..UiFrame::new(size[0], size[1])
    };
    let ui = UiContext::default().draw(&t.world, assets, &frame);
    let request = RenderRequest {
        camera: None,
        view: None,
        width: size[0],
        height: size[1],
        overlays: None,
        view_mode: concordia_render::ViewMode::lit,
        debug: Vec::new(),
        ui,
    };
    let world = RenderWorld::extract(&t.world);
    let image = renderer
        .render(
            &world,
            &mut concordia_render::MemoryAssets::default(),
            &request,
        )
        .expect("render");
    let rgba = RgbaImage::new(image.width, image.height, image.rgba).unwrap();
    match Golden::from_env().check("ui", name, &rgba) {
        Ok(_) => {}
        Err(GoldenError::Missing { path }) => {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/golden-candidates");
            Golden::new(dir.clone(), dir.clone(), true, false)
                .check("ui", name, &rgba)
                .unwrap();
            panic!(
                "golden ui/{name} is missing ({}); its candidate is in {}/ui",
                path.display(),
                dir.display()
            );
        }
        Err(e) => panic!("golden ui/{name}: {e}"),
    }
}

fn root(t: &mut Tree, w: f32, h: f32) -> EntityId {
    t.root(
        UiRoot {
            canvas: [w, h],
            ..UiRoot::default()
        },
        UiNode::default(),
    )
}

fn at(x: f32, y: f32, w: f32, h: f32) -> UiNode {
    UiNode {
        position: Position::Anchored(Anchored {
            offsets: Edges::new(x, y, x + w, y + h),
            ..Anchored::default()
        }),
        ..UiNode::default()
    }
}

fn stops(colors: &[[f32; 4]]) -> Vec<GradientStop> {
    let n = (colors.len() - 1) as f32;
    colors
        .iter()
        .enumerate()
        .map(|(i, &color)| GradientStop {
            offset: i as f32 / n,
            color,
        })
        .collect()
}

const RED: [f32; 4] = [0.9, 0.1, 0.1, 1.0];
const ORANGE: [f32; 4] = [1.0, 0.55, 0.05, 1.0];
const BLUE: [f32; 4] = [0.1, 0.3, 0.9, 1.0];
const GREEN: [f32; 4] = [0.1, 0.8, 0.3, 1.0];
const WHITE: [f32; 4] = [1.0; 4];

#[test]
fn ui_hud() {
    let (mut t, mut assets) = hud_fixture(0.6);
    golden("ui_hud", &mut t, &mut assets, [640, 360]);
}

/// The HUD with its bar and label driven to 20 %, 50 % and 90 %.
#[test]
fn ui_hud_sequence() {
    let failed: Vec<String> = [0.2, 0.5, 0.9]
        .into_iter()
        .enumerate()
        .filter_map(|(i, fill)| {
            let (mut t, mut assets) = hud_fixture(fill);
            let name = format!("ui_hud_sequence_{i}");
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                golden(&name, &mut t, &mut assets, [640, 360])
            }))
            .err()
            .map(|_| name)
        })
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
}

/// Solid, bordered and rounded boxes; linear gradients at 0°, 45° and 90°; a radial gradient; a
/// half-opaque box over another; per-corner radii.
#[test]
fn ui_gradients_borders_radii() {
    let mut t = Tree::new();
    let r = root(&mut t, 480.0, 240.0);
    let boxes: Vec<(UiNode, UiStyle)> = vec![
        (
            at(20.0, 20.0, 100.0, 60.0),
            UiStyle {
                background: Some(Background::Solid(RED)),
                ..UiStyle::default()
            },
        ),
        (
            at(140.0, 20.0, 100.0, 60.0),
            UiStyle {
                background: Some(Background::Solid(BLUE)),
                border: Some(Border {
                    width: 4.0,
                    color: WHITE,
                }),
                radius: Some([12.0; 4]),
                ..UiStyle::default()
            },
        ),
        (
            at(260.0, 20.0, 100.0, 60.0),
            UiStyle {
                background: Some(Background::Linear {
                    angle_deg: 0.0,
                    stops: stops(&[RED, ORANGE]),
                }),
                ..UiStyle::default()
            },
        ),
        (
            at(380.0, 20.0, 80.0, 60.0),
            UiStyle {
                background: Some(Background::Linear {
                    angle_deg: 90.0,
                    stops: stops(&[BLUE, GREEN, ORANGE]),
                }),
                ..UiStyle::default()
            },
        ),
        (
            at(20.0, 100.0, 100.0, 100.0),
            UiStyle {
                background: Some(Background::Radial {
                    center: [0.5, 0.5],
                    radius: [0.5, 0.5],
                    stops: stops(&[WHITE, BLUE]),
                }),
                radius: Some([50.0; 4]),
                ..UiStyle::default()
            },
        ),
        (
            at(140.0, 100.0, 120.0, 100.0),
            UiStyle {
                background: Some(Background::Linear {
                    angle_deg: 45.0,
                    stops: stops(&[GREEN, BLUE]),
                }),
                radius: Some([0.0, 24.0, 0.0, 24.0]),
                ..UiStyle::default()
            },
        ),
        (
            at(280.0, 100.0, 100.0, 80.0),
            UiStyle {
                background: Some(Background::Solid(GREEN)),
                ..UiStyle::default()
            },
        ),
        (
            at(320.0, 130.0, 100.0, 80.0),
            UiStyle {
                background: Some(Background::Solid(RED)),
                opacity: Some(0.5),
                border: Some(Border {
                    width: 2.0,
                    color: WHITE,
                }),
                radius: Some([8.0; 4]),
                ..UiStyle::default()
            },
        ),
    ];
    for (node, style) in boxes {
        let id = t.child(r, node);
        t.world.insert(id, style).unwrap();
    }
    golden(
        "ui_gradients_borders_radii",
        &mut t,
        &mut MemoryAssets::default(),
        [480, 240],
    );
}

/// One 24 × 24 frame stretched, tiled and tile-fitted at three sizes, and one without a center.
#[test]
fn ui_nine_patch() {
    let image: AssetRef = "builtin:texture/frame".parse().unwrap();
    let mut assets = MemoryAssets::default();
    assets.insert_image(&image, frame_image(24));
    let mut t = Tree::new();
    let r = root(&mut t, 480.0, 200.0);
    let patch = |axis: AxisStretch, draw_center: bool| UiNinePatch {
        image: image.clone(),
        region: None,
        margins: [8; 4],
        axis_h: axis,
        axis_v: axis,
        draw_center,
        tint: [1.0; 4],
    };
    for (node, p) in [
        (
            at(20.0, 20.0, 120.0, 60.0),
            patch(AxisStretch::Stretch, true),
        ),
        (at(160.0, 20.0, 133.0, 60.0), patch(AxisStretch::Tile, true)),
        (
            at(310.0, 20.0, 150.0, 77.0),
            patch(AxisStretch::TileFit, true),
        ),
        (
            at(20.0, 110.0, 200.0, 70.0),
            patch(AxisStretch::Stretch, false),
        ),
        (
            at(250.0, 110.0, 12.0, 70.0),
            patch(AxisStretch::Stretch, true),
        ),
    ] {
        let id = t.child(r, node);
        t.world.insert(id, p).unwrap();
    }
    golden("ui_nine_patch", &mut t, &mut assets, [480, 200]);
}

/// Styled runs (sizes, weights, italic, colors, underline, mono), the four alignments, and an
/// ellipsized line.
#[test]
fn ui_text_runs() {
    let mut t = Tree::new();
    let r = root(&mut t, 480.0, 260.0);
    let rich = t.child(r, at(16.0, 12.0, 448.0, 60.0));
    let run = |text: &str| TextRun {
        text: text.into(),
        ..TextRun::default()
    };
    t.world
        .insert(
            rich,
            UiText {
                runs: vec![
                    run("Deal "),
                    TextRun {
                        size: Some(28.0),
                        weight: Some(700.0),
                        color: Some(ORANGE),
                        ..run("12")
                    },
                    run(" fire damage, "),
                    TextRun {
                        italic: Some(true),
                        color: Some([0.6, 0.8, 1.0, 1.0]),
                        ..run("slowly")
                    },
                    run(", "),
                    TextRun {
                        underline: true,
                        ..run("now")
                    },
                    run(" in "),
                    TextRun {
                        font: Some(MONO.parse().unwrap()),
                        color: Some(GREEN),
                        ..run("mono")
                    },
                ],
                ..UiText::default()
            },
        )
        .unwrap();
    t.world
        .insert(
            rich,
            UiStyle {
                font_size: Some(18.0),
                ..UiStyle::default()
            },
        )
        .unwrap();
    for (i, align) in [
        TextAlign::Start,
        TextAlign::Center,
        TextAlign::End,
        TextAlign::Justify,
    ]
    .into_iter()
    .enumerate()
    {
        let id = t.child(
            r,
            at(
                16.0 + (i % 2) as f32 * 232.0,
                80.0 + (i / 2) as f32 * 70.0,
                216.0,
                60.0,
            ),
        );
        t.world
            .insert(
                id,
                UiText {
                    align,
                    ..UiText::plain("The quick brown fox jumps over the lazy dog.")
                },
            )
            .unwrap();
        t.world
            .insert(
                id,
                UiStyle {
                    font_size: Some(14.0),
                    background: Some(Background::Solid([0.1, 0.1, 0.15, 1.0])),
                    ..UiStyle::default()
                },
            )
            .unwrap();
    }
    let cut = t.child(r, at(16.0, 226.0, 200.0, 24.0));
    t.world
        .insert(
            cut,
            UiText {
                overflow: Overflow::Ellipsis,
                max_lines: Some(1),
                ..UiText::plain("An ellipsized line that is far too long to fit")
            },
        )
        .unwrap();
    golden(
        "ui_text_runs",
        &mut t,
        &mut MemoryAssets::default(),
        [480, 260],
    );
}

/// Every fill mode at 60 %, fill colors over a track.
#[test]
fn ui_progress_modes() {
    let mut t = Tree::new();
    let r = root(&mut t, 480.0, 220.0);
    let modes = [
        FillMode::LeftToRight,
        FillMode::RightToLeft,
        FillMode::TopToBottom,
        FillMode::BottomToTop,
        FillMode::BilinearH,
        FillMode::BilinearV,
        FillMode::Clockwise,
        FillMode::CounterClockwise,
        FillMode::ClockwiseAndCounter,
    ];
    for (i, fill) in modes.into_iter().enumerate() {
        let (x, y) = (16.0 + (i % 5) as f32 * 92.0, 16.0 + (i / 5) as f32 * 104.0);
        let id = t.child(r, at(x, y, 80.0, 80.0));
        let progress = UiProgress {
            value: 60.0,
            fill,
            radial: RadialFill {
                start_deg: 0.0,
                sweep_deg: 360.0,
                center: [0.5, 0.5],
            },
            track: Some(Background::Solid([0.12, 0.12, 0.15, 1.0])),
            bar: Some(Background::Linear {
                angle_deg: 90.0,
                stops: stops(&[RED, ORANGE]),
            }),
            ..UiProgress::default()
        };
        t.world.insert(id, progress).unwrap();
        let radius = if i >= 6 { 40.0 } else { 6.0 };
        t.world
            .insert(
                id,
                UiStyle {
                    radius: Some([radius; 4]),
                    ..UiStyle::default()
                },
            )
            .unwrap();
    }
    golden(
        "ui_progress_modes",
        &mut t,
        &mut MemoryAssets::default(),
        [480, 220],
    );
}

/// The seven stretch modes of a 32 × 24 checker in 100 × 70 boxes, plus a mirrored region.
#[test]
fn ui_image_modes() {
    let image: AssetRef = "builtin:texture/checker".parse().unwrap();
    let mut assets = MemoryAssets::default();
    assets.insert_image(
        &image,
        checker(32, 24, 8, [230, 230, 240, 255], [40, 120, 220, 255]),
    );
    let mut t = Tree::new();
    let r = root(&mut t, 480.0, 200.0);
    let modes = [
        ImageMode::Scale,
        ImageMode::Tile,
        ImageMode::Keep,
        ImageMode::KeepCentered,
        ImageMode::KeepAspect,
        ImageMode::KeepAspectCentered,
        ImageMode::KeepAspectCovered,
    ];
    for (i, mode) in modes.into_iter().enumerate() {
        let (x, y) = (12.0 + (i % 4) as f32 * 116.0, 12.0 + (i / 4) as f32 * 92.0);
        let id = t.child(
            r,
            UiNode {
                clip: true,
                ..at(x, y, 100.0, 70.0)
            },
        );
        t.world
            .insert(
                id,
                UiStyle {
                    background: Some(Background::Solid([0.15, 0.15, 0.2, 1.0])),
                    ..UiStyle::default()
                },
            )
            .unwrap();
        t.world
            .insert(
                id,
                UiImage {
                    image: image.clone(),
                    region: None,
                    mode,
                    tint: [1.0; 4],
                    flip_h: false,
                    flip_v: false,
                },
            )
            .unwrap();
    }
    let id = t.child(r, at(360.0, 104.0, 100.0, 70.0));
    t.world
        .insert(
            id,
            UiImage {
                image,
                region: Some([0, 0, 16, 12]),
                mode: ImageMode::Scale,
                tint: [1.0, 0.8, 0.8, 1.0],
                flip_h: true,
                flip_v: false,
            },
        )
        .unwrap();
    golden("ui_image_modes", &mut t, &mut assets, [480, 200]);
}

/// A clipping panel cuts its overflowing children and a nested clip cuts again; clipped text
/// stops at its box.
#[test]
fn ui_clip() {
    let mut t = Tree::new();
    let r = root(&mut t, 320.0, 200.0);
    let panel = t.child(
        r,
        UiNode {
            clip: true,
            ..at(40.0, 30.0, 200.0, 120.0)
        },
    );
    t.world
        .insert(
            panel,
            UiStyle {
                background: Some(Background::Solid([0.15, 0.15, 0.2, 1.0])),
                ..UiStyle::default()
            },
        )
        .unwrap();
    let big = t.child(
        panel,
        UiNode {
            clip: true,
            ..at(-30.0, 60.0, 150.0, 100.0)
        },
    );
    t.world
        .insert(
            big,
            UiStyle {
                background: Some(Background::Solid(BLUE)),
                radius: Some([20.0; 4]),
                ..UiStyle::default()
            },
        )
        .unwrap();
    let inner = t.child(big, at(100.0, -20.0, 120.0, 60.0));
    t.world
        .insert(
            inner,
            UiStyle {
                background: Some(Background::Solid(ORANGE)),
                ..UiStyle::default()
            },
        )
        .unwrap();
    let label = t.child(panel, at(120.0, 10.0, 70.0, 22.0));
    t.world
        .insert(
            label,
            UiText {
                overflow: Overflow::Clip,
                wrap: Wrap::None,
                ..UiText::plain("Clipped text runs on")
            },
        )
        .unwrap();
    golden("ui_clip", &mut t, &mut MemoryAssets::default(), [320, 200]);
}

/// `capture.screenshot` draws the in-game UI by default; `ui: false` leaves it out, the
/// `ui_bounds` layer adds outlines, and a `ui_scale` outside 0.25..=8 is refused.
/// One widget fixture (`common::controls`) on its own root, as a golden.
fn controls_golden(name: &str, size: [u32; 2], add: fn(&mut Tree, EntityId, [f32; 2])) {
    let mut t = Tree::new();
    let r = root(&mut t, size[0] as f32, size[1] as f32);
    add(&mut t, r, [10.0, 10.0]);
    golden(name, &mut t, &mut common::controls::assets(), size);
}

/// Normal, hovered, pressed, disabled and focused buttons (themed per state; the focused one
/// draws the focus ring).
#[test]
fn ui_button_states() {
    controls_golden(
        "ui_button_states",
        [400, 52],
        common::controls::button_states,
    );
}

/// Checkbox, switch and button-look toggles, off and on.
#[test]
fn ui_toggle_looks() {
    controls_golden(
        "ui_toggle_looks",
        [320, 120],
        common::controls::toggle_looks,
    );
}

/// Continuous, stepped and vertical sliders.
#[test]
fn ui_slider() {
    controls_golden("ui_slider", [260, 140], common::controls::sliders);
}

/// An open dropdown with its third option highlighted.
#[test]
fn ui_dropdown_open() {
    controls_golden(
        "ui_dropdown_open",
        [220, 180],
        common::controls::dropdown_open,
    );
}

/// Tabs with the second page showing.
#[test]
fn ui_tabs() {
    controls_golden("ui_tabs", [300, 160], common::controls::tabs);
}

/// A scrolled list clipped to its box, with its bar.
#[test]
fn ui_scroll() {
    controls_golden("ui_scroll", [220, 160], common::controls::scroll);
}

/// A tooltip shown beside its button.
#[test]
fn ui_tooltip() {
    controls_golden("ui_tooltip", [320, 100], common::controls::tooltip);
}

/// Text boxes: focused with a selection and caret, a placeholder, a password, multi-line.
#[test]
fn ui_text_boxes() {
    controls_golden("ui_text_boxes", [240, 216], common::controls::text_boxes);
}

/// A spin box with its arrows.
#[test]
fn ui_spin_box() {
    controls_golden("ui_spin_box", [140, 52], common::controls::spin_box);
}

/// A multi-select list with two items selected.
#[test]
fn ui_list() {
    controls_golden("ui_list", [200, 200], common::controls::list);
}

/// World-space UI (2.7b.3): a damage number 1 m above an enemy 6 m ahead, and a floating health
/// bar 0.8 m above another 4 m ahead, below and to the right, seen by the camera at the origin
/// looking down −z.
#[test]
fn ui_world_damage_number() {
    use concordia_core::math::Vec3;
    use concordia_scene::builtin::Parent;
    let mut t = Tree::new();
    let enemy = |t: &mut Tree, p: [f32; 3]| {
        let id = t.world.spawn(&mut t.ids).unwrap();
        t.world
            .insert(
                id,
                Transform {
                    translation: Vec3::from(p),
                    ..Transform::default()
                },
            )
            .unwrap();
        id
    };
    let world_root = |t: &mut Tree, parent: EntityId, canvas: [f32; 2], y: f32| {
        let id = t.world.spawn(&mut t.ids).unwrap();
        t.world.insert(id, Transform::default()).unwrap();
        t.world.insert(id, Parent(parent)).unwrap();
        t.world
            .insert(
                id,
                UiRoot {
                    canvas,
                    space: RootSpace::World {
                        offset: [0.0, y, 0.0],
                        pixel_size: 1.0,
                    },
                    ..UiRoot::default()
                },
            )
            .unwrap();
        t.world
            .insert(
                id,
                UiNode {
                    layout: Layout::Stack,
                    ..UiNode::default()
                },
            )
            .unwrap();
        id
    };
    let far = enemy(&mut t, [0.0, 0.0, -6.0]);
    let number = world_root(&mut t, far, [120.0, 40.0], 1.0);
    let label = t.child(number, UiNode::default());
    t.world
        .insert(
            label,
            UiText {
                runs: vec![TextRun {
                    text: "-42".into(),
                    size: Some(30.0),
                    weight: Some(700.0),
                    color: Some(ORANGE),
                    ..TextRun::default()
                }],
                align: TextAlign::Center,
                valign: VAlign::Center,
                ..UiText::default()
            },
        )
        .unwrap();
    let near = enemy(&mut t, [1.8, -1.5, -4.0]);
    let bar_root = world_root(&mut t, near, [80.0, 10.0], 0.8);
    let bar = t.child(bar_root, UiNode::default());
    t.world
        .insert(
            bar,
            UiProgress {
                value: 65.0,
                track: Some(Background::Solid([0.05, 0.05, 0.05, 0.9])),
                bar: Some(Background::Solid(RED)),
                ..UiProgress::default()
            },
        )
        .unwrap();
    golden(
        "ui_world_damage_number",
        &mut t,
        &mut MemoryAssets::default(),
        [320, 180],
    );
}

#[test]
fn capture_screenshot_draws_the_ui_unless_asked_not_to() {
    if renderer().is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut config = concordia_api::EngineConfig::new(dir.path());
    config.deterministic_ids = Some(9);
    config.modules = vec![
        Arc::new(UiModule::new()),
        Arc::new(concordia_render::RenderModule::new(Arc::new(
            concordia_rhi_metal::new_headless,
        ))),
    ];
    let mut engine = concordia_api::Engine::new(config).unwrap();
    let s = engine.open_session();
    let mut call = |method: &str, params: serde_json::Value| engine.call(s, method, params);
    let actor = "agent:test";
    call(
        "scene.new",
        serde_json::json!({"path": "scenes/a.scene.ron", "name": "A", "actor": actor}),
    )
    .unwrap();
    call("entity.spawn", serde_json::json!({"actor": actor, "components": {"Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]}, "Camera": {}}})).unwrap();
    let root = call("entity.spawn", serde_json::json!({"actor": actor, "components": {"UiRoot": {"canvas": [64.0, 64.0]}, "UiNode": {}}})).unwrap();
    call(
        "entity.spawn",
        serde_json::json!({"actor": actor, "components": {
            "Parent": root["id"],
            "UiNode": {"position": {"Anchored": {"offsets": {"left": 8.0, "top": 8.0, "right": 56.0, "bottom": 56.0}}}},
            "UiStyle": {"background": {"Solid": [1.0, 0.0, 0.0, 1.0]}},
        }}),
    )
    .unwrap();
    let mut shot = |extra: serde_json::Value| {
        let mut params = serde_json::json!({"width": 64, "height": 64});
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        call("capture.screenshot", params)
    };
    let with = shot(serde_json::json!({})).unwrap()["image"]["data_base64"].clone();
    let without = shot(serde_json::json!({"ui": false})).unwrap()["image"]["data_base64"].clone();
    let bounds =
        shot(serde_json::json!({"debug": ["ui_bounds"]})).unwrap()["image"]["data_base64"].clone();
    assert_ne!(with, without);
    assert_ne!(with, bounds);
    let err = shot(serde_json::json!({"ui_scale": 9.0})).unwrap_err();
    assert_eq!(err.data.kind, "InvalidParams", "{err:?}");
    assert_eq!(err.data.pointer.as_deref(), Some("/ui_scale"));
}

/// `capture.screenshot` places world-space UI with the capture's camera (2.7b.3): a world root
/// 5 m in front of the camera draws (the image differs from `"ui": false`); turned around, the
/// camera sees it behind and draws none (the image equals `"ui": false`).
#[test]
fn capture_screenshot_places_world_ui_with_its_camera() {
    if renderer().is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut config = concordia_api::EngineConfig::new(dir.path());
    config.deterministic_ids = Some(9);
    config.modules = vec![
        Arc::new(UiModule::new()),
        Arc::new(concordia_render::RenderModule::new(Arc::new(
            concordia_rhi_metal::new_headless,
        ))),
    ];
    let mut engine = concordia_api::Engine::new(config).unwrap();
    let s = engine.open_session();
    let mut call = |method: &str, params: serde_json::Value| engine.call(s, method, params);
    let actor = "agent:test";
    call(
        "scene.new",
        serde_json::json!({"path": "scenes/w.scene.ron", "name": "W", "actor": actor}),
    )
    .unwrap();
    let camera = call("entity.spawn", serde_json::json!({"actor": actor, "components": {"Transform": {"translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]}, "Camera": {}}})).unwrap()["id"].clone();
    let root = call("entity.spawn", serde_json::json!({"actor": actor, "components": {
        "Transform": {"translation": [0.0, 0.0, -5.0], "rotation": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0]},
        "UiRoot": {"canvas": [32.0, 32.0], "space": {"World": {"offset": [0.0, 0.0, 0.0], "pixel_size": 1.0}}},
        "UiNode": {"layout": "Stack"},
    }})).unwrap();
    call(
        "entity.spawn",
        serde_json::json!({"actor": actor, "components": {
            "Parent": root["id"],
            "UiNode": {},
            "UiStyle": {"background": {"Solid": [1.0, 0.0, 0.0, 1.0]}},
        }}),
    )
    .unwrap();
    let shot = |call: &mut dyn FnMut(
        &str,
        serde_json::Value,
    ) -> Result<serde_json::Value, concordia_api::ApiError>,
                ui: bool| {
        call(
            "capture.screenshot",
            serde_json::json!({"width": 64, "height": 64, "ui": ui}),
        )
        .unwrap()["image"]["data_base64"]
            .clone()
    };
    assert_ne!(
        shot(&mut call, true),
        shot(&mut call, false),
        "the world root is drawn"
    );
    call("component.patch", serde_json::json!({"actor": actor, "entity": camera, "component": "Transform", "patch": {"rotation": [0.0, 1.0, 0.0, 0.0]}})).unwrap();
    assert_eq!(
        shot(&mut call, true),
        shot(&mut call, false),
        "behind the camera, nothing is drawn"
    );
}
