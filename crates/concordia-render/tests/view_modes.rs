//! Viewport view modes (M1 phase 1.16): debug views of what the renderer does. Each mode has a
//! golden (inspected by eye once), plus structural checks that do not depend on the goldens.

mod common;

use common::{M1, Scene, check_golden, pixel, renderer};
use concordia_core::math::{Quat, Vec3};
use concordia_render::shadow::CascadeSettings;
use concordia_render::{Camera, DirectionalLight, Image, PointLight, Renderer, View, ViewMode};

const W: u32 = 320;
const H: u32 = 180;

/// A grey ground, a red cube, a smooth metal sphere, a shadowing sun and three point lights.
fn scene() -> Scene {
    let mut s = Scene::new();
    let grey = s.material(
        "grey",
        M1 {
            base_color: [0.5, 0.5, 0.5, 1.0],
            roughness: 0.8,
            metallic: 0.0,
            ..M1::default()
        },
    );
    let red = s.material(
        "red",
        M1 {
            base_color: [0.8, 0.1, 0.1, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            ..M1::default()
        },
    );
    let metal = s.material(
        "metal",
        M1 {
            base_color: [0.9, 0.9, 0.9, 1.0],
            roughness: 0.2,
            metallic: 1.0,
            ..M1::default()
        },
    );
    s.mesh("plane", grey, Vec3::ZERO, Quat::IDENTITY, Vec3::splat(20.0));
    s.mesh(
        "cube",
        red,
        Vec3::new(-1.0, 0.5, 0.0),
        Quat::from_rotation_y(0.4),
        Vec3::ONE,
    );
    s.mesh(
        "sphere",
        metal,
        Vec3::new(1.2, 0.6, 0.5),
        Quat::IDENTITY,
        Vec3::splat(1.2),
    );
    let sun = s.sun(Vec3::new(-0.4, -1.0, -0.3), 20_000.0);
    s.world
        .insert(
            sun,
            DirectionalLight {
                color: [1.0; 3],
                illuminance_lux: 20_000.0,
                shadows: Some(CascadeSettings {
                    cascades: 3,
                    resolution: 1024,
                    ..CascadeSettings::default()
                }),
            },
        )
        .unwrap();
    for (i, x) in [-3.0f32, 0.0, 3.0].into_iter().enumerate() {
        let light = s.entity(Vec3::new(x, 1.0, 2.0), Quat::IDENTITY, Vec3::ONE);
        let mut color = [0.2; 3];
        color[i] = 1.0;
        s.world
            .insert(
                light,
                PointLight {
                    color,
                    intensity_lumens: 100_000.0,
                    range: 6.0,
                    ..Default::default()
                },
            )
            .unwrap();
    }
    s.camera(EYE, TARGET, Camera::default());
    s
}

const EYE: Vec3 = Vec3::new(0.0, 3.0, 8.0);
const TARGET: Vec3 = Vec3::new(0.0, 0.3, 0.0);

/// Where `point` lands in the image.
fn project(point: Vec3) -> [u32; 2] {
    let view = View {
        eye: EYE,
        target: TARGET,
        projection: Camera::default().projection,
    };
    let [x, y] = view.project(point, W as f32, H as f32).expect("in view");
    [x as u32, y as u32]
}

fn render(r: &mut Renderer, s: &mut Scene, mode: ViewMode) -> Image {
    s.render_mode(r, W, H, mode)
}

const MODES: [(ViewMode, &str); 9] = [
    (ViewMode::lit, "lit"),
    (ViewMode::unlit, "unlit"),
    (ViewMode::wireframe, "wireframe"),
    (ViewMode::normals, "normals"),
    (ViewMode::albedo, "albedo"),
    (ViewMode::roughness_metallic, "roughness_metallic"),
    (ViewMode::overdraw, "overdraw"),
    (ViewMode::shadow_cascades, "shadow_cascades"),
    (ViewMode::light_clusters, "light_clusters"),
];

#[test]
fn every_view_mode_matches_its_golden() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene();
    let failed: Vec<String> = MODES
        .into_iter()
        .filter_map(|(mode, name)| {
            let image = render(&mut r, &mut s, mode);
            check_golden(&format!("view_mode_{name}"), &image).err()
        })
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn every_view_mode_differs_from_lit() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene();
    let lit = render(&mut r, &mut s, ViewMode::lit);
    for (mode, name) in &MODES[1..] {
        let image = render(&mut r, &mut s, *mode);
        let changed = lit
            .rgba
            .chunks(4)
            .zip(image.rgba.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            changed > (W * H / 10) as usize,
            "{name} changed only {changed} pixels"
        );
    }
}

/// The ground, below the objects in the image's bottom middle.
const GROUND: (u32, u32) = (W / 2, H - 8);

fn near(got: u8, want: u8, name: &str) {
    assert!(got.abs_diff(want) <= 3, "{name}: {got} vs {want}");
}

#[test]
fn data_views_show_raw_material_and_geometry_values() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene();
    let (x, y) = GROUND;
    // sRGB encodings: 0.5 → 188, 0.8 → 231, 1.0 → 255, 0 → 0.
    let normals = pixel(&render(&mut r, &mut s, ViewMode::normals), x, y);
    near(normals[0], 188, "normal x (0)");
    near(normals[1], 255, "normal y (1)");
    near(normals[2], 188, "normal z (0)");
    let albedo = pixel(&render(&mut r, &mut s, ViewMode::albedo), x, y);
    for c in &albedo[..3] {
        near(*c, 188, "grey albedo");
    }
    let rm = pixel(&render(&mut r, &mut s, ViewMode::roughness_metallic), x, y);
    near(rm[0], 0, "unused red");
    near(rm[1], 231, "roughness 0.8 in green");
    near(rm[2], 0, "metallic 0 in blue");
}

#[test]
fn overdraw_is_brighter_where_surfaces_overlap() {
    let Some(mut r) = renderer() else { return };
    let mut s = scene();
    let image = render(&mut r, &mut s, ViewMode::overdraw);
    let luma = |p: [u8; 4]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
    let ground_only = luma(pixel(&image, GROUND.0, GROUND.1));
    // The cube's center on screen: it covers the ground behind it.
    let cube = project(Vec3::new(-1.0, 0.5, 0.0));
    let over = luma(pixel(&image, cube[0], cube[1]));
    assert!(
        over > ground_only,
        "cube over ground {over} vs ground {ground_only}"
    );
}
