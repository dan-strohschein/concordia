//! Probe GI in the renderer (M4 4.8b): `[render] gi = "probes"` lights what only bounced light
//! reaches.

mod common;

use common::{M1, Scene};
use concordia_core::math::{Quat, Vec3};
use concordia_render::gi::{Gi, GiSettings, GiTechnique};
use concordia_render::{Camera, Image, Msaa, SpotLight};

const SIZE: (u32, u32) = (240, 160);

fn white() -> M1 {
    M1 {
        base_color: [0.8, 0.8, 0.8, 1.0],
        metallic: 0.0,
        roughness: 0.9,
        ..Default::default()
    }
}

/// A 5 m room open toward the camera, lit only by a spot shining straight down on the floor.
fn room() -> Scene {
    let mut scene = Scene::new();
    let w = scene.material("white", white());
    for (at, size) in [
        ([0.0, -0.1, 0.0], [5.4, 0.2, 5.4]),
        ([0.0, 5.1, 0.0], [5.4, 0.2, 5.4]),
        ([0.0, 2.5, -2.6], [5.4, 5.4, 0.2]),
        ([-2.6, 2.5, 0.0], [0.2, 5.4, 5.4]),
        ([2.6, 2.5, 0.0], [0.2, 5.4, 5.4]),
    ] {
        scene.mesh(
            "cube",
            w.clone(),
            Vec3::from(at),
            Quat::IDENTITY,
            Vec3::from(size),
        );
    }
    let lamp = scene.entity(
        Vec3::new(0.0, 4.8, 0.0),
        Quat::from_rotation_arc(Vec3::NEG_Z, Vec3::NEG_Y),
        Vec3::ONE,
    );
    scene
        .world
        .insert(
            lamp,
            SpotLight {
                intensity_lumens: 4000.0,
                range: 12.0,
                inner_angle: 0.4,
                outer_angle: 0.6,
                ..SpotLight::default()
            },
        )
        .unwrap();
    scene.camera(
        Vec3::new(0.0, 2.5, 9.0),
        Vec3::new(0.0, 2.5, 0.0),
        // EV 2: the floor (about 3 nits) and the bounce-lit ceiling (about 1 nit) both read well.
        Camera {
            exposure_ev100: 2.0,
            msaa: Msaa::Off,
            ..Camera::default()
        },
    );
    scene
}

/// Mean luma (0..255) of rows `rows` of `image`.
fn band(image: &Image, rows: std::ops::Range<u32>) -> f32 {
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in rows {
        for x in image.width / 4..image.width * 3 / 4 {
            let p = ((y * image.width + x) * 4) as usize;
            let c = &image.rgba[p..p + 3];
            sum += 0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32;
            n += 1.0;
        }
    }
    sum / n
}

fn probes() -> Gi {
    Gi {
        technique: GiTechnique::Probes,
        settings: GiSettings {
            probes: [6, 6, 6],
            spacing: 0.9,
            center: Some([0.0, 2.5, 0.0]),
            ..GiSettings::default()
        },
    }
}

#[test]
fn gi_lights_what_only_bounced_light_reaches() {
    let Some(mut renderer) = common::renderer() else {
        return;
    };
    let mut scene = room();
    let dark = scene.render(&mut renderer, SIZE.0, SIZE.1);
    renderer.set_gi(probes()).unwrap();
    let mut lit = dark.clone();
    for _ in 0..40 {
        lit = scene.render(&mut renderer, SIZE.0, SIZE.1);
    }
    // The ceiling (the image's top band) sees no direct light; with GI the floor's bounce lights it.
    let (before, after) = (band(&dark, 8..28), band(&lit, 8..28));
    println!("ceiling luma: without GI {before:.1}, with GI {after:.1}");
    assert!(after > 3.0 * before + 8.0, "ceiling {before} → {after}");
    // Turning GI off again restores the image without it.
    renderer.set_gi(Gi::default()).unwrap();
    let off = scene.render(&mut renderer, SIZE.0, SIZE.1);
    assert_eq!(off.rgba, dark.rgba);
}

#[test]
fn gi_settings_refuse_out_of_range_values() {
    let Some(mut renderer) = common::renderer() else {
        return;
    };
    for bad in [
        GiSettings {
            rays: 0,
            ..GiSettings::default()
        },
        GiSettings {
            probes: [0, 8, 8],
            ..GiSettings::default()
        },
        GiSettings {
            spacing: -1.0,
            ..GiSettings::default()
        },
    ] {
        let gi = Gi {
            technique: GiTechnique::Probes,
            settings: bad,
        };
        assert!(renderer.set_gi(gi).is_err(), "{bad:?}");
    }
}
