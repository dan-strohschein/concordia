//! Clips bind to any skeleton by bone name (2.4.1, ADR-0074).

mod common;

use common::Project;
use concordia_anim::BoundClip;
use concordia_assets::ChannelPath;
use concordia_core::math::Quat;

#[test]
fn clip_binds_to_another_skeleton_by_bone_name() {
    let mut p = Project::new();
    let character = p.fbx("skinned.fbx");
    let run_file = p.fbx("mixamo_run.fbx");
    let skeleton = p.skeleton(&character);
    let run = p.clip(&run_file, "clip/Armature|mixamo.com");
    let bound = BoundClip::bind(&run, &skeleton).unwrap();
    assert_eq!(bound.duration(), run.duration);
    for name in bound.unmatched() {
        assert!(
            skeleton.index_of(name).is_none(),
            "{name} is a joint but was unmatched"
        );
    }
    // At a key time the sampled rotation is that key, for every joint the clip rotates.
    let k = 5;
    let mut pose = skeleton.rest_pose();
    let mut checked = 0;
    for c in run
        .channels
        .iter()
        .filter(|c| c.path == ChannelPath::Rotation)
    {
        let Some(j) = skeleton.index_of(&c.target_name) else {
            continue;
        };
        bound.sample(c.times[k], &mut pose).unwrap();
        let key = Quat::from_slice(&c.values[k * 4..k * 4 + 4]).normalize();
        let got = pose.bones[j].rotation;
        assert!(
            got.dot(key).abs() > 1.0 - 1e-6,
            "{}: {got} vs key {key}",
            c.target_name
        );
        checked += 1;
    }
    assert_eq!(
        checked,
        skeleton.len(),
        "the run rotates every joint of the character"
    );
}
