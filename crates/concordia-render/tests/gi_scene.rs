//! The probe GI's scene (M4 4.8b): proxy meshes, what a ray sees of a material, and the instance
//! structure kept across frames.

use concordia_assets::{AssetRef, Material, MeshData, Slab};
use concordia_bvh::Ray;
use concordia_core::math::{Mat4, Vec3};
use concordia_render::gi::{self, GiInstance, GiScene};

/// An n × n grid over `height(x, z)` on a 4 m square.
fn surface(n: u32, height: impl Fn(f32, f32) -> f32) -> MeshData {
    let side = n + 1;
    let positions: Vec<[f32; 3]> = (0..side * side)
        .map(|i| {
            let (x, z) = (
                4.0 * (i % side) as f32 / n as f32 - 2.0,
                4.0 * (i / side) as f32 / n as f32 - 2.0,
            );
            [x, height(x, z), z]
        })
        .collect();
    let mut indices = Vec::new();
    for row in 0..n {
        for col in 0..n {
            let a = row * side + col;
            indices.extend([a, a + side, a + 1, a + 1, a + side, a + side + 1]);
        }
    }
    MeshData {
        normals: vec![[0.0, 1.0, 0.0]; positions.len()],
        aabb: ([-2.0, -1.0, -2.0], [2.0, 1.0, 2.0]),
        positions,
        indices,
        ..Default::default()
    }
}

fn bumps(x: f32, z: f32) -> f32 {
    0.3 * (x * 1.5).sin() * (z * 1.5).cos()
}

#[test]
fn proxy_stays_within_error() {
    let mesh = surface(64, bumps);
    let proxy = gi::proxy(&mesh).unwrap();
    let before = mesh.indices.len() / 3;
    assert!(proxy.len() >= 32, "{} triangles", proxy.len());
    assert!(proxy.len() < before / 4, "{before} → {}", proxy.len());
    // Every proxy corner is on the surface (vertices are kept, not moved), and every triangle's
    // centre is within 1% of the extent (4 m) of it, with slack for the curvature between samples.
    for t in &proxy {
        for v in t.v {
            assert!((v.y - bumps(v.x, v.z)).abs() < 1e-4, "{v}");
        }
        let c = (t.v[0] + t.v[1] + t.v[2]) / 3.0;
        assert!((c.y - bumps(c.x, c.z)).abs() <= 0.02 * 4.0, "{c}");
    }
}

#[test]
fn a_small_mesh_is_its_own_proxy() {
    let mesh = surface(2, |_, _| 0.0);
    assert_eq!(gi::proxy(&mesh).unwrap().len(), 8);
}

#[test]
fn proxy_refuses_broken_meshes() {
    let mut mesh = surface(2, |_, _| 0.0);
    mesh.indices.push(999);
    assert!(gi::proxy(&mesh).is_err());
}

#[test]
fn a_material_shows_its_diffuse_albedo_and_emission() {
    let plain = Material::from_slab(Slab {
        base_color: [0.8, 0.4, 0.2, 1.0],
        metallic: 0.0,
        ..Default::default()
    });
    let look = gi::material(&plain);
    assert!((look.albedo - Vec3::new(0.8, 0.4, 0.2)).length() < 1e-6);
    assert_eq!(look.emission, Vec3::ZERO);
    // A metal reflects nothing diffusely; a glow emits in nits.
    let metal = Material::from_slab(Slab {
        base_color: [0.9, 0.9, 0.9, 1.0],
        metallic: 1.0,
        ..Default::default()
    });
    assert_eq!(gi::material(&metal).albedo, Vec3::ZERO);
    let glow = Material::from_slab(Slab {
        emissive: [1.0, 0.5, 0.25],
        emissive_strength: 2000.0,
        ..Default::default()
    });
    assert!((gi::material(&glow).emission - Vec3::new(2000.0, 1000.0, 500.0)).length() < 1e-3);
}

fn floor_ref() -> AssetRef {
    "builtin:mesh/plane".parse().unwrap()
}

fn mat_ref(name: &str) -> AssetRef {
    format!("builtin:material/{name}")
        .parse()
        .unwrap_or_else(|_| floor_ref())
}

#[test]
fn the_scene_follows_moving_instances_without_a_rebuild() {
    let mut scene = GiScene::default();
    let mesh = surface(4, |_, _| 0.0);
    scene.add_mesh(&floor_ref(), &mesh).unwrap();
    scene.add_material(&mat_ref("default"), &Material::from_slab(Slab::default()));
    let at = |y: f32| GiInstance {
        mesh: floor_ref(),
        material: mat_ref("default"),
        world: Mat4::from_translation(Vec3::new(0.0, y, 0.0)),
    };
    let down = Ray {
        origin: Vec3::new(0.5, 10.0, 0.5),
        dir: Vec3::NEG_Y,
        t_min: 0.0,
        t_max: f32::INFINITY,
    };
    let first = scene.update(&[at(0.0), at(5.0)]);
    assert!(first.rebuilt);
    assert_eq!(scene.accel().intersect(&down).map(|h| h.t), Some(5.0));
    // Same instances, one moved: refit, not rebuilt.
    let second = scene.update(&[at(0.0), at(2.0)]);
    assert!(!second.rebuilt && second.moved == 1, "{second:?}");
    assert_eq!(scene.accel().intersect(&down).map(|h| h.t), Some(8.0));
    // A different set of instances: rebuilt.
    let third = scene.update(&[at(1.0)]);
    assert!(third.rebuilt);
    assert_eq!(scene.accel().intersect(&down).map(|h| h.t), Some(9.0));
    // Each instance's material is known to the GPU by index.
    assert_eq!(scene.instance_materials(), &[0]);
}

#[test]
fn instances_of_unknown_meshes_are_left_out() {
    let mut scene = GiScene::default();
    let ghost = GiInstance {
        mesh: floor_ref(),
        material: mat_ref("default"),
        world: Mat4::IDENTITY,
    };
    let report = scene.update(&[ghost]);
    assert_eq!(report.skipped, 1);
    assert!(scene.accel().instances().is_empty());
}
