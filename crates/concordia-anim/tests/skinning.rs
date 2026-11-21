//! Linear blend skinning on the CPU, the reference for the GPU (M2 2.4.2): hand-computed vertices,
//! unit normals, the rest pose, palettes in the bind mesh's space, bounds and pose blending.

mod common;

use common::{Project, close, joint};
use concordia_anim::{
    AnimLibrary, BoundClip, Pose, Skeleton, bind_mesh, joint_bounds, model_matrix, skin_vertices,
    skinned_bounds,
};
use concordia_assets::{AssetKind, ImportReport, MeshData, MeshRenderer, Skinned};
use concordia_core::id::DeterministicIdGen;
use concordia_core::math::{Mat4, Quat, Vec3};
use concordia_reflect::TypeRegistry;
use concordia_terra::World;
use std::f32::consts::FRAC_PI_2;

/// One vertex: position, normal, tangent, joints and weights.
type Vertex = ([f32; 3], [f32; 3], [f32; 4], [u16; 4], [f32; 4]);

/// A mesh of `vertices`, each with one normal, one tangent and up to four influences.
fn mesh(vertices: &[Vertex]) -> MeshData {
    let mut m = MeshData::default();
    for (p, n, t, j, w) in vertices {
        m.positions.push(*p);
        m.normals.push(*n);
        m.tangents.push(*t);
        m.uv0.push([0.0, 0.0]);
        m.joints.push(*j);
        m.weights.push(*w);
    }
    m.indices = (0..vertices.len() as u32).collect();
    m.indices.truncate(vertices.len() / 3 * 3);
    m.aabb = ([-2.0; 3], [2.0; 3]);
    m
}

/// Two bones: `root` at the origin and `tip` one up, both bound at rest; `tip` bent 90° about Z,
/// and a third bone `wide` (child of root) scaled 2× along X.
fn two_bones() -> (Skeleton, Pose) {
    let mut joints = vec![
        joint("root", None, Vec3::ZERO),
        joint("tip", Some(0), Vec3::Y),
        joint("wide", Some(0), Vec3::ZERO),
    ];
    joints[1].inverse_bind = Mat4::from_translation(-Vec3::Y);
    let skeleton = Skeleton::new(joints).unwrap();
    let mut pose = skeleton.rest_pose();
    pose.bones[1].rotation = Quat::from_rotation_z(FRAC_PI_2);
    pose.bones[2].scale = Vec3::new(2.0, 1.0, 1.0);
    (skeleton, pose)
}

#[test]
fn cpu_skinning_matches_reference_vertices() {
    let (skeleton, pose) = two_bones();
    let palette = skeleton.palette(&pose).unwrap();
    let r = std::f32::consts::FRAC_1_SQRT_2;
    let fixture = mesh(&[
        // Only the tip: T(0,1,0)·Rz(90°)·T(0,-1,0) takes (1,1,0) to (0,2,0); +X normals turn to +Y.
        (
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, -1.0],
            [1, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
        ),
        // A quarter root, three quarters tip: (1,2,0) and (-1,2,0) blend to (-0.5, 2, 0).
        (
            [1.0, 2.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0, 1, 0, 0],
            [0.25, 0.75, 0.0, 0.0],
        ),
        // Only `wide` (2× along X): the normal follows the inverse transpose, not the matrix.
        (
            [1.0, 0.0, 0.0],
            [r, r, 0.0],
            [r, -r, 0.0, 1.0],
            [2, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
        ),
    ]);
    let out = skin_vertices(&fixture, &palette).unwrap();
    let eps = 1e-5;
    assert!(
        close(out.positions[0], Vec3::new(0.0, 2.0, 0.0), eps),
        "{}",
        out.positions[0]
    );
    assert!(close(out.normals[0], Vec3::Y, eps), "{}", out.normals[0]);
    assert!(
        close(out.tangents[0].truncate(), Vec3::NEG_X, eps),
        "{}",
        out.tangents[0]
    );
    assert_eq!(out.tangents[0].w, -1.0, "the bitangent sign is kept");
    assert!(
        close(out.positions[1], Vec3::new(-0.5, 2.0, 0.0), eps),
        "{}",
        out.positions[1]
    );
    assert!(close(out.normals[1], Vec3::Z, eps), "{}", out.normals[1]);
    assert!(
        close(out.positions[2], Vec3::new(2.0, 0.0, 0.0), eps),
        "{}",
        out.positions[2]
    );
    // M = diag(2, 1, 1): the inverse transpose takes (1, 1, 0) to (0.5, 1, 0), M itself to (2, 1, 0).
    let normal = Vec3::new(0.5, 1.0, 0.0).normalize();
    assert!(close(out.normals[2], normal, eps), "{}", out.normals[2]);
    let tangent = Vec3::new(2.0, -1.0, 0.0).normalize();
    assert!(
        close(out.tangents[2].truncate(), tangent, eps),
        "{}",
        out.tangents[2]
    );
    assert!(
        out.normals[2].dot(out.tangents[2].truncate()).abs() < eps,
        "still perpendicular"
    );
}

#[test]
fn palette_is_in_the_bind_meshs_space() {
    let (skeleton, pose) = two_bones();
    let mesh = Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0));
    let placed = skeleton.clone().with_mesh(mesh).unwrap();
    assert_eq!(placed.mesh(), mesh);
    let model = placed.model_space(&pose).unwrap();
    let expected = placed.joint_palette(&model, mesh);
    let got = placed.palette(&pose).unwrap();
    for (j, (a, b)) in got.iter().zip(&expected).enumerate() {
        assert!(a.abs_diff_eq(*b, 1e-6), "joint {j}: {a} vs {b}");
    }
    assert!(
        !got[1].abs_diff_eq(skeleton.palette(&pose).unwrap()[1], 1e-3),
        "the mesh matrix matters"
    );
    let singular = Mat4::from_scale(Vec3::new(1.0, 0.0, 1.0));
    assert!(
        skeleton.clone().with_mesh(singular).is_err(),
        "a singular mesh matrix"
    );
    assert!(
        skeleton.with_mesh(Mat4::NAN).is_err(),
        "a non-finite mesh matrix"
    );
}

#[test]
fn cpu_skinning_keeps_normals_unit() {
    let (skeleton, mut pose) = two_bones();
    pose.bones[0].scale = Vec3::new(0.5, 3.0, 1.5);
    pose.bones[1].rotation = Quat::from_euler(concordia_core::math::EulerRot::XYZ, 0.3, 1.1, -0.7);
    let palette = skeleton.palette(&pose).unwrap();
    let mut vertices = Vec::new();
    for i in 0..30 {
        let f = i as f32;
        let n = Vec3::new(f.sin(), f.cos(), 0.3).normalize();
        let t = n.any_orthonormal_vector();
        let a = (f * 0.37).fract();
        let b = (1.0 - a) * 0.5;
        vertices.push((
            [f * 0.1, 1.0 - f * 0.05, 0.2],
            n.to_array(),
            [t.x, t.y, t.z, 1.0],
            [0, 1, 2, 0],
            [a, b, 1.0 - a - b, 0.0],
        ));
    }
    let out = skin_vertices(&mesh(&vertices), &palette).unwrap();
    for (i, (n, t)) in out.normals.iter().zip(&out.tangents).enumerate() {
        assert!(
            (n.length() - 1.0).abs() < 1e-5,
            "normal {i}: length {}",
            n.length()
        );
        assert!((t.truncate().length() - 1.0).abs() < 1e-5, "tangent {i}");
    }
    let mut p = Project::new();
    let rig = p.fbx("skinned.fbx");
    let fbx = p.skinned_mesh(&rig);
    let (skeleton, walk) = walk_pose(&mut p, &rig, 0.4);
    let out = skin_vertices(&fbx, &skeleton.palette(&walk).unwrap()).unwrap();
    assert!(out.normals.iter().all(|n| (n.length() - 1.0).abs() < 1e-5));
}

#[test]
fn rest_pose_skinning_is_the_bind_mesh() {
    let mut p = Project::new();
    let simple = p.gltf("SimpleSkin", "glTF", "SimpleSkin.gltf");
    assert_rest_skinning_is_the_bind_mesh(&mut p, &simple);
    let rig = p.fbx("skinned.fbx");
    assert_rest_skinning_is_the_bind_mesh(&mut p, &rig);
}

fn assert_rest_skinning_is_the_bind_mesh(p: &mut Project, report: &ImportReport) {
    let skeleton = bound_skeleton(p, report);
    let prefab = p.prefab(report);
    let mesh_entity = p.skinned_entity(report);
    assert!(
        skeleton
            .mesh()
            .abs_diff_eq(model_matrix(&prefab, mesh_entity).unwrap(), 1e-6),
        "the bind mesh is the skinned entity"
    );
    let data = p.skinned_mesh(report);
    let out = skin_vertices(&data, &skeleton.palette(&skeleton.rest_pose()).unwrap()).unwrap();
    let size = (Vec3::from_array(data.aabb.1) - Vec3::from_array(data.aabb.0)).max_element();
    for (i, (got, want)) in out.positions.iter().zip(&data.positions).enumerate() {
        let off = (*got - Vec3::from_array(*want)).abs().max_element();
        assert!(off <= 1e-4 * size.max(1.0), "vertex {i}: {got} vs {want:?}");
    }
    for (i, (got, want)) in out.normals.iter().zip(&data.normals).enumerate() {
        assert!(close(*got, Vec3::from_array(*want), 1e-4), "normal {i}");
    }
    // The library gives play worlds the same skeleton.
    let library = loaded_library(p, report);
    let skin = Project::sub(report, "skin/0");
    let shared = library
        .shared_skeleton(&skin)
        .expect("the library shares its skeleton");
    assert_eq!(*shared, skeleton);
    assert!(std::ptr::eq(
        shared.as_ref(),
        library.skeleton(&skin).unwrap()
    ));
}

#[test]
fn skinning_refuses_what_it_cannot_skin() {
    let (skeleton, pose) = two_bones();
    let palette = skeleton.palette(&pose).unwrap();
    let ok = (
        [0.0; 3],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
        [0, 0, 0, 0],
        [1.0, 0.0, 0.0, 0.0],
    );
    let mut far = ok;
    far.3 = [0, 3, 0, 0];
    let err = skin_vertices(&mesh(&[ok, ok, far]), &palette)
        .unwrap_err()
        .to_string();
    assert!(err.contains("vertex 2") && err.contains("joint 3"), "{err}");
    let mut unskinned = mesh(&[ok, ok, ok]);
    unskinned.joints.clear();
    let err = skin_vertices(&unskinned, &palette).unwrap_err().to_string();
    assert!(err.contains("joints"), "{err}");
}

#[test]
fn skinned_bounds_contain_every_skinned_vertex() {
    let mut p = Project::new();
    let rig = p.fbx("skinned.fbx");
    let data = p.skinned_mesh(&rig);
    let boxes = joint_bounds(&data);
    let (skeleton, _) = walk_pose(&mut p, &rig, 0.0);
    assert_eq!(
        boxes.len(),
        skeleton.len(),
        "one box per joint the mesh may use"
    );
    let rest = skeleton.palette(&skeleton.rest_pose()).unwrap();
    let (lo, hi) = skinned_bounds(&boxes, &rest).expect("the mesh is influenced");
    let (alo, ahi) = (Vec3::from_array(data.aabb.0), Vec3::from_array(data.aabb.1));
    assert!(
        close(lo, alo, 1e-3) && close(hi, ahi, 1e-3),
        "at rest the bounds are the mesh's"
    );
    for t in [0.1, 0.35, 0.6, 0.9] {
        let (skeleton, pose) = walk_pose(&mut p, &rig, t);
        let palette = skeleton.palette(&pose).unwrap();
        let (lo, hi) = skinned_bounds(&boxes, &palette).unwrap();
        let out = skin_vertices(&data, &palette).unwrap();
        for v in &out.positions {
            assert!(
                v.cmpge(lo - 1e-4).all() && v.cmple(hi + 1e-4).all(),
                "at {t}s, {v} is outside {lo}..{hi}"
            );
        }
    }
    let unused: Vec<Option<(Vec3, Vec3)>> = vec![None; 3];
    assert_eq!(skinned_bounds(&unused, &rest[..3]), None);
}

#[test]
fn poses_interpolate_bone_by_bone() {
    let (_, a) = two_bones();
    let mut b = a.clone();
    b.bones[0].translation = Vec3::new(4.0, 0.0, 0.0);
    b.bones[1].rotation = Quat::IDENTITY;
    b.bones[2].scale = Vec3::ONE;
    let quarter = a.lerp(&b, 0.25).unwrap();
    assert!(close(
        quarter.bones[0].translation,
        Vec3::new(1.0, 0.0, 0.0),
        1e-6
    ));
    let angle = quarter.bones[1].rotation.angle_between(Quat::IDENTITY);
    assert!((angle - FRAC_PI_2 * 0.75).abs() < 1e-5, "slerp: {angle}");
    assert!(close(
        quarter.bones[2].scale,
        Vec3::new(1.75, 1.0, 1.0),
        1e-6
    ));
    assert_eq!(a.lerp(&b, 0.0).unwrap(), a);
    assert_eq!(a.lerp(&b, 1.0).unwrap(), b);
    let short = Pose {
        bones: a.bones[..2].to_vec(),
    };
    assert!(a.lerp(&short, 0.5).is_err());
}

/// The skin's skeleton bound to its mesh, as the library builds it.
fn bound_skeleton(p: &mut Project, report: &ImportReport) -> Skeleton {
    let skin = Project::sub(report, "skin/0");
    let prefab = p.prefab(report);
    p.skeleton(report)
        .with_mesh(bind_mesh(&prefab, &skin).unwrap())
        .unwrap()
}

/// The library for one character of `report` playing `anim_stacks.fbx`'s Walk.
fn loaded_library(p: &mut Project, report: &ImportReport) -> AnimLibrary {
    let mut world = World::new();
    let id = world.spawn(&mut DeterministicIdGen::new(1)).unwrap();
    let skin = Project::sub(report, "skin/0");
    world.insert(id, Skinned { skin }).unwrap();
    let (library, report) = AnimLibrary::load(&mut p.db, &registry(), &mut world).unwrap();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    library
}

/// `skinned.fbx`'s skeleton and its pose `t` seconds into `anim_stacks.fbx`'s Walk.
fn walk_pose(p: &mut Project, rig: &ImportReport, t: f32) -> (Skeleton, Pose) {
    let skeleton = bound_skeleton(p, rig);
    let actions = p.fbx("anim_stacks.fbx");
    let clip = p.clip(&actions, "clip/Armature|Walk");
    let bound = BoundClip::bind(&clip, &skeleton).unwrap();
    let mut pose = skeleton.rest_pose();
    bound.sample(t, &mut pose).unwrap();
    (skeleton, pose)
}

fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    concordia_assets::register_types(&mut registry).unwrap();
    concordia_anim::register_types(&mut registry).unwrap();
    registry
}

impl Project {
    /// The mesh drawn by the skinned entity of `report`.
    fn skinned_mesh(&mut self, report: &ImportReport) -> MeshData {
        let doc = self.prefab(report);
        let entity = doc
            .entities
            .iter()
            .find(|e| e.components.contains_key("Skinned"))
            .unwrap();
        let renderer = entity.components["MeshRenderer"]
            .as_any()
            .downcast_ref::<MeshRenderer>()
            .unwrap()
            .clone();
        self.db.load(&renderer.mesh, AssetKind::Mesh).unwrap()
    }
}
