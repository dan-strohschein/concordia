//! Light transport against independent answers, on every backend (M4 4.1b2 CPU, 4.1b3 GPU): the
//! white furnace, a disk light's irradiance, the engine's own point, spot and sun definitions,
//! convergence and determinism; and the two backends against each other.

mod common;

use common::backends;

use std::f32::consts::PI;

use concordia_assets::{Material, MeshData, Slab};
use concordia_core::math::{Mat4, Vec3};
use concordia_material::MaterialProgram;
use concordia_pathtrace::{Backend, EnvMap, HdrImage, SceneBuilder, Settings, render, render_cpu};
use concordia_render_types::{
    Camera, DirectionalLight, PointLight, Projection, SpotLight, Tonemap, candela, distance_window,
};

/// EV100 at which `Camera::exposure()` is 1, so pixels are radiance.
fn unit_exposure() -> f32 {
    (1.0f32 / 1.2).log2()
}

/// An orthographic camera `height` wide looking at `target` from direction `from` (unit).
fn ortho(target: Vec3, from: Vec3, height: f32) -> (Mat4, Camera) {
    let eye = target + from * 5.0;
    let up = if from.y.abs() > 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let forward = (target - eye).normalize();
    let right = forward.cross(up).normalize();
    let world = Mat4::from_cols(
        right.extend(0.0),
        right.cross(forward).extend(0.0),
        (-forward).extend(0.0),
        eye.extend(1.0),
    );
    let camera = Camera {
        projection: Projection::Orthographic {
            height,
            near: 0.01,
            far: 100.0,
        },
        exposure_ev100: unit_exposure(),
        tonemap: Tonemap::Linear,
        ..Camera::default()
    };
    (world, camera)
}

/// A square in the XZ plane facing +Y, `size` wide, centred at the origin.
fn quad(size: f32) -> MeshData {
    let h = size / 2.0;
    MeshData {
        positions: vec![[-h, 0.0, -h], [h, 0.0, -h], [h, 0.0, h], [-h, 0.0, h]],
        normals: vec![[0.0, 1.0, 0.0]; 4],
        indices: vec![0, 2, 1, 0, 3, 2],
        ..MeshData::default()
    }
}

/// A disk of radius `r` in the XZ plane facing −Y, as a fan of `n` triangles.
fn disk(r: f32, n: u32) -> MeshData {
    let mut positions = vec![[0.0, 0.0, 0.0]];
    for i in 0..n {
        let a = i as f32 / n as f32 * 2.0 * PI;
        positions.push([r * a.cos(), 0.0, r * a.sin()]);
    }
    let indices = (0..n).flat_map(|i| [0, 1 + i, 1 + (i + 1) % n]).collect();
    MeshData {
        normals: vec![[0.0, -1.0, 0.0]; positions.len()],
        positions,
        indices,
        ..MeshData::default()
    }
}

fn material(slab: Slab) -> Material {
    Material::from_slab(slab)
}

fn white_lambert() -> Slab {
    Slab {
        base_color: [1.0, 1.0, 1.0, 1.0],
        specular: 0.0,
        roughness: 1.0,
        ..Slab::default()
    }
}

fn mean(image: &HdrImage) -> Vec3 {
    let sum: Vec3 = image.pixels.iter().map(|p| Vec3::from_array(*p)).sum();
    sum / image.pixels.len() as f32
}

fn settings(size: u32, spp: u32, max_bounces: u32) -> Settings {
    Settings {
        width: size,
        height: size,
        spp,
        seed: 7,
        max_bounces,
        threads: None,
    }
}

/// A large quad of `slab` in a uniform white sky, seen at cosine `mu`.
fn furnace(backend: &Backend, slab: Slab, mu: f32) -> Vec3 {
    let mut b = SceneBuilder::new();
    let mesh = b.mesh(&quad(1000.0)).unwrap();
    let mat = b.material(&material(slab)).unwrap();
    b.instance(mesh, Mat4::IDENTITY, mat);
    b.environment(EnvMap::uniform([1.0; 3], 1.0));
    let from = Vec3::new((1.0 - mu * mu).sqrt(), mu, 0.0);
    let (world, camera) = ortho(Vec3::ZERO, from, 0.5);
    b.camera(world, &camera);
    let scene = b.build().unwrap();
    mean(&render(&scene, &settings(16, 1024, 64), backend).unwrap())
}

#[test]
fn pathtrace_white_furnace_conserves_energy() {
    for (name, backend) in backends() {
        for mu in [0.9, 0.5] {
            white_furnace(name, &backend, mu);
        }
    }
}

fn white_furnace(name: &str, backend: &Backend, mu: f32) {
    {
        let lambert = furnace(backend, white_lambert(), mu);
        for c in lambert.to_array() {
            assert!(
                (c - 1.0).abs() <= 0.005,
                "{name}: Lambert at mu {mu}: {lambert}"
            );
        }
        let rough_metal = Slab {
            metallic: 1.0,
            roughness: 0.8,
            ..white_lambert()
        };
        let [a, b] = concordia_material::dfg::direct_albedo(mu, 0.8);
        let expected = a + b;
        let got = furnace(backend, rough_metal, mu);
        for c in got.to_array() {
            assert!(
                (c - expected).abs() <= 0.03 * expected,
                "{name}: rough GGX at mu {mu}: {got} vs directional albedo {expected}"
            );
        }
    }
}

/// The view direction from the shaded point toward the camera in the disk and light tests.
fn view() -> Vec3 {
    Vec3::new(0.9, 0.436, 0.0).normalize()
}

/// Radiance toward `v` from a point at the origin (normal +Y) lit by a disk of radiance 1,
/// radius `r`, `h` above, by midpoint quadrature over the disk; and the cosine-only integral.
fn disk_quadrature(program: &MaterialProgram, r: f32, h: f32) -> (Vec3, f32) {
    let (rings, segments) = (400, 800);
    let (mut radiance, mut cosines) = (Vec3::ZERO, 0.0f64);
    for i in 0..rings {
        let (r0, r1) = (
            r * i as f32 / rings as f32,
            r * (i + 1) as f32 / rings as f32,
        );
        let rm = (r0 + r1) / 2.0;
        let area = PI * (r1 * r1 - r0 * r0) / segments as f32;
        for j in 0..segments {
            let a = (j as f32 + 0.5) / segments as f32 * 2.0 * PI;
            let to = Vec3::new(rm * a.cos(), h, rm * a.sin());
            let d2 = to.length_squared();
            let l = to / d2.sqrt();
            let cos_light = l.y;
            let solid = area * cos_light / d2;
            radiance += program.eval(Vec3::Y, view(), l).color * solid;
            cosines += f64::from(l.y * solid);
        }
    }
    (radiance, cosines as f32)
}

#[test]
fn pathtrace_matches_analytic_irradiance_from_disk_light() {
    let (r, h) = (0.5f32, 1.0f32);
    let plane = material(white_lambert());
    let program = MaterialProgram::compile(&plane).unwrap();
    let (expected, cosines) = disk_quadrature(&program, r, h);
    let analytic = PI * r * r / (h * h + r * r);
    assert!(
        (cosines - analytic).abs() <= 1e-3 * analytic,
        "quadrature {cosines} vs closed form {analytic}"
    );

    let mut b = SceneBuilder::new();
    let ground = b.mesh(&quad(100.0)).unwrap();
    let lamp = b.mesh(&disk(r, 256)).unwrap();
    let ground_mat = b.material(&plane).unwrap();
    let emitter = b
        .material(&material(Slab {
            base_color: [0.0, 0.0, 0.0, 1.0],
            specular: 0.0,
            emissive: [1.0; 3],
            ..Slab::default()
        }))
        .unwrap();
    b.instance(ground, Mat4::IDENTITY, ground_mat);
    b.instance(
        lamp,
        Mat4::from_translation(Vec3::new(0.0, h, 0.0)),
        emitter,
    );
    let (world, camera) = ortho(Vec3::ZERO, view(), 0.01);
    b.camera(world, &camera);
    let scene = b.build().unwrap();
    for (name, backend) in backends() {
        let got = mean(&render(&scene, &settings(4, 4096, 1), &backend).unwrap());
        for (g, e) in got.to_array().into_iter().zip(expected.to_array()) {
            assert!(
                (g - e).abs() <= 0.01 * e,
                "{name}: {got} vs quadrature {expected}"
            );
        }
    }
}

/// A white plane lit only by what `add` puts in the scene, seen at `target`.
fn direct_only(backend: &Backend, target: Vec3, add: impl FnOnce(&mut SceneBuilder)) -> Vec3 {
    let mut b = SceneBuilder::new();
    let ground = b.mesh(&quad(100.0)).unwrap();
    let mat = b.material(&material(white_lambert())).unwrap();
    b.instance(ground, Mat4::IDENTITY, mat);
    add(&mut b);
    let (world, camera) = ortho(target, view(), 0.001);
    b.camera(world, &camera);
    mean(&render(&b.build().unwrap(), &settings(2, 4, 1), backend).unwrap())
}

fn close(got: Vec3, want: Vec3, what: &str) {
    for (g, w) in got.to_array().into_iter().zip(want.to_array()) {
        assert!(
            (g - w).abs() <= 1e-3 * w.max(1e-6),
            "{what}: {got} vs {want}"
        );
    }
}

#[test]
fn punctual_lights_follow_the_engines_definitions() {
    for (name, backend) in backends() {
        punctual_lights(name, &backend);
    }
}

fn punctual_lights(name: &str, backend: &Backend) {
    let program = MaterialProgram::compile(&material(white_lambert())).unwrap();
    let f = |p: Vec3, l: Vec3| program.eval(Vec3::Y, view(), (l - p).normalize()).color;

    let light = PointLight {
        color: [1.0, 0.5, 0.25],
        intensity_lumens: 800.0,
        range: 10.0,
        ..Default::default()
    };
    let at = Vec3::new(0.0, 2.0, 0.0);
    let got = direct_only(backend, Vec3::ZERO, |b| b.point_light(at, &light));
    let want = f(Vec3::ZERO, at)
        * Vec3::from_array(light.color)
        * candela(800.0)
        * distance_window(4.0, 10.0);
    close(got, want, &format!("{name}: point light"));

    let spot = SpotLight {
        color: [1.0; 3],
        intensity_lumens: 800.0,
        range: 10.0,
        inner_angle: 0.3,
        outer_angle: 0.5,
        ..Default::default()
    };
    let p = Vec3::new(0.8, 0.0, 0.0);
    let got = direct_only(backend, p, |b| b.spot_light(at, Vec3::NEG_Y, &spot));
    let cone = concordia_render_types::SpotCone::new(0.3, 0.5);
    let to = at - p;
    let cos_axis = (-to.normalize()).dot(Vec3::NEG_Y);
    let want = f(p, at)
        * candela(800.0)
        * distance_window(to.length_squared(), 10.0)
        * cone.attenuation(cos_axis);
    assert!(
        cone.attenuation(cos_axis) > 0.0 && cone.attenuation(cos_axis) < 1.0,
        "inside the penumbra"
    );
    close(got, want, &format!("{name}: spot light"));

    let sun = DirectionalLight {
        color: [1.0, 0.9, 0.8],
        illuminance_lux: 1000.0,
        shadows: None,
    };
    let travel = Vec3::new(0.0, -1.0, -0.3).normalize();
    let got = direct_only(backend, Vec3::ZERO, |b| b.sun(travel, &sun));
    let want = program.eval(Vec3::Y, view(), -travel).color * Vec3::from_array(sun.color) * 1000.0;
    close(got, want, &format!("{name}: sun"));
}

/// Emission is luminance in nits: exposure scales it like any light (ADR-0102).
#[test]
fn emission_is_luminance_scaled_by_exposure() {
    let scene = |ev100: f32| {
        let mut b = SceneBuilder::new();
        let panel = b.mesh(&quad(10.0)).unwrap();
        let glow = b
            .material(&material(Slab {
                base_color: [0.0, 0.0, 0.0, 1.0],
                specular: 0.0,
                emissive: [0.6, 0.3, 0.15],
                ..Slab::default()
            }))
            .unwrap();
        b.instance(panel, Mat4::IDENTITY, glow);
        let (world, mut camera) = ortho(Vec3::ZERO, Vec3::Y, 1.0);
        camera.exposure_ev100 = ev100;
        b.camera(world, &camera);
        b.build().unwrap()
    };
    for (name, backend) in backends() {
        let draw = |ev| mean(&render(&scene(ev), &settings(2, 4, 1), &backend).unwrap());
        close(
            draw(0.0),
            Vec3::new(0.5, 0.25, 0.125),
            &format!("{name}: nits × 1/1.2 at EV 0"),
        );
        close(
            draw(1.0),
            Vec3::new(0.25, 0.125, 0.0625),
            &format!("{name}: EV 1 halves it"),
        );
    }
}

/// A sphere on a plane under a sky and a sun, 8×8, for the convergence and determinism tests.
fn small_scene() -> concordia_pathtrace::Scene {
    let mut b = SceneBuilder::new();
    let ground = b.mesh(&quad(20.0)).unwrap();
    let ball = match concordia_assets::Builtin::of(&"builtin:mesh/sphere".parse().unwrap()) {
        Some(concordia_assets::BuiltinAsset::Mesh(m)) => b.mesh(&m).unwrap(),
        _ => panic!("the built-in sphere"),
    };
    let grey = b.material(&material(Slab::default())).unwrap();
    b.instance(ground, Mat4::IDENTITY, grey);
    b.instance(ball, Mat4::from_translation(Vec3::new(0.0, 0.5, 0.0)), grey);
    b.environment(EnvMap::uniform([0.3, 0.4, 0.6], 1.0));
    b.sun(
        Vec3::new(-0.4, -1.0, -0.2).normalize(),
        &DirectionalLight {
            illuminance_lux: 2.0,
            ..DirectionalLight::default()
        },
    );
    let (world, camera) = ortho(
        Vec3::new(0.0, 0.4, 0.0),
        Vec3::new(0.6, 0.6, 0.5).normalize(),
        2.0,
    );
    b.camera(world, &camera);
    b.build().unwrap()
}

fn rmse(a: &HdrImage, b: &HdrImage) -> f64 {
    let sum: f64 = a
        .pixels
        .iter()
        .zip(&b.pixels)
        .flat_map(|(x, y)| (0..3).map(move |c| f64::from(x[c] - y[c]).powi(2)))
        .sum();
    (sum / (a.pixels.len() * 3) as f64).sqrt()
}

#[test]
fn pathtrace_converges_at_expected_rate() {
    let scene = small_scene();
    for (name, backend) in backends() {
        let draw = |seed, spp| {
            render(
                &scene,
                &Settings {
                    seed,
                    ..settings(8, spp, 8)
                },
                &backend,
            )
            .unwrap()
        };
        let reference = draw(1, 16384);
        let points: Vec<(f64, f64)> = [16, 64, 256, 1024]
            .into_iter()
            .map(|spp| (f64::from(spp).ln(), rmse(&draw(2, spp), &reference).ln()))
            .collect();
        let n = points.len() as f64;
        let (sx, sy) = points
            .iter()
            .fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
        let (mx, my) = (sx / n, sy / n);
        let slope = points.iter().map(|(x, y)| (x - mx) * (y - my)).sum::<f64>()
            / points.iter().map(|(x, _)| (x - mx).powi(2)).sum::<f64>();
        assert!(
            (-0.6..=-0.4).contains(&slope),
            "{name}: log-log slope {slope} (1/√spp is −0.5): {points:?}"
        );
    }
}

#[test]
fn pathtrace_is_deterministic_for_fixed_seed() {
    let scene = small_scene();
    for (name, backend) in backends() {
        let run = |seed| {
            render(
                &scene,
                &Settings {
                    seed,
                    ..settings(8, 16, 8)
                },
                &backend,
            )
            .unwrap()
        };
        let one = run(3);
        assert_eq!(one.pixels, run(3).pixels, "{name}: same seed, same image");
        assert_ne!(
            one.pixels,
            run(4).pixels,
            "{name}: another seed, another image"
        );
    }
    let threads = |n| {
        render_cpu(
            &scene,
            &Settings {
                seed: 3,
                threads: Some(n),
                ..settings(8, 16, 8)
            },
        )
        .unwrap()
    };
    assert_eq!(
        threads(1).pixels,
        threads(8).pixels,
        "cpu: identical at 1 and 8 threads"
    );
}

/// The GPU and CPU tracers draw the same picture: they differ from each other no more than two
/// CPU images with different seeds do, and their means agree to 1%.
#[test]
fn gpu_and_cpu_tracers_converge() {
    let scene = small_scene();
    let Some((_, gpu)) = backends().into_iter().find(|(n, _)| *n == "gpu") else {
        return;
    };
    let draw = |seed, backend: &Backend| {
        render(
            &scene,
            &Settings {
                seed,
                ..settings(8, 4096, 8)
            },
            backend,
        )
        .unwrap()
    };
    let (cpu_a, cpu_b, gpu_a) = (
        draw(1, &Backend::Cpu),
        draw(2, &Backend::Cpu),
        draw(3, &gpu),
    );
    let noise = rmse(&cpu_a, &cpu_b);
    let apart = rmse(&gpu_a, &cpu_a);
    assert!(
        apart <= 1.5 * noise,
        "gpu vs cpu rmse {apart} against the noise {noise}"
    );
    let (m_gpu, m_cpu) = (mean(&gpu_a), mean(&cpu_a));
    for (g, c) in m_gpu.to_array().into_iter().zip(m_cpu.to_array()) {
        assert!((g - c).abs() <= 0.01 * c, "means {m_gpu} vs {m_cpu}");
    }
}

/// With no GPU the default backend is the CPU, and it renders.
#[test]
fn auto_falls_back_to_the_cpu_without_a_device() {
    let backend = Backend::auto(None);
    assert!(matches!(backend, Backend::Cpu), "{backend:?}");
    let image = render(&small_scene(), &settings(4, 2, 2), &backend).unwrap();
    assert!(mean(&image).max_element() > 0.0);
}

/// Where camera rays miss everything the renderer shows the skybox, or the camera's clear color
/// when the sky isn't drawn (both display-referred, like the renderer's).
#[test]
fn camera_misses_show_the_skybox_or_the_clear_color() {
    let scene = |visible: bool| {
        let mut b = SceneBuilder::new();
        let sky = EnvMap::uniform([2.0, 2.0, 2.0], 1.0);
        b.environment_with_background(sky, visible);
        let (world, mut camera) = ortho(Vec3::ZERO, Vec3::Y, 1.0);
        camera.clear = [0.1, 0.2, 0.3, 1.0];
        b.camera(world, &camera);
        b.build().unwrap()
    };
    for (name, backend) in backends() {
        let draw = |s| mean(&render(&s, &settings(2, 1, 1), &backend).unwrap());
        close(
            draw(scene(true)),
            Vec3::splat(2.0),
            &format!("{name}: the skybox"),
        );
        close(
            draw(scene(false)),
            Vec3::new(0.1, 0.2, 0.3),
            &format!("{name}: the clear color"),
        );
    }
}

/// Layered, mixed and covered materials (the GPU's program walker) shade as the CPU's
/// `MaterialProgram::eval` does: under a sun alone (no noise) the two backends agree.
#[test]
fn gpu_and_cpu_agree_on_layered_materials() {
    use concordia_assets::{AlphaMode, Fuzz, Haze, MaterialNode, ThinFilm};
    use std::collections::BTreeMap;
    let Some((_, gpu)) = backends().into_iter().find(|(n, _)| *n == "gpu") else {
        return;
    };
    let slab = |s: Slab| MaterialNode::Slab(s);
    let paint = Slab {
        base_color: [0.7, 0.1, 0.05, 1.0],
        roughness: 0.4,
        ..Slab::default()
    };
    let coat = Slab {
        base_color: [1.0; 4],
        roughness: 0.05,
        transmittance: [0.9, 0.9, 0.9],
        thin_film: Some(ThinFilm {
            thickness_nm: 380.0,
            ior: 1.45,
        }),
        ..Slab::default()
    };
    let cloth = Slab {
        fuzz: Some(Fuzz {
            color: [0.6, 0.6, 0.9],
            roughness: 0.5,
        }),
        haze: Some(Haze {
            roughness: 0.6,
            weight: 0.4,
        }),
        // Partly transparent: blending covers it by 0.6 (opaque materials ignore slab alpha).
        base_color: [0.7, 0.1, 0.05, 0.6],
        ..paint.clone()
    };
    let nodes = BTreeMap::from([
        ("paint".to_owned(), slab(paint)),
        ("coat".to_owned(), slab(coat)),
        ("cloth".to_owned(), slab(cloth)),
        (
            "coated".to_owned(),
            MaterialNode::Vertical {
                top: "coat".into(),
                bottom: "paint".into(),
            },
        ),
        (
            "mixed".to_owned(),
            MaterialNode::Horizontal {
                a: "coated".into(),
                b: "cloth".into(),
                mix: 0.35,
                mix_texture: None,
            },
        ),
        (
            "surface".to_owned(),
            MaterialNode::Coverage {
                node: "mixed".into(),
                coverage: 0.8,
            },
        ),
    ]);
    for alpha_mode in [AlphaMode::Opaque, AlphaMode::Blend] {
        let layered = Material {
            nodes: nodes.clone(),
            root: "surface".into(),
            alpha_mode,
            ..Material::default()
        };
        let mut b = SceneBuilder::new();
        let ground = b.mesh(&quad(100.0)).unwrap();
        let mat = b.material(&layered).unwrap();
        b.instance(ground, Mat4::IDENTITY, mat);
        b.sun(
            Vec3::new(-0.3, -1.0, 0.2).normalize(),
            &DirectionalLight {
                illuminance_lux: 10.0,
                ..DirectionalLight::default()
            },
        );
        let (world, camera) = ortho(Vec3::ZERO, view(), 0.001);
        b.camera(world, &camera);
        let scene = b.build().unwrap();
        let s = Settings {
            max_bounces: 1,
            ..settings(2, 1, 1)
        };
        let (cpu, gpu_image) = (
            mean(&render(&scene, &s, &Backend::Cpu).unwrap()),
            mean(&render(&scene, &s, &gpu).unwrap()),
        );
        assert!(cpu.max_element() > 0.0, "{alpha_mode:?}: lit");
        for (g, c) in gpu_image.to_array().into_iter().zip(cpu.to_array()) {
            assert!(
                (g - c).abs() <= 2e-3 * c.max(1e-3),
                "{alpha_mode:?}: gpu {gpu_image} vs cpu {cpu}"
            );
        }
    }
}
