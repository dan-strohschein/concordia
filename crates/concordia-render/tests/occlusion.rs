//! Two-pass HZB occlusion on the meshlet path (ADR-0100, M4 4.4b): pass 1 draws what was visible
//! last frame, the HZB is built from its depth, and pass 2 draws whatever else isn't hidden. It
//! never loses geometry (occlusion on against off, on both draw paths: camera cuts, a moving
//! occluder, the first frame), culls the hidden city, and devices without min depth resolves skip
//! it when they draw with MSAA.

mod common;
use common::{M1, Scene, assert_culling_agrees, meshlet_renderers, with_meshlets};
use concordia_core::math::{Quat, Vec3};
use concordia_render::{Camera, CullCode, Culling, Image, Msaa, Renderer};
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

/// A scene to render.
struct Shot {
    scene: Scene,
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
                    },
                )
                .unwrap();
        }
    }
    scene.mesh(
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
    scene.camera(view.0, view.1, camera(msaa));
    Shot { scene }
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

/// A fresh renderer has no history: pass 1 draws no meshlets and pass 2 draws all that are
/// visible. The next frame of the same view draws them all in pass 1, and nothing in pass 2.
#[test]
fn first_frame_draws_everything_in_pass_two() {
    for mut r in meshlet_renderers() {
        r.set_cull_readback(true);
        let mut shot = city(BACK, Msaa::X4);
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
