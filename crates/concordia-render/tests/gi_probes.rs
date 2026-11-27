//! The probe update on the GPU (M4 4.8b): what probes converge to in scenes with known answers.

mod common;

use concordia_assets::{AssetRef, Material, Slab, shapes};
use concordia_core::math::{Mat4, UVec3, Vec3};
use concordia_render::gi::{GiInputs, GiInstance, GiLight, GiProbes, GiScene, GiSun, ProbeGrid};
use concordia_shaders::{SlangToolchain, SlangcCompiler};

fn mesh_ref(name: &str) -> AssetRef {
    format!("builtin:mesh/{name}").parse().unwrap()
}

fn mat_ref(name: &str) -> AssetRef {
    format!("asset:0193a1c2-7e40-7000-8000-00006d656d6f#material/{name}")
        .parse()
        .unwrap()
}

fn probes(device: &dyn concordia_rhi::Device) -> GiProbes {
    let compiler = SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
    GiProbes::new(device, &compiler).expect("the GI kernels build")
}

/// Six boxes closing the room [-2, 2]³ (walls 0.2 thick), each `material`.
fn room(scene: &mut GiScene, material: &AssetRef) -> Vec<GiInstance> {
    let walls = [
        ([0.0, -2.1, 0.0], [4.4, 0.2, 4.4]),
        ([0.0, 2.1, 0.0], [4.4, 0.2, 4.4]),
        ([-2.1, 0.0, 0.0], [0.2, 4.4, 4.4]),
        ([2.1, 0.0, 0.0], [0.2, 4.4, 4.4]),
        ([0.0, 0.0, -2.1], [4.4, 4.4, 0.2]),
        ([0.0, 0.0, 2.1], [4.4, 4.4, 0.2]),
    ];
    walls
        .iter()
        .enumerate()
        .map(|(i, (at, size))| {
            let mesh = mesh_ref(&format!("wall{i}"));
            scene.add_mesh(&mesh, &shapes::cube(*size)).unwrap();
            GiInstance {
                mesh,
                material: material.clone(),
                world: Mat4::from_translation(Vec3::from(*at)),
            }
        })
        .collect()
}

#[test]
fn gi_furnace_test_no_energy_gain() {
    let Some(device) = common::device() else {
        return;
    };
    // Every wall emits 1 nit and reflects half: the infinite-bounce radiance is 1 / (1 − ½) = 2,
    // so every probe's irradiance, in every direction, converges to π · 2 and never exceeds it.
    let mut scene = GiScene::default();
    let glow = mat_ref("furnace");
    scene.add_material(
        &glow,
        &Material::from_slab(Slab {
            base_color: [0.5, 0.5, 0.5, 1.0],
            metallic: 0.0,
            emissive: [1.0, 1.0, 1.0],
            emissive_strength: 1.0,
            ..Default::default()
        }),
    );
    let instances = room(&mut scene, &glow);
    scene.update(&instances);
    let mut gi = probes(device.as_ref());
    let grid = ProbeGrid {
        origin: Vec3::splat(-1.0),
        counts: UVec3::splat(3),
        spacing: 1.0,
    };
    let expected = std::f32::consts::PI * 2.0;
    let mut low = f32::INFINITY;
    for frame in 0..120 {
        let inputs = GiInputs {
            grid,
            rays: 128,
            frame,
            ..Default::default()
        };
        gi.update(device.as_ref(), &scene, &inputs).unwrap();
        if frame >= 100 {
            for probe in 0..27 {
                for e in gi.irradiance(device.as_ref(), probe).unwrap() {
                    for c in e.to_array() {
                        assert!(c <= expected * 1.02, "energy gained: {c} > {expected}");
                        low = low.min(c);
                    }
                }
            }
        }
    }
    assert!(low >= expected * 0.9, "converged to {low}, not {expected}");
}

#[test]
fn probes_see_a_sunlit_floor_and_its_shadow() {
    let Some(device) = common::device() else {
        return;
    };
    // A white floor (albedo 0.8) under a sun of 1000 lux from straight above; a roof over x < 0.
    let mut scene = GiScene::default();
    let white = mat_ref("white");
    scene.add_material(
        &white,
        &Material::from_slab(Slab {
            base_color: [0.8, 0.8, 0.8, 1.0],
            metallic: 0.0,
            ..Default::default()
        }),
    );
    scene
        .add_mesh(&mesh_ref("floor"), &shapes::cube([200.0, 0.2, 200.0]))
        .unwrap();
    scene
        .add_mesh(&mesh_ref("roof"), &shapes::cube([100.0, 0.2, 200.0]))
        .unwrap();
    let place = |mesh: &str, at: [f32; 3]| GiInstance {
        mesh: mesh_ref(mesh),
        material: white.clone(),
        world: Mat4::from_translation(Vec3::from(at)),
    };
    // The roof's top is lit too, but probes under it see only its dark underside and the floor.
    scene.update(&[
        place("floor", [0.0, -0.1, 0.0]),
        place("roof", [-50.0, 6.0, 0.0]),
    ]);
    let mut gi = probes(device.as_ref());
    // A row of probes 1 m above the floor, 4 m apart: the first under the roof (x = -4), the
    // last in the sun far from it (x = 56), so its upper half sees only sky. (Spacing matters:
    // relocation keeps probes 0.2 spacings from surfaces.)
    let grid = ProbeGrid {
        origin: Vec3::new(-4.0, 1.0, 0.0),
        counts: UVec3::new(16, 1, 1),
        spacing: 4.0,
    };
    for frame in 0..60 {
        let inputs = GiInputs {
            grid,
            rays: 256,
            frame,
            sun: Some(GiSun {
                direction: Vec3::NEG_Y,
                illuminance: Vec3::splat(1000.0),
            }),
            ..Default::default()
        };
        gi.update(device.as_ref(), &scene, &inputs).unwrap();
    }
    // The texel looking straight down sees the lit floor: E = π · (ρ/π · E_sun) = 800 lux.
    // Looking up it sees the black sky: the texel nearest +Y leans 8° off it, so its lobe catches
    // a sliver of floor at the horizon, about (1 − cos 8°) / 2 ≈ 0.5% of 800.
    let down = concordia_render::gi::octa::texel_of(Vec3::NEG_Y, 8);
    let up = concordia_render::gi::octa::texel_of(Vec3::Y, 8);
    let texel = |e: &[Vec3], t: concordia_core::math::UVec2| e[(t.y * 8 + t.x) as usize];
    let sunny = gi.irradiance(device.as_ref(), 15).unwrap();
    let shaded = gi.irradiance(device.as_ref(), 0).unwrap();
    let lit = texel(&sunny, down).x;
    assert!((lit - 800.0).abs() < 80.0, "sunlit floor: {lit}");
    let sky = texel(&sunny, up).x;
    assert!(sky < 0.02 * lit, "the sky is black: {sky}");
    let dark = texel(&shaded, down).x;
    assert!(
        dark < 0.3 * lit,
        "the roof shades the floor: {dark} vs {lit}"
    );
}

/// A spell light appearing over a dark floor: probes near it reach 90% of their converged
/// irradiance within 4 updates (the light event), and with the light steady they hold within
/// ±1% (`adaptive_hysteresis_reacts_within_4_frames_to_spell_light`, the GPU half). Open ground,
/// not a closed room: each update adds one bounce, so in a room of 70% walls (converged ≈ 3.3 ×
/// the first bounce) no update scheme reaches 90% in four.
#[test]
fn adaptive_hysteresis_reacts_within_4_frames_to_spell_light() {
    let Some(device) = common::device() else {
        return;
    };
    let mut scene = GiScene::default();
    let white = mat_ref("white");
    scene.add_material(
        &white,
        &Material::from_slab(Slab {
            base_color: [0.7, 0.7, 0.7, 1.0],
            metallic: 0.0,
            ..Default::default()
        }),
    );
    scene
        .add_mesh(&mesh_ref("ground"), &shapes::cube([40.0, 0.2, 40.0]))
        .unwrap();
    scene.update(&[GiInstance {
        mesh: mesh_ref("ground"),
        material: white.clone(),
        world: Mat4::from_translation(Vec3::new(0.0, -1.1, 0.0)),
    }]);
    let mut gi = probes(device.as_ref());
    let grid = ProbeGrid {
        origin: Vec3::new(-1.0, 0.0, -1.0),
        counts: UVec3::new(3, 1, 3),
        spacing: 1.0,
    };
    let light = GiLight {
        position: Vec3::new(0.5, 0.5, 0.5),
        range: 10.0,
        direction: Vec3::NEG_Y,
        intensity: Vec3::splat(200.0),
        spot: (0.0, 1.0),
        shadowed: false,
    };
    let inputs = |frame: u32, lit: bool, event: bool| GiInputs {
        grid,
        frame,
        lights: if lit { vec![light] } else { Vec::new() },
        light_events: if event {
            vec![light.position]
        } else {
            Vec::new()
        },
        ..Default::default()
    };
    // Dark and settled.
    for f in 0..20 {
        gi.update(device.as_ref(), &scene, &inputs(f, false, false))
            .unwrap();
    }
    // The probe at the centre, its mean irradiance over all texels.
    let centre = 4;
    let mean = |gi: &GiProbes| {
        let e = gi.irradiance(device.as_ref(), centre).unwrap();
        e.iter().map(|c| c.x).sum::<f32>() / e.len() as f32
    };
    let mut after = Vec::new();
    for f in 20..24 {
        gi.update(device.as_ref(), &scene, &inputs(f, true, f == 20))
            .unwrap();
        after.push(mean(&gi));
    }
    // Converge with the light steady, then watch it hold.
    for f in 24..160 {
        gi.update(device.as_ref(), &scene, &inputs(f, true, false))
            .unwrap();
    }
    let converged = mean(&gi);
    let mut drift = 0.0f32;
    for f in 160..200 {
        gi.update(device.as_ref(), &scene, &inputs(f, true, false))
            .unwrap();
        drift = drift.max((mean(&gi) / converged - 1.0).abs());
    }
    println!("after the light: {after:?}; converged {converged}; drift {drift}");
    assert!(
        after[3] >= 0.9 * converged,
        "90% within 4 updates: {after:?} of {converged}"
    );
    assert!(drift <= 0.01, "steady within ±1%: {drift}");
}
