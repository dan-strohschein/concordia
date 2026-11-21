//! Skeletons from imported skins: joint order, validation, model space and the bind pose (2.4.1).

mod common;

use common::{Project, close, joint};
use concordia_anim::{Pose, Skeleton, model_matrix};
use concordia_core::math::{Mat4, Quat, Vec3};

#[test]
fn skeleton_orders_parents_before_children() {
    let skeleton = Skeleton::new(vec![
        joint("hand", Some(1), Vec3::Y),
        joint("arm", Some(2), Vec3::Y),
        joint("root", None, Vec3::ZERO),
    ])
    .unwrap();
    assert_eq!(skeleton.order(), [2, 1, 0]);
    assert_eq!(skeleton.index_of("arm"), Some(1));
    let mut p = Project::new();
    let rig = p.fbx("skinned.fbx");
    let skeleton = p.skeleton(&rig);
    assert_eq!(skeleton.len(), 7);
    let mut seen = vec![false; skeleton.len()];
    for &j in skeleton.order() {
        if let Some(parent) = skeleton.joints()[j].parent {
            assert!(
                seen[parent],
                "{} before its parent",
                skeleton.joints()[j].name
            );
        }
        seen[j] = true;
    }
    assert!(seen.iter().all(|s| *s), "every joint is ordered once");
}

#[test]
fn bad_skeletons_are_errors_naming_the_joint() {
    let err = |joints| Skeleton::new(joints).unwrap_err().to_string();
    let cycle = err(vec![
        joint("a", Some(1), Vec3::ZERO),
        joint("b", Some(0), Vec3::ZERO),
    ]);
    assert!(cycle.contains("cycle") && cycle.contains('a'), "{cycle}");
    let out = err(vec![joint("a", Some(5), Vec3::ZERO)]);
    assert!(out.contains("a") && out.contains('5'), "{out}");
    let dup = err(vec![
        joint("a", None, Vec3::ZERO),
        joint("a", None, Vec3::ZERO),
    ]);
    assert!(dup.contains("duplicate") && dup.contains("\"a\""), "{dup}");
    let empty = err(vec![joint("", None, Vec3::ZERO)]);
    assert!(empty.contains("empty"), "{empty}");

    let mut p = Project::new();
    let rig = p.fbx("skinned.fbx");
    let prefab = p.prefab(&rig);
    let mut skin = p.skin(&rig);
    skin.inverse_bind.pop();
    let lengths = Skeleton::from_skin(&skin, &prefab).unwrap_err().to_string();
    assert!(lengths.contains("inverse bind"), "{lengths}");
    let mut skin = p.skin(&rig);
    skin.joints[0] = "018bcfe5-6800-7000-a424-6c55076b137f".parse().unwrap();
    let missing = Skeleton::from_skin(&skin, &prefab).unwrap_err().to_string();
    assert!(missing.contains(&skin.joint_names[0]), "{missing}");
}

#[test]
fn model_space_chains_parents_offsets_and_locals() {
    let mut skeleton_joints = vec![
        joint("root", None, Vec3::new(0.0, 1.0, 0.0)),
        joint("tip", Some(0), Vec3::new(0.0, 1.0, 0.0)),
    ];
    skeleton_joints[1].offset = Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0));
    let skeleton = Skeleton::new(skeleton_joints).unwrap();
    let mut pose = skeleton.rest_pose();
    assert_eq!(pose.bones.len(), 2);
    pose.bones[0].rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    let model = skeleton.model_space(&pose).unwrap();
    let at = |j: usize| model[j].transform_point3(Vec3::ZERO);
    assert!(close(at(0), Vec3::new(0.0, 1.0, 0.0), 1e-6), "{}", at(0));
    // Rotating the root 90° about Z turns the tip's (0, 1, 2) offset into (-1, 0, 2).
    assert!(close(at(1), Vec3::new(-1.0, 1.0, 2.0), 1e-6), "{}", at(1));
    let short = Pose {
        bones: vec![pose.bones[0]],
    };
    assert!(
        skeleton.model_space(&short).is_err(),
        "a pose of the wrong size"
    );
}

/// At rest, every skinning matrix is the identity: the mesh is drawn where it was bound.
fn assert_rest_palette_is_identity(p: &mut Project, report: &concordia_assets::ImportReport) {
    let skeleton = p.skeleton(report);
    let prefab = p.prefab(report);
    let mesh = model_matrix(&prefab, p.skinned_entity(report)).unwrap();
    let model = skeleton.model_space(&skeleton.rest_pose()).unwrap();
    let palette = skeleton.joint_palette(&model, mesh);
    assert_eq!(palette.len(), skeleton.len());
    for (j, m) in palette.iter().enumerate() {
        let worst = (*m - Mat4::IDENTITY)
            .to_cols_array()
            .iter()
            .fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(
            worst < 1e-4,
            "{}: off by {worst}",
            skeleton.joints()[j].name
        );
    }
}

#[test]
fn skeleton_bind_pose_matches_inverse_bind_matrices() {
    let mut p = Project::new();
    let simple = p.gltf("SimpleSkin", "glTF", "SimpleSkin.gltf");
    assert_rest_palette_is_identity(&mut p, &simple);
    let rig = p.fbx("skinned.fbx");
    assert_rest_palette_is_identity(&mut p, &rig);
}
