//! Virtual shadow maps on the GPU (M4 4.7a, ADR-0091): the camera depth prepass, page marking and
//! allocation, page rendering on both draw paths, sampling, and the CSM fallback.

mod common;
use common::{M1, Scene, meshlet_renderers, with_meshlets};
use concordia_core::math::{Quat, Vec3};
use concordia_render::vsm::ShadowTechnique;
use concordia_render::{Camera, DirectionalLight, Image, Msaa, Renderer, ViewMode};

const SIZE: (u32, u32) = (320, 180);

fn lit(base: [f32; 3]) -> M1 {
    M1 {
        base_color: [base[0], base[1], base[2], 1.0],
        metallic: 0.0,
        roughness: 0.6,
        ..M1::default()
    }
}

/// A ground (M1 path), a row of meshlet balls with a box among them (M1), a shadowed sun, and a
/// camera looking down the row with `msaa`; with `casts` false nothing casts a shadow.
fn yard(msaa: Msaa, casts: bool) -> Scene {
    let mut scene = Scene::new();
    let ball = scene.assets.insert_mesh(
        "ball",
        with_meshlets(concordia_assets::shapes::sphere(1.0, 64, 32)),
    );
    let stone = scene.material("stone", lit([0.6, 0.55, 0.5]));
    let grass = scene.material("grass", lit([0.3, 0.5, 0.25]));
    let ground = scene.mesh(
        "plane",
        grass,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(40.0, 1.0, 40.0),
    );
    for i in 0..6 {
        let id = scene.entity(
            Vec3::new(-5.0 + 2.0 * i as f32, 1.0, -4.0 - 3.0 * i as f32),
            Quat::IDENTITY,
            Vec3::ONE,
        );
        scene
            .world
            .insert(
                id,
                concordia_assets::MeshRenderer {
                    mesh: ball.clone(),
                    material: stone.clone(),
                    cast_shadows: casts,
                },
            )
            .unwrap();
    }
    let cube = scene.mesh(
        "cube",
        stone,
        Vec3::new(1.0, 0.5, -2.0),
        Quat::IDENTITY,
        Vec3::ONE,
    );
    for id in [ground, cube] {
        let mut r = scene
            .world
            .get::<concordia_assets::MeshRenderer>(id)
            .unwrap()
            .clone();
        r.cast_shadows = casts;
        scene.world.insert(id, r).unwrap();
    }
    let sun = scene.sun(Vec3::new(-0.4, -1.0, -0.3), 50_000.0);
    let mut light = scene.world.get::<DirectionalLight>(sun).unwrap().clone();
    light.shadows = Some(Default::default());
    scene.world.insert(sun, light).unwrap();
    scene.camera(
        Vec3::new(0.0, 3.0, 6.0),
        Vec3::new(0.0, 0.5, -10.0),
        Camera {
            exposure_ev100: 14.0,
            msaa,
            ..Camera::default()
        },
    );
    scene
}

fn render(r: &mut Renderer, scene: &mut Scene, shadows: ShadowTechnique, mode: ViewMode) -> Image {
    r.set_shadows(shadows);
    // Twice: the first frame has no HZB history, the second runs both culling passes.
    scene.render_mode(r, SIZE.0, SIZE.1, mode);
    scene.render_mode(r, SIZE.0, SIZE.1, mode)
}

/// Under `vsm` the frame draws the camera's depth first (`forward.prepass`, and
/// `forward.prepass_late` for pass 2's survivors when the HZB splits the frame), builds the HZB from
/// it and shades in one `forward` pass; under `csm` the frame is M1's and 4.4b's.
#[test]
fn vsm_frames_run_a_depth_prepass_before_forward() {
    for mut r in meshlet_renderers() {
        r.set_perf_mode(true);
        let mut scene = yard(Msaa::Off, false);
        render(&mut r, &mut scene, ShadowTechnique::vsm, ViewMode::lit);
        let passes = r.stats().timed_passes;
        let at = |name: &str| passes.iter().position(|p| p == name);
        let (prepass, late, hzb, forward) = (
            at("forward.prepass").expect("a prepass"),
            at("forward.prepass_late").expect("pass 2's prepass"),
            at("meshlet.hzb").expect("an HZB"),
            at("forward").expect("forward"),
        );
        assert!(prepass < hzb && hzb < late && late < forward, "{passes:?}");
        assert_eq!(at("forward_late"), None, "one shading pass: {passes:?}");

        render(&mut r, &mut scene, ShadowTechnique::csm, ViewMode::lit);
        let passes = r.stats().timed_passes;
        assert!(
            !passes.iter().any(|p| p.starts_with("forward.prepass")),
            "{passes:?}"
        );
        assert!(passes.iter().any(|p| p == "forward_late"), "{passes:?}");
    }
}

/// With nothing casting shadows (the sun asks for them), `vsm` and `csm` frames are the same picture on every draw path, with
/// and without MSAA: the prepass and the single shading pass draw exactly what the split frame did.
#[test]
fn prepass_leaves_unshadowed_images_unchanged() {
    for mut r in meshlet_renderers() {
        for msaa in [Msaa::Off, Msaa::X4] {
            let mut scene = yard(msaa, false);
            let csm = render(&mut r, &mut scene, ShadowTechnique::csm, ViewMode::lit);
            let vsm = render(&mut r, &mut scene, ShadowTechnique::vsm, ViewMode::lit);
            let flip = common::flip(&csm, &vsm).mean;
            assert!(
                flip <= 0.001,
                "{msaa:?} on {:?}: FLIP {flip}",
                r.stats().raster_path
            );
        }
    }
}

/// The HZB under `vsm` is built from the prepass's depth and matches the one `csm` builds from pass
/// 1's (the `hzb` view, single-sample).
#[test]
fn vsm_hzb_comes_from_the_prepass() {
    for mut r in meshlet_renderers() {
        let mut scene = yard(Msaa::Off, false);
        let csm = render(&mut r, &mut scene, ShadowTechnique::csm, ViewMode::hzb);
        let vsm = render(&mut r, &mut scene, ShadowTechnique::vsm, ViewMode::hzb);
        let flip = common::flip(&csm, &vsm).mean;
        assert!(flip <= 0.001, "{:?}: FLIP {flip}", r.stats().raster_path);
        assert!(
            common::pixel(&vsm, SIZE.0 / 2, SIZE.1 - 10)[0] > 0,
            "the HZB isn't empty"
        );
    }
}

/// A `vsm` frame marks pages from the prepass's depth, allocates them and clears them before it
/// shades (`vsm.mark`, `vsm.allocate`, `vsm.clear` between the prepass and `forward`).
#[test]
fn vsm_frames_mark_allocate_and_clear_pages() {
    for mut r in meshlet_renderers() {
        r.set_perf_mode(true);
        let mut scene = yard(Msaa::Off, true);
        render(&mut r, &mut scene, ShadowTechnique::vsm, ViewMode::lit);
        let passes = r.stats().timed_passes;
        let at = |name: &str| passes.iter().position(|p| p == name);
        let order: Vec<usize> = [
            "forward.prepass_late",
            "vsm.mark",
            "vsm.allocate",
            "vsm.clear",
            "forward",
        ]
        .iter()
        .map(|n| at(n).unwrap_or_else(|| panic!("{n} missing: {passes:?}")))
        .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{passes:?}");
    }
}

/// The fraction of `image`'s pixels in its lower half that are darker than `threshold` (luma).
fn dark_fraction(image: &Image, threshold: f32) -> f32 {
    let mut dark = 0;
    let mut total = 0;
    for y in image.height / 2..image.height {
        for x in 0..image.width {
            let [r, g, b, _] = common::pixel(image, x, y);
            let luma = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
            dark += u32::from(luma < threshold);
            total += 1;
        }
    }
    dark as f32 / total as f32
}

/// Virtual shadow maps shadow the ground where cascades do: the balls' and the box's shadows cover
/// about the same share of the ground (within 15 % of each other), and the pictures are close.
#[test]
fn vsm_shadows_land_where_csm_shadows_do() {
    for mut r in meshlet_renderers() {
        let mut scene = yard(Msaa::Off, true);
        let csm = render(&mut r, &mut scene, ShadowTechnique::csm, ViewMode::lit);
        let vsm = render(&mut r, &mut scene, ShadowTechnique::vsm, ViewMode::lit);
        let (c, v) = (dark_fraction(&csm, 60.0), dark_fraction(&vsm, 60.0));
        assert!(c > 0.02, "the scene has shadows: {c}");
        assert!(
            (v - c).abs() <= 0.15 * c,
            "{:?}: csm {c}, vsm {v}",
            r.stats().raster_path
        );
        let flip = common::flip(&csm, &vsm).mean;
        assert!(flip <= 0.05, "{:?}: FLIP {flip}", r.stats().raster_path);
    }
}

/// Both meshlet draw paths write the same pages: mesh shaders against compute + indirect.
#[test]
fn vsm_matches_on_both_routes() {
    let renderers = meshlet_renderers();
    if renderers.len() < 2 {
        return;
    }
    let images: Vec<Image> = renderers
        .into_iter()
        .map(|mut r| {
            render(
                &mut r,
                &mut yard(Msaa::Off, true),
                ShadowTechnique::vsm,
                ViewMode::lit,
            )
        })
        .collect();
    let flip = common::flip(&images[0], &images[1]).mean;
    assert!(flip <= 0.001, "FLIP {flip}");
}

/// Asked for `vsm` on a device without fragment atomics or attachment-less passes (masked), the
/// renderer draws CSM: the same picture as asking for `csm`.
#[test]
fn vsm_falls_back_to_csm_without_caps() {
    for mask in [
        concordia_rhi::CapsMask {
            fragment_atomics: true,
            ..Default::default()
        },
        concordia_rhi::CapsMask {
            attachmentless_passes: true,
            ..Default::default()
        },
    ] {
        let Some(mut r) = common::renderer_masked(&mask) else {
            return;
        };
        let mut scene = yard(Msaa::Off, true);
        let csm = render(&mut r, &mut scene, ShadowTechnique::csm, ViewMode::lit);
        let fallback = render(&mut r, &mut scene, ShadowTechnique::vsm, ViewMode::lit);
        assert_eq!(csm.rgba, fallback.rgba, "{mask:?}");
    }
}

/// `render.stats.shadows` says which technique ran and counts the pages: every requested page gets
/// one with the default pool, and meshlets were drawn into the levels.
#[test]
fn vsm_stats_report_the_technique_and_pages() {
    for mut r in meshlet_renderers() {
        let mut scene = yard(Msaa::Off, true);
        render(&mut r, &mut scene, ShadowTechnique::vsm, ViewMode::lit);
        let s = r.stats().shadows;
        assert_eq!(
            (s.requested, s.technique),
            (ShadowTechnique::vsm, ShadowTechnique::vsm)
        );
        assert_eq!(
            (s.pool_pages, s.levels, s.reason.as_deref()),
            (4096, 16, None)
        );
        assert!(s.pages_requested > 0, "{s:?}");
        assert_eq!(s.pages_allocated, s.pages_requested, "{s:?}");
        assert_eq!(s.pages_starved, 0);
        assert!(s.shadow_meshlets > 0, "{s:?}");
        render(&mut r, &mut scene, ShadowTechnique::csm, ViewMode::lit);
        let s = r.stats().shadows;
        assert_eq!(
            (s.technique, s.pool_pages, s.pages_requested),
            (ShadowTechnique::csm, 0, 0)
        );
    }
}

/// Where `vsm` can't run, the stats say so and why.
#[test]
fn vsm_reports_why_it_fell_back() {
    let mask = concordia_rhi::CapsMask {
        fragment_atomics: true,
        ..Default::default()
    };
    let Some(mut r) = common::renderer_masked(&mask) else {
        return;
    };
    render(
        &mut r,
        &mut yard(Msaa::Off, true),
        ShadowTechnique::vsm,
        ViewMode::lit,
    );
    let s = r.stats().shadows;
    assert_eq!(
        (s.requested, s.technique),
        (ShadowTechnique::vsm, ShadowTechnique::csm)
    );
    assert!(
        s.reason.as_deref().is_some_and(|r| r.contains("atomics")),
        "{s:?}"
    );
}

/// Meshlets over no page due to be drawn are culled: twenty balls far behind the camera (inside
/// the coarse levels, under no visible pixel's page) add no shadow meshlets.
#[test]
fn page_rect_culling_skips_meshlets_over_clean_pages() {
    for mut r in meshlet_renderers() {
        let mut near = yard(Msaa::Off, true);
        render(&mut r, &mut near, ShadowTechnique::vsm, ViewMode::lit);
        let without = r.stats().shadows.shadow_meshlets;
        let mut far = yard(Msaa::Off, true);
        let ball = far.assets.insert_mesh(
            "far ball",
            with_meshlets(concordia_assets::shapes::sphere(1.0, 64, 32)),
        );
        let stone = far.material("far stone", lit([0.5, 0.5, 0.5]));
        for i in 0..20 {
            let id = far.entity(
                Vec3::new(-20.0 + 2.0 * i as f32, 1.0, 300.0),
                Quat::IDENTITY,
                Vec3::ONE,
            );
            far.world
                .insert(
                    id,
                    concordia_assets::MeshRenderer {
                        mesh: ball.clone(),
                        material: stone.clone(),
                        cast_shadows: true,
                    },
                )
                .unwrap();
        }
        render(&mut r, &mut far, ShadowTechnique::vsm, ViewMode::lit);
        let with = r.stats().shadows.shadow_meshlets;
        assert!(without > 0);
        assert_eq!(with, without, "{:?}", r.stats().raster_path);
    }
}

/// The debug views: `shadow_mask` is the sun's visibility (grey, both lit and shadowed pixels);
/// `vsm_pages` and `vsm_levels` tint the ground with several colours; `vsm_cache` shows every page
/// drawn this frame (yellow) until caching arrives (4.7b).
#[test]
fn vsm_debug_views_show_pages_levels_cache_and_the_mask() {
    for mut r in meshlet_renderers() {
        let mut scene = yard(Msaa::Off, true);
        let mask = render(
            &mut r,
            &mut scene,
            ShadowTechnique::vsm,
            ViewMode::shadow_mask,
        );
        let ground: Vec<[u8; 4]> = (SIZE.1 / 2..SIZE.1)
            .flat_map(|y| (0..SIZE.0).map(move |x| (x, y)))
            .map(|(x, y)| common::pixel(&mask, x, y))
            .collect();
        assert!(ground.iter().all(|[r, g, b, _]| r == g && g == b), "grey");
        assert!(ground.iter().any(|p| p[0] < 20) && ground.iter().any(|p| p[0] > 235));
        let distinct = |image: &Image| {
            (SIZE.1 / 2..SIZE.1)
                .step_by(4)
                .flat_map(|y| (0..SIZE.0).step_by(4).map(move |x| (x, y)))
                .map(|(x, y)| common::pixel(image, x, y))
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        };
        let lit = render(&mut r, &mut scene, ShadowTechnique::vsm, ViewMode::lit);
        for mode in [ViewMode::vsm_pages, ViewMode::vsm_levels] {
            let image = render(&mut r, &mut scene, ShadowTechnique::vsm, mode);
            assert_ne!(image.rgba, lit.rgba, "{mode:?}");
            assert!(distinct(&image) > distinct(&lit), "{mode:?}");
        }
        let cache = render(
            &mut r,
            &mut scene,
            ShadowTechnique::vsm,
            ViewMode::vsm_cache,
        );
        let [cr, cg, cb, _] = common::pixel(&cache, SIZE.0 / 2, SIZE.1 - 10);
        assert!(cr > cb && cg > cb, "yellowish: {cr} {cg} {cb}");
    }
}

/// Regression goldens (`tests/golden/m4_vsm/`, tight): the yard lit with virtual shadow maps, its
/// sun visibility, and the page, level and cache views, at 640 × 360 on the device's own path.
#[test]
fn vsm_regression_goldens() {
    let Some(mut r) = meshlet_renderers().into_iter().next() else {
        return;
    };
    let mut scene = yard(Msaa::Off, true);
    r.set_shadows(ShadowTechnique::vsm);
    let mut shot = |mode| {
        scene.render_mode(&mut r, 640, 360, mode);
        scene.render_mode(&mut r, 640, 360, mode)
    };
    let images = [
        ("yard_lit", shot(ViewMode::lit)),
        ("yard_shadow_mask", shot(ViewMode::shadow_mask)),
        ("yard_vsm_pages", shot(ViewMode::vsm_pages)),
        ("yard_vsm_levels", shot(ViewMode::vsm_levels)),
        ("yard_vsm_cache", shot(ViewMode::vsm_cache)),
    ];
    let named: Vec<(&str, &Image)> = images.iter().map(|(n, i)| (*n, i)).collect();
    common::assert_goldens_in("m4_vsm", &named);
}

/// Review footage (not a check; runs only with CONCORDIA_REVIEW_FRAMES=<dir>): raw RGBA frames at
/// 640 × 360 of the camera dollying through the yard (lit, then the page view) and of a ball rolling
/// across the ground (lit, then the sun mask), for the review page's videos.
#[test]
fn vsm_review_frames() {
    let Ok(dir) = std::env::var("CONCORDIA_REVIEW_FRAMES") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let Some(mut r) = meshlet_renderers().into_iter().next() else {
        return;
    };
    r.set_shadows(ShadowTechnique::vsm);
    let frames = 48;
    let mut scene = yard(Msaa::Off, true);
    let camera = scene.camera(
        Vec3::new(0.0, 3.0, 6.0),
        Vec3::new(0.0, 0.5, -10.0),
        Camera {
            exposure_ev100: 14.0,
            msaa: Msaa::Off,
            priority: 1,
            ..Camera::default()
        },
    );
    for mode in [ViewMode::lit, ViewMode::vsm_pages] {
        let mut raw = Vec::new();
        for i in 0..frames {
            let t = i as f32 / frames as f32;
            let eye = Vec3::new(-3.0 + 6.0 * t, 3.0, 6.0 - 4.0 * t);
            scene.aim(camera, eye, eye + Vec3::new(0.0, -2.5, -16.0));
            raw.extend(scene.render_mode(&mut r, 640, 360, mode).rgba);
        }
        std::fs::write(dir.join(format!("dolly_{mode:?}.rgba")), raw).unwrap();
    }
    let mut scene = yard(Msaa::Off, true);
    let ball = scene.entity(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(0.6));
    let red = scene.material("ball", lit([0.8, 0.3, 0.2]));
    scene
        .world
        .insert(
            ball,
            concordia_assets::MeshRenderer {
                mesh: "builtin:mesh/sphere".parse().unwrap(),
                material: red,
                cast_shadows: true,
            },
        )
        .unwrap();
    for mode in [ViewMode::lit, ViewMode::shadow_mask] {
        let mut raw = Vec::new();
        for i in 0..frames {
            let t = i as f32 / frames as f32;
            let at = Vec3::new(
                -4.0 + 8.0 * t,
                0.3 + 0.8 * (t * std::f32::consts::PI).sin(),
                -1.0,
            );
            scene.aim(ball, at, at + Vec3::Z);
            let mut transform = *scene
                .world
                .get::<concordia_scene::builtin::Transform>(ball)
                .unwrap();
            transform.scale = Vec3::splat(0.6);
            scene.world.insert(ball, transform).unwrap();
            raw.extend(scene.render_mode(&mut r, 640, 360, mode).rgba);
        }
        std::fs::write(dir.join(format!("ball_{mode:?}.rgba")), raw).unwrap();
    }
}
