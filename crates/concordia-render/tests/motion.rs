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
    r.set_motion_readback(true);
    r.set_meshlets(true);
    let rw = RenderWorld::extract(&scene.world);
    let request = RenderRequest {
        camera: None,
        view: Some(view.clone()),
        width: SIZE.0,
        height: SIZE.1,
        overlays: None,
        view_mode: concordia_render::ViewMode::lit,
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

#[test]
fn blended_draws_leave_motion_untouched() {
    for mut r in renderers() {
        let glass = M1 {
            base_color: [0.3, 0.6, 1.0, 0.5],
            alpha_mode: concordia_assets::AlphaMode::Blend,
            ..common::unlit([1.0; 4])
        };
        let mut q = quads(glass);
        let eye = view(Vec3::new(0.0, 0.0, 5.0));
        let _ = motion(&mut r, &mut q.scene, &eye);
        move_to(&mut q.scene, q.m1, Vec3::new(-0.7, 0.0, 0.0));
        let moved = motion(&mut r, &mut q.scene, &eye);
        assert!(
            moved.uv.iter().all(|m| *m == [0.0, 0.0]),
            "a moving translucent quad writes no motion"
        );
    }
}
