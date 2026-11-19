//! Image-based lighting and the skybox on the GPU (M1 phase 1.6). Skips without a GPU unless
//! CONCORDIA_REQUIRE_GPU=1.

mod common;

use common::{Scene, golden, pixel, renderer};
use concordia_assets::{AssetRef, ImageSettings, MaterialData};
use concordia_core::math::{Quat, Vec3};
use concordia_render::shadow::CascadeSettings;
use concordia_render::{
    Camera, DirectionalLight, Environment, Msaa, RenderError, RenderRequest, RenderWorld,
};

fn sky(s: &mut Scene) -> AssetRef {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/hdri/kloofendal_48d_partly_cloudy_puresky_512.hdr"
    ))
    .unwrap();
    let texture = concordia_assets::decode_image(&bytes, &ImageSettings::default()).unwrap();
    s.assets.insert_texture("sky", texture)
}

fn environment(s: &mut Scene, skybox: bool) {
    let map = sky(s);
    let id = s.entity(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    s.world
        .insert(
            id,
            Environment {
                map,
                intensity: 1.0,
                skybox,
            },
        )
        .unwrap();
}

#[test]
fn pbr_spheres_with_ibl_golden() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    environment(&mut s, true);
    for (i, (metallic, roughness)) in [
        (0.0, 0.1),
        (0.0, 0.5),
        (0.0, 1.0),
        (1.0, 0.05),
        (1.0, 0.35),
        (1.0, 0.8),
    ]
    .into_iter()
    .enumerate()
    {
        let m = s.material(
            &format!("m{i}"),
            MaterialData {
                base_color: [0.95, 0.64, 0.54, 1.0],
                metallic,
                roughness,
                ..MaterialData::default()
            },
        );
        let at = Vec3::new(
            (i % 3) as f32 * 2.2 - 2.2,
            if i < 3 { 1.1 } else { -1.1 },
            0.0,
        );
        s.mesh("sphere", m, at, Quat::IDENTITY, Vec3::ONE);
    }
    s.camera(
        Vec3::new(0.0, 0.5, 7.0),
        Vec3::new(0.0, 0.3, 0.0),
        Camera {
            exposure_ev100: 0.0,
            ..Camera::default()
        },
    );
    let image = s.render(&mut r, 384, 256);
    let top = pixel(&image, 192, 4);
    assert!(top[2] > 60, "the skybox fills the background: {top:?}");
    golden("pbr_spheres_ibl", &image, 0.01);
}

#[test]
fn ibl_lights_the_side_the_sun_does_not_reach() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    environment(&mut s, false);
    let m = s.material(
        "white",
        MaterialData {
            base_color: [0.8; 4],
            metallic: 0.0,
            roughness: 0.8,
            ..MaterialData::default()
        },
    );
    s.mesh("sphere", m, Vec3::ZERO, Quat::IDENTITY, Vec3::splat(2.0));
    let sun = s.entity(
        Vec3::ZERO,
        Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::new(1.0, -0.2, 0.0).normalize()),
        Vec3::ONE,
    );
    s.world
        .insert(
            sun,
            DirectionalLight {
                color: [1.0; 3],
                illuminance_lux: 3.0,
                shadows: Some(CascadeSettings::default()),
            },
        )
        .unwrap();
    s.camera(
        Vec3::new(0.0, 0.0, 6.0),
        Vec3::ZERO,
        Camera {
            exposure_ev100: 0.0,
            msaa: Msaa::Off,
            ..Camera::default()
        },
    );
    let image = s.render(&mut r, 128, 128);
    // The sphere spans about ±18 px around the center; the sun comes from the left.
    let (sunny, shaded) = (pixel(&image, 52, 64), pixel(&image, 76, 64));
    assert!(
        shaded[0] > 25,
        "sky light reaches the side facing away from the sun: {shaded:?}"
    );
    assert!(
        sunny[0] > shaded[0],
        "the sunlit side stays brighter: {sunny:?} vs {shaded:?}"
    );
}

#[test]
fn environment_with_a_missing_map_names_it() {
    let Some(mut r) = renderer() else { return };
    let mut s = Scene::new();
    let ghost: AssetRef = "asset:0193a1c2-7e40-7000-8000-0000000051e7"
        .parse()
        .unwrap();
    let id = s.entity(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    s.world
        .insert(
            id,
            Environment {
                map: ghost,
                intensity: 1.0,
                skybox: true,
            },
        )
        .unwrap();
    s.camera(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, Camera::default());
    let rw = RenderWorld::extract(&s.world);
    let err = r
        .render(
            &rw,
            &mut s.assets,
            &RenderRequest {
                camera: None,
                width: 8,
                height: 8,
            },
        )
        .unwrap_err();
    assert!(
        matches!(err, RenderError::Asset { .. }) && err.to_string().contains("0000000051e7"),
        "{err}"
    );
}
