//! Motion vectors (M4 4.3b2): the forward pass writes where each pixel's surface is now minus where
//! it was last frame (UV units, y down) on every path: M1, the compute path and mesh shaders.
//! Read back with `Renderer::read_motion`.

mod common;
use common::{M1, Scene, compute_renderer, renderer, with_meshlets};
use concordia_assets::{MeshData, MeshRenderer, shapes};
use concordia_core::id::EntityId;
use concordia_core::math::{Quat, Vec3};
use concordia_render::{
    Camera, MotionImage, Projection, RenderRequest, RenderWorld, Renderer, View,
};
use concordia_scene::builtin::Transform;

const SIZE: (u32, u32) = (320, 180);
/// `Rg16Float` keeps about 0.005 px of these motions; 0.01 px is the bound.
const PX: f32 = 0.01;

/// A flat `n`×`n` grid of quads in the XY plane, 1 × 1, facing +Z.
fn grid(n: u32) -> MeshData {
    let step = 1.0 / n as f32;
    let mut vertices = Vec::new();
    for y in 0..=n {
        for x in 0..=n {
            let p = [x as f32 * step - 0.5, y as f32 * step - 0.5, 0.0];
            vertices.push((p, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.0, 0.0]));
        }
    }
    let at = |x: u32, y: u32| y * (n + 1) + x;
    let indices = (0..n)
        .flat_map(|y| (0..n).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            [
                at(x, y),
                at(x + 1, y),
                at(x + 1, y + 1),
                at(x, y),
                at(x + 1, y + 1),
                at(x, y + 1),
            ]
        })
        .collect();
    shapes::mesh(vertices, indices)
}

/// The scene: a quad on the M1 path at x = -0.8 and one with meshlets at x = 0.8, both 1.2 wide.
struct Quads {
    scene: Scene,
    m1: EntityId,
    meshlets: EntityId,
}

fn quads(material: M1) -> Quads {
    let mut scene = Scene::new();
    let plain = scene.assets.insert_mesh("plain", grid(4));
    let dense = scene.assets.insert_mesh("dense", with_meshlets(grid(64)));
    let paint = scene.material("paint", material);
    let mut put = |mesh, x: f32| {
        let id = scene.entity(Vec3::new(x, 0.0, 0.0), Quat::IDENTITY, Vec3::splat(1.2));
        let renderer = MeshRenderer {
            mesh,
            material: paint.clone(),
            cast_shadows: false,
            ..Default::default()
        };
        scene.world.insert(id, renderer).unwrap();
        id
    };
    let m1 = put(plain, -0.8);
    let meshlets = put(dense, 0.8);
    scene.camera(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Camera::default());
    Quads {
        scene,
        m1,
        meshlets,
    }
}

fn view(eye: Vec3) -> View {
    View {
        eye,
        target: eye - Vec3::Z * 5.0,
        projection: Projection::default(),
    }
}

/// Renders `scene` from `view` and returns its motion vectors.
fn motion(r: &mut Renderer, scene: &mut Scene, view: &View) -> MotionImage {
    motion_with(r, scene, view, (concordia_render::ViewMode::lit, None))
}

/// [`motion`] in `view_mode`, with editor `overlays`.
fn motion_with(
    r: &mut Renderer,
    scene: &mut Scene,
    view: &View,
    (view_mode, overlays): (
        concordia_render::ViewMode,
        Option<concordia_render::Overlays>,
    ),
) -> MotionImage {
    r.set_motion_readback(true);
    r.set_meshlets(true);
    let rw = RenderWorld::extract(&scene.world);
    let request = RenderRequest {
        camera: None,
        view: Some(view.clone()),
        width: SIZE.0,
        height: SIZE.1,
        overlays,
        view_mode,
        debug: Vec::new(),
        ui: Default::default(),
    };
    r.render(&rw, &mut scene.assets, &request).expect("render");
    r.read_motion().expect("motion readback is on").clone()
}

fn move_to(scene: &mut Scene, id: EntityId, at: Vec3) {
    let t = Transform {
        translation: at,
        rotation: Quat::IDENTITY,
        scale: Vec3::splat(1.2),
    };
    scene.world.insert(id, t).unwrap();
}

/// Pixel (x, y) of `point` from `view`.
fn project(view: &View, point: Vec3) -> [f32; 2] {
    view.project(point, SIZE.0 as f32, SIZE.1 as f32)
        .expect("in view")
}

/// Pixels well inside each quad (its center ± a third of its half-width on screen).
fn inside(view: &View, center: Vec3) -> Vec<(u32, u32)> {
    let [cx, cy] = project(view, center);
    let [ex, _] = project(view, center + Vec3::X * 0.6);
    let r = ((ex - cx) / 3.0) as i32;
    (-r..=r)
        .step_by(3)
        .flat_map(|dy| (-r..=r).step_by(3).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| ((cx as i32 + dx) as u32, (cy as i32 + dy) as u32))
        .collect()
}

fn renderers() -> Vec<Renderer> {
    let mut all: Vec<Renderer> = renderer().into_iter().collect();
    if all
        .first()
        .is_some_and(|r| common::meshlet_path(r) == concordia_render::RasterPath::mesh_shader)
    {
        all.extend(compute_renderer());
    }
    all
}

fn assert_motion(m: &MotionImage, pixels: &[(u32, u32)], want: [f32; 2], what: &str) {
    for &(x, y) in pixels {
        let got = m.pixels(x, y);
        assert!(
            (got[0] - want[0]).abs() <= PX && (got[1] - want[1]).abs() <= PX,
            "{what} at ({x}, {y}): {got:?}, want {want:?}"
        );
    }
}

#[test]
fn motion_vectors_zero_for_static_scene_and_exact_for_translation() {
    for mut r in renderers() {
        let mut q = quads(common::unlit([1.0; 4]));
        let eye = view(Vec3::new(0.0, 0.0, 5.0));
        let _ = motion(&mut r, &mut q.scene, &eye);
        let still = motion(&mut r, &mut q.scene, &eye);
        assert!(
            still.uv.iter().all(|m| *m == [0.0, 0.0]),
            "a static scene has no motion"
        );
        let d = Vec3::new(0.1, 0.05, 0.0);
        let (a, b) = (Vec3::new(-0.8, 0.0, 0.0), Vec3::new(0.8, 0.0, 0.0));
        move_to(&mut q.scene, q.m1, a + d);
        move_to(&mut q.scene, q.meshlets, b + d);
        let moved = motion(&mut r, &mut q.scene, &eye);
        for (center, path) in [(a, "M1"), (b, "meshlets")] {
            let [x0, y0] = project(&eye, center);
            let [x1, y1] = project(&eye, center + d);
            let want = [x1 - x0, y1 - y0];
            assert!(
                want[0] > 3.0 && want[1] < -1.0,
                "a visible motion: {want:?}"
            );
            let what = format!("{path} quad, {:?}", r.stats().raster_path);
            assert_motion(&moved, &inside(&eye, center + d), want, &what);
        }
        assert_eq!(moved.pixels(2, 2), [0.0, 0.0], "the background didn't move");
    }
}

/// The camera's sub-pixel jitter (M4 4.6) moves only where pixels are rasterized: a still scene
/// rendered with a different offset each frame still has exactly zero motion, on every path.
#[test]
fn still_scene_motion_stays_zero_under_jitter() {
    use concordia_render::temporal::SubpixelOverride;
    for mut r in renderers() {
        let mut q = quads(common::unlit([1.0; 4]));
        let eye = view(Vec3::new(0.0, 0.0, 5.0));
        for (frame, offset) in [[0.3, -0.2], [-0.4, 0.45], [0.1, 0.1]]
            .into_iter()
            .enumerate()
        {
            r.set_subpixel_override(Some(SubpixelOverride {
                offset,
                mip_bias: 0.0,
            }));
            let m = motion(&mut r, &mut q.scene, &eye);
            let what = format!("frame {frame}, {:?}", r.stats().raster_path);
            assert!(m.uv.iter().all(|m| *m == [0.0, 0.0]), "{what}");
        }
    }
}

#[test]
fn camera_motion_moves_static_pixels() {
    for mut r in renderers() {
        let mut q = quads(common::unlit([1.0; 4]));
        let before = view(Vec3::new(0.0, 0.0, 5.0));
        let after = view(Vec3::new(-0.15, 0.1, 5.0));
        let _ = motion(&mut r, &mut q.scene, &before);
        let moved = motion(&mut r, &mut q.scene, &after);
        for center in [Vec3::new(-0.8, 0.0, 0.0), Vec3::new(0.8, 0.0, 0.0)] {
            let [x0, y0] = project(&before, center);
            let [x1, y1] = project(&after, center);
            assert_motion(
                &moved,
                &inside(&after, center),
                [x1 - x0, y1 - y0],
                "camera",
            );
        }
    }
}

#[test]
fn first_frame_and_reset_have_zero_motion() {
    for mut r in renderers() {
        let mut q = quads(common::unlit([1.0; 4]));
        let eye = view(Vec3::new(0.0, 0.0, 5.0));
        let first = motion(&mut r, &mut q.scene, &eye);
        assert!(first.uv.iter().all(|m| *m == [0.0, 0.0]), "first frame");
        move_to(&mut q.scene, q.m1, Vec3::new(0.0, 0.3, 0.0));
        r.reset_motion_history();
        let cut = motion(&mut r, &mut q.scene, &view(Vec3::new(1.0, 0.0, 5.0)));
        assert!(cut.uv.iter().all(|m| *m == [0.0, 0.0]), "after a reset");
    }
}

/// A still translucent pane in front of a moving opaque quad leaves the quad's motion as it is:
/// blended draws write no motion.
#[test]
fn blended_draws_leave_motion_untouched() {
    for mut r in renderers() {
        let mut q = quads(common::unlit([1.0; 4]));
        let glass = q.scene.material(
            "glass",
            M1 {
                base_color: [0.3, 0.6, 1.0, 0.5],
                alpha_mode: concordia_assets::AlphaMode::Blend,
                ..common::unlit([1.0; 4])
            },
        );
        let pane = q.scene.assets.insert_mesh("pane", grid(2));
        let id = q
            .scene
            .entity(Vec3::new(0.0, 0.0, 0.5), Quat::IDENTITY, Vec3::splat(4.0));
        let renderer = MeshRenderer {
            mesh: pane,
            material: glass,
            cast_shadows: false,
            ..Default::default()
        };
        q.scene.world.insert(id, renderer).unwrap();
        let eye = view(Vec3::new(0.0, 0.0, 5.0));
        let _ = motion(&mut r, &mut q.scene, &eye);
        let (from, to) = (Vec3::new(-0.8, 0.0, 0.0), Vec3::new(-0.7, 0.0, 0.0));
        move_to(&mut q.scene, q.m1, to);
        let moved = motion(&mut r, &mut q.scene, &eye);
        let ([x0, y0], [x1, y1]) = (project(&eye, from), project(&eye, to));
        assert_motion(
            &moved,
            &inside(&eye, to),
            [x1 - x0, y1 - y0],
            "behind the pane",
        );
        assert_eq!(moved.pixels(2, 2), [0.0, 0.0], "the pane itself is still");
    }
}

/// Debug views write no motion: overdraw and wireframe draw every surface, hidden ones too.
#[test]
fn debug_views_write_no_motion() {
    for mut r in renderers() {
        let mut q = quads(common::unlit([1.0; 4]));
        let eye = view(Vec3::new(0.0, 0.0, 5.0));
        for mode in [
            concordia_render::ViewMode::overdraw,
            concordia_render::ViewMode::wireframe,
        ] {
            let _ = motion_with(&mut r, &mut q.scene, &eye, (mode, None));
            move_to(&mut q.scene, q.m1, Vec3::new(-0.6, 0.1, 0.0));
            let moved = motion_with(&mut r, &mut q.scene, &eye, (mode, None));
            assert!(moved.uv.iter().all(|m| *m == [0.0, 0.0]), "{mode:?}");
        }
    }
}

/// An environment shown as the skybox.
fn skybox(scene: &mut Scene) {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/hdri/kloofendal_48d_partly_cloudy_puresky_512.hdr"
    ))
    .unwrap();
    let image = concordia_assets::ImageSettings::default();
    let texture = concordia_assets::decode_image(&bytes, &image).unwrap();
    let map = scene.assets.insert_texture("sky", texture);
    let id = scene.entity(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    let environment = concordia_render::Environment {
        map,
        intensity: 1.0,
        skybox: true,
    };
    scene.world.insert(id, environment).unwrap();
}

/// The sky is infinitely far: the camera's rotation moves it, its translation doesn't.
#[test]
fn the_sky_moves_with_the_cameras_rotation_only() {
    for mut r in renderers() {
        let mut q = quads(common::unlit([1.0; 4]));
        skybox(&mut q.scene);
        let eye = Vec3::new(0.0, 0.0, 5.0);
        let _ = motion(&mut r, &mut q.scene, &view(eye));
        let shifted = view(eye + Vec3::new(0.3, 0.2, 0.0));
        let moved = motion(&mut r, &mut q.scene, &shifted);
        assert_eq!(moved.pixels(2, 2), [0.0, 0.0], "translation");
        let turned = View {
            eye: shifted.eye,
            target: shifted.eye + Vec3::new(-0.4, 0.0, -5.0),
            projection: Projection::default(),
        };
        let turning = motion(&mut r, &mut q.scene, &turned);
        // A sky direction up and left of the quads; both views share the eye, so any point along
        // it projects where the direction does.
        let sky = shifted.eye + Vec3::new(-2.2, 1.1, -5.0);
        let ([x0, y0], [x1, y1]) = (project(&shifted, sky), project(&turned, sky));
        let want = [x1 - x0, y1 - y0];
        assert!(want[0] > 5.0, "turning left moves the sky right: {want:?}");
        let got = turning.pixels(x1 as u32, y1 as u32);
        assert!(
            (got[0] - want[0]).abs() < 0.3 && (got[1] - want[1]).abs() < 0.3,
            "sky motion {got:?}, want {want:?}"
        );
    }
}

/// The editor grid writes no motion: a quad moving below it keeps its own, seen through the grid.
#[test]
fn the_editor_grid_leaves_motion_untouched() {
    for mut r in renderers() {
        let mut q = quads(common::unlit([1.0; 4]));
        let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let m1 = q.m1;
        let lie = |scene: &mut Scene, x: f32| {
            let t = Transform {
                translation: Vec3::new(x, -0.5, 0.0),
                rotation: flat,
                scale: Vec3::splat(1.2),
            };
            scene.world.insert(m1, t).unwrap();
        };
        lie(&mut q.scene, -0.8);
        let above = View {
            eye: Vec3::new(0.0, 3.0, 3.0),
            target: Vec3::new(0.0, -0.5, 0.0),
            projection: Projection::default(),
        };
        let grid = concordia_render::Overlays {
            grid: true,
            ..Default::default()
        };
        let mode = concordia_render::ViewMode::lit;
        let _ = motion_with(&mut r, &mut q.scene, &above, (mode, Some(grid.clone())));
        lie(&mut q.scene, -0.6);
        let moved = motion_with(&mut r, &mut q.scene, &above, (mode, Some(grid)));
        let [x, y] = project(&above, Vec3::new(-0.6, -0.5, 0.0));
        let [mx, _] = moved.pixels(x as u32, y as u32);
        assert!(mx > 2.0, "the quad's motion under the grid: {mx}");
    }
}

/// The `motion` view: still pixels black, moving ones bright, hued by direction (4.3b2). Golden
/// `m4_motion/view_motion`: the quads moving apart.
#[test]
fn motion_view_colors_moving_pixels_by_direction() {
    let Some(mut r) = renderer() else { return };
    let mut q = quads(common::unlit([1.0; 4]));
    let eye = view(Vec3::new(0.0, 0.0, 5.0));
    let frame = |r: &mut Renderer, q: &mut Quads| {
        let rw = RenderWorld::extract(&q.scene.world);
        let request = RenderRequest {
            camera: None,
            view: Some(eye.clone()),
            width: SIZE.0,
            height: SIZE.1,
            overlays: None,
            view_mode: concordia_render::ViewMode::motion,
            debug: Vec::new(),
            ui: Default::default(),
        };
        r.render(&rw, &mut q.scene.assets, &request)
            .expect("render")
    };
    let still = frame(&mut r, &mut q);
    let [cx, cy] = project(&eye, Vec3::new(-0.8, 0.0, 0.0));
    assert_eq!(
        common::pixel(&still, cx as u32, cy as u32)[..3],
        [0, 0, 0],
        "still"
    );
    move_to(&mut q.scene, q.m1, Vec3::new(-1.0, 0.0, 0.0));
    move_to(&mut q.scene, q.meshlets, Vec3::new(1.0, 0.0, 0.0));
    let moving = frame(&mut r, &mut q);
    let at = |x: f32| {
        let [px, py] = project(&eye, Vec3::new(x, 0.0, 0.0));
        common::pixel(&moving, px as u32, py as u32)
    };
    let (left, right) = (at(-1.0), at(1.0));
    // About 7 pixels of motion each: under half brightness, cyan going left, red going right.
    assert!(
        left[0] < 20 && left[1] > 100 && left[2] > 100,
        "left: {left:?}"
    );
    assert!(
        right[0] > 100 && right[1] < 20 && right[2] < 20,
        "right: {right:?}"
    );
    assert_eq!(
        common::pixel(&moving, 2, 2)[..3],
        [0, 0, 0],
        "the background is still"
    );
    common::golden_in("m4_motion", "view_motion", &moving);
}
