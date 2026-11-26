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
