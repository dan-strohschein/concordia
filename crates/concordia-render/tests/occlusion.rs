//! Two-pass HZB occlusion on the meshlet path (ADR-0100, M4 4.4b): pass 1 draws what was visible
//! last frame, the HZB is built from its depth, and pass 2 draws whatever else isn't hidden. It
//! never loses geometry (occlusion on against off, on both draw paths: camera cuts, a moving
//! occluder, the first frame), culls the hidden city, and devices without min depth resolves skip
//! it when they draw with MSAA. Such devices (lavapipe) run the occlusion tests single-sample, and
//! skip their goldens, which show 4× MSAA.

mod common;
use common::{M1, Scene, assert_culling_agrees, golden_in, meshlet_renderers, with_meshlets};
use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};
use concordia_render::{Camera, CullCode, Culling, Image, Msaa, Renderer, ViewMode};
use concordia_rhi::CapsMask;

const SUITE: &str = "m4_culling";
const SIZE: (u32, u32) = (320, 180);

/// Culling with occlusion off (the frustum and cone tests stay on).
const NO_OCCLUSION: Culling = Culling {
    frustum: true,
    cone: true,
    occlusion: false,
};

fn lit(base: [f32; 3]) -> M1 {
    M1 {
        base_color: [base[0], base[1], base[2], 1.0],
        metallic: 0.0,
        roughness: 0.5,
        ..M1::default()
    }
}

fn camera(msaa: Msaa) -> Camera {
    Camera {
        exposure_ev100: 0.0,
        msaa,
        ..Camera::default()
    }
}

/// A scene with its camera.
struct Shot {
    scene: Scene,
    camera: EntityId,
    /// The occluder that moves (the city's wall).
    wall: EntityId,
}

/// Street level in front of the city's wall, looking in.
const FRONT: (Vec3, Vec3) = (Vec3::new(0.0, 1.7, 4.0), Vec3::new(0.0, 1.7, -50.0));
/// The far side of the city, where nothing hides it, looking back.
const BACK: (Vec3, Vec3) = (Vec3::new(0.0, 6.0, -100.0), Vec3::new(0.0, 1.0, -40.0));

/// The occluded city: a 16 × 16 grid of dense meshlet blocks (balls of 128 meshlets) behind a long
/// opaque wall on the M1 path, a ground, and a sun; the camera at `view` with `msaa`.
fn city(view: (Vec3, Vec3), msaa: Msaa) -> Shot {
    let mut scene = Scene::new();
    let ball = scene.assets.insert_mesh(
        "block",
        with_meshlets(concordia_assets::shapes::sphere(1.0, 96, 48)),
    );
    let stone = scene.material("stone", lit([0.6, 0.55, 0.5]));
    let grey = scene.material("wall", lit([0.4, 0.4, 0.45]));
    for i in 0..16 {
        for j in 0..16 {
            let at = Vec3::new(-37.5 + 5.0 * i as f32, 1.5, -10.0 - 5.0 * j as f32);
            let id = scene.entity(at, Quat::IDENTITY, Vec3::splat(1.5));
            scene
                .world
                .insert(
                    id,
                    concordia_assets::MeshRenderer {
                        mesh: ball.clone(),
                        material: stone.clone(),
                        cast_shadows: false,
                        ..Default::default()
                    },
                )
                .unwrap();
        }
    }
    let wall = scene.mesh(
        "cube",
        grey.clone(),
        Vec3::new(0.0, 1.5, 0.0),
        Quat::IDENTITY,
        Vec3::new(200.0, 3.0, 1.0),
    );
    scene.mesh(
        "plane",
        grey,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(300.0, 1.0, 300.0),
    );
    scene.sun(Vec3::new(-0.3, -1.0, -0.4), 4.0);
    let camera = scene.camera(view.0, view.1, camera(msaa));
    Shot {
        scene,
        camera,
        wall,
    }
}

/// The MSAA the occlusion tests draw with: 4× (what the goldens show) where the device resolves
/// min depth, else single-sample, where occlusion needs no resolve.
fn msaa_for(r: &Renderer) -> Msaa {
    match r.device().caps().depth_resolve_min {
        true => Msaa::X4,
        false => Msaa::Off,
    }
}

/// Whether a frame drawn with `msaa` is compared with the suite's (4× MSAA) goldens.
fn has_goldens(msaa: Msaa, test: &str) -> bool {
    if msaa != Msaa::X4 {
        println!("{test}: goldens skipped (single-sample: no min depth resolve on this device)");
    }
    msaa == Msaa::X4
}

/// `shot` on `r` with occlusion off, then with it on.
fn on_and_off(r: &mut Renderer, shot: &mut Shot) -> (Image, Image) {
    r.set_culling(NO_OCCLUSION);
    let off = shot.scene.render(r, SIZE.0, SIZE.1);
    r.set_culling(Culling::default());
    let on = shot.scene.render(r, SIZE.0, SIZE.1);
    (off, on)
}

/// The passes `r` timed for its last frame, rendering `shot` in perf mode.
fn passes(r: &mut Renderer, shot: &mut Shot) -> Vec<String> {
    r.set_perf_mode(true);
    let _ = shot.scene.render(r, SIZE.0, SIZE.1);
    r.set_perf_mode(false);
    r.stats().timed_passes
}

/// True if `names` hold the split forward pass in order (`meshlet.cull_late` on the compute path).
fn split(names: &[String], compute: bool) -> bool {
    let at = |n: &str| names.iter().position(|p| p == n);
    let order = match compute {
        true => vec![
            "forward",
            "meshlet.hzb",
            "meshlet.cull_late",
            "forward_late",
        ],
        false => vec!["forward", "meshlet.hzb", "forward_late"],
    };
    let found: Option<Vec<usize>> = order.iter().map(|n| at(n)).collect();
    found.is_some_and(|f| f.windows(2).all(|w| w[0] < w[1]))
}

fn compute_path(r: &Renderer) -> bool {
    r.stats().raster_path == concordia_render::RasterPath::compute_indirect
}

/// With occlusion on, the forward pass splits around the HZB build (and, on the compute path, pass
/// 2's culling), with MSAA on and off and on both paths; the split frame matches the single pass.
#[test]
fn split_forward_pass_matches_the_single_pass() {
    for mut r in meshlet_renderers() {
        for msaa in [Msaa::X4, Msaa::Off] {
            if msaa == Msaa::X4 && msaa_for(&r) == Msaa::Off {
                // devices_without_depth_resolve_min_skip_occlusion covers MSAA here.
                continue;
            }
            let mut shot = city(BACK, msaa);
            let (off, on) = on_and_off(&mut r, &mut shot);
            let what = format!("{msaa:?} {:?}: occlusion on vs off", r.stats().raster_path);
            assert_culling_agrees(SUITE, "camera_cut", (&off, &on), &what);
            let names = passes(&mut r, &mut shot);
            assert!(split(&names, compute_path(&r)), "{what}: {names:?}");
            r.set_culling(NO_OCCLUSION);
            let single = passes(&mut r, &mut shot);
            assert!(
                !single
                    .iter()
                    .any(|p| p == "forward_late" || p == "meshlet.hzb")
            );
        }
    }
}

/// Without min depth resolves, an MSAA frame keeps one forward pass and culls nothing by
/// occlusion; a single-sample frame still splits (its depth needs no resolve).
#[test]
fn devices_without_depth_resolve_min_skip_occlusion() {
    let mask = CapsMask {
        depth_resolve_min: true,
        ..CapsMask::default()
    };
    for mut r in common::meshlet_renderers_masked(&mask) {
        assert!(!r.device().caps().depth_resolve_min);
        let mut msaa = city(FRONT, Msaa::X4);
        let (off, on) = on_and_off(&mut r, &mut msaa);
        let _ = msaa.scene.render(&mut r, SIZE.0, SIZE.1);
        let m = r.stats().meshlets;
        assert_eq!(
            (m.after_occlusion, m.drawn_late),
            (m.after_cone, 0),
            "{m:?}"
        );
        assert_culling_agrees(SUITE, "occluded_city", (&off, &on), "masked, MSAA");
        let names = passes(&mut r, &mut msaa);
        assert!(!names.iter().any(|p| p == "meshlet.hzb"), "{names:?}");
        let mut single = city(FRONT, Msaa::Off);
        let _ = single.scene.render(&mut r, SIZE.0, SIZE.1);
        let _ = single.scene.render(&mut r, SIZE.0, SIZE.1);
        let m = r.stats().meshlets;
        assert!(m.after_occlusion < m.after_cone, "{m:?}");
        let names = passes(&mut r, &mut single);
        assert!(split(&names, compute_path(&r)), "{names:?}");
    }
}

/// From the second frame (the first has no history, by design), at least 90 % of the meshlets that
/// pass the frustum and cone tests are hidden behind the wall; the image is the unoccluded one.
#[test]
fn occlusion_culls_at_least_90_percent_in_occluded_city_scene() {
    for mut r in meshlet_renderers() {
        let msaa = msaa_for(&r);
        let mut shot = city(FRONT, msaa);
        let (off, _) = on_and_off(&mut r, &mut shot);
        let on = shot.scene.render(&mut r, SIZE.0, SIZE.1);
        let m = r.stats().meshlets;
        println!("{:?}: {m:?}", r.stats().raster_path);
        assert!(m.after_cone > 10_000, "the city is in view: {m:?}");
        assert!(
            m.after_occlusion * 10 <= m.after_cone,
            "at least 90 % hidden: {m:?}"
        );
        assert_eq!(m.meshlets, m.drawn_early + m.drawn_late);
        assert_culling_agrees(SUITE, "occluded_city", (&off, &on), "frame 2");
        if has_goldens(msaa, "occluded city") {
            golden_in(SUITE, "occluded_city", &off);
        }
    }
}

/// A fresh renderer has no history: pass 1 draws no meshlets and pass 2 draws all that are
/// visible. The next frame of the same view draws them all in pass 1, and nothing in pass 2.
#[test]
fn first_frame_draws_everything_in_pass_two() {
    for mut r in meshlet_renderers() {
        r.set_cull_readback(true);
        let mut shot = city(BACK, msaa_for(&r));
        let _ = shot.scene.render(&mut r, SIZE.0, SIZE.1);
        let first = r.stats().meshlets;
        assert_eq!(first.drawn_early, 0, "{first:?}");
        assert!(first.drawn_late > 0 && first.drawn_late == first.meshlets);
        let codes = r.read_culling().unwrap();
        assert!(
            !codes
                .iter()
                .any(|m| m.view == 0 && m.code == Some(CullCode::Drawn))
        );
        let _ = shot.scene.render(&mut r, SIZE.0, SIZE.1);
        let second = r.stats().meshlets;
        assert_eq!(
            (second.drawn_early, second.drawn_late),
            (first.meshlets, 0),
            "{second:?}"
        );
    }
}

/// A camera cut, both ways: frame N from one side of the city, frame N + 1 from the other. Pass 1
/// draws last frame's set, which is wrong for the new view, and pass 2 draws everything else, so
/// the cut frame equals the unoccluded one.
#[test]
fn two_pass_occlusion_has_no_false_negatives_on_camera_cut() {
    for mut r in meshlet_renderers() {
        let msaa = msaa_for(&r);
        let mut goldens = Vec::new();
        for (from, to, name) in [(FRONT, BACK, "camera_cut"), (BACK, FRONT, "occluded_city")] {
            let mut shot = city(to, msaa);
            r.set_culling(NO_OCCLUSION);
            let off = shot.scene.render(&mut r, SIZE.0, SIZE.1);
            r.set_culling(Culling::default());
            shot.scene.aim(shot.camera, from.0, from.1);
            for _ in 0..2 {
                let _ = shot.scene.render(&mut r, SIZE.0, SIZE.1);
            }
            shot.scene.aim(shot.camera, to.0, to.1);
            let cut = shot.scene.render(&mut r, SIZE.0, SIZE.1);
            let m = r.stats().meshlets;
            let what = format!("{:?} cut to {name}: {m:?}", r.stats().raster_path);
            assert_culling_agrees(SUITE, name, (&off, &cut), &what);
            if name == "camera_cut" {
                assert!(m.drawn_late > m.drawn_early, "pass 2 does the work: {what}");
                goldens.push(off);
            }
        }
        if has_goldens(msaa, "camera cut") {
            goldens
                .iter()
                .for_each(|g| golden_in(SUITE, "camera_cut", g));
        }
    }
}

/// A field of meshlet balls behind a wall sliding across it for 120 frames at 320 × 180: every
/// frame with occlusion on (keeping its history) equals the same frame with occlusion off.
#[test]
fn two_pass_occlusion_has_no_false_negatives_with_moving_occluder() {
    let renderers = meshlet_renderers();
    let offs = meshlet_renderers();
    for (mut on, mut off) in renderers.into_iter().zip(offs) {
        off.set_culling(NO_OCCLUSION);
        let msaa = msaa_for(&on);
        let mut shot = field(msaa);
        let mut late = 0;
        let mut goldens = Vec::new();
        for frame in 0..120u32 {
            let x = -14.0 + 28.0 * frame as f32 / 119.0;
            let t = concordia_scene::builtin::Transform {
                translation: Vec3::new(x, 2.0, -6.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::new(8.0, 4.0, 1.0),
            };
            shot.scene.world.insert(shot.wall, t).unwrap();
            let want = shot.scene.render(&mut off, SIZE.0, SIZE.1);
            let got = shot.scene.render(&mut on, SIZE.0, SIZE.1);
            late += on.stats().meshlets.drawn_late;
            // Frames between the goldens share the middle golden's culling tolerance.
            let (name, golden) = match frame {
                0 => ("moving_occluder_000", true),
                60 => ("moving_occluder_060", true),
                119 => ("moving_occluder_119", true),
                _ => ("moving_occluder_060", false),
            };
            let what = format!("{:?} frame {frame}", on.stats().raster_path);
            assert_culling_agrees(SUITE, name, (&want, &got), &what);
            if golden {
                goldens.push((name, want));
            }
        }
        assert!(late > 0, "balls the wall uncovers are drawn in pass 2");
        let goldens: Vec<(&str, &Image)> = goldens.iter().map(|(n, i)| (*n, i)).collect();
        if has_goldens(msaa, "moving occluder") {
            common::assert_goldens_in(SUITE, &goldens);
        }
    }
}

/// Twenty-four dense meshlet balls on a ground, a wall in front of them (moved by the test), a sun;
/// the camera draws with `msaa`.
fn field(msaa: Msaa) -> Shot {
    let mut scene = Scene::new();
    let ball = scene.assets.insert_mesh(
        "ball",
        with_meshlets(concordia_assets::shapes::sphere(1.0, 96, 48)),
    );
    let red = scene.material("red", lit([0.8, 0.25, 0.1]));
    let grey = scene.material("grey", lit([0.5, 0.5, 0.55]));
    for i in 0..6 {
        for j in 0..4 {
            let at = Vec3::new(-7.5 + 3.0 * i as f32, 1.0, -10.0 - 3.0 * j as f32);
            let id = scene.entity(at, Quat::IDENTITY, Vec3::ONE);
            scene
                .world
                .insert(
                    id,
                    concordia_assets::MeshRenderer {
                        mesh: ball.clone(),
                        material: red.clone(),
                        cast_shadows: true,
                        ..Default::default()
                    },
                )
                .unwrap();
        }
    }
    let wall = scene.mesh(
        "cube",
        grey.clone(),
        Vec3::new(-14.0, 2.0, -6.0),
        Quat::IDENTITY,
        Vec3::new(8.0, 4.0, 1.0),
    );
    scene.mesh(
        "plane",
        grey,
        Vec3::ZERO,
        Quat::IDENTITY,
        Vec3::new(80.0, 1.0, 80.0),
    );
    scene.sun(Vec3::new(-0.3, -1.0, -0.4), 4.0);
    let camera = scene.camera(
        Vec3::new(0.0, 2.0, 2.0),
        Vec3::new(0.0, 1.0, -12.0),
        camera(msaa),
    );
    Shot {
        scene,
        camera,
        wall,
    }
}

/// The `occlusion` view tints meshlets by the pass that drew them (pass 1 green, pass 2 orange, M1
/// draws grey) and the `hzb` view shows the HZB's level 0 as grey depth, on a frame where the
/// wall has just moved (so both passes draw), the same on both paths.
#[test]
fn occlusion_and_hzb_view_modes_golden() {
    for mut r in meshlet_renderers() {
        let msaa = msaa_for(&r);
        let mut shot = field(msaa);
        let mut images = Vec::new();
        for mode in [ViewMode::occlusion, ViewMode::hzb] {
            for (frame, x) in [(0, -6.0), (1, 2.0)] {
                let t = concordia_scene::builtin::Transform {
                    translation: Vec3::new(x, 2.0, -6.0),
                    rotation: Quat::IDENTITY,
                    scale: Vec3::new(8.0, 4.0, 1.0),
                };
                shot.scene.world.insert(shot.wall, t).unwrap();
                let image = shot.scene.render_mode(&mut r, SIZE.0, SIZE.1, mode);
                if frame == 1 {
                    images.push((mode, image));
                }
            }
            let m = r.stats().meshlets;
            assert!(m.drawn_early > 0 && m.drawn_late > 0, "{mode:?}: {m:?}");
        }
        let names = ["view_occlusion", "view_hzb"];
        let goldens: Vec<(&str, &Image)> =
            images.iter().zip(names).map(|((_, i), n)| (n, i)).collect();
        if has_goldens(msaa, "occlusion and hzb views") {
            common::assert_goldens_in(SUITE, &goldens);
        }
    }
}

/// Timings only (perf is deferred): the occluded city at 1280×720, 150 frames after 30 warm-up
/// frames, on both paths with occlusion off and on; prints each pass's median. Run on the
/// reference Mac: `cargo nextest run -p concordia-render --release --run-ignored only
/// -E 'test(occluded_city_timings)' --no-capture`.
#[test]
#[ignore = "timings for the As built, run by hand in release"]
fn occluded_city_timings() {
    for mut r in meshlet_renderers() {
        r.set_perf_mode(true);
        let path = format!("{:?}", common::meshlet_path(&r));
        let mut shot = city(FRONT, Msaa::X4);
        for (label, culling) in [
            ("no occlusion", NO_OCCLUSION),
            ("occlusion", Culling::default()),
        ] {
            r.set_culling(culling);
            for (name, ms) in common::median_pass_times(&mut r, &mut shot.scene, (1280, 720), 150) {
                println!("{path} ({label}) {name}: median {ms:.3} ms");
            }
            println!("{path} ({label}) meshlets: {:?}", r.stats().meshlets);
        }
    }
}
