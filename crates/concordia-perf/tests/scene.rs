//! Perf scenes: validated text, and a camera path the measured frames move along (M4 4.1a).

use concordia_perf::{CameraKey, PerfScene};

const TEXT: &str = r#"PerfScene(
    project: "../../../examples/m1-physics-stack",
    scene: "scenes/stack.scene.ron",
    width: 1280,
    height: 720,
    warmup_frames: 120,
    frames: 300,
    fov_y: 0.9,
    path: [
        (eye: (0.0, 2.0, 10.0), target: (0.0, 1.0, 0.0)),
        (eye: (10.0, 2.0, 0.0), target: (0.0, 1.0, 0.0)),
        (eye: (0.0, 6.0, -10.0), target: (0.0, 1.0, 0.0)),
    ],
)"#;

#[test]
fn perf_scene_reads_and_rejects_bad_values() {
    let scene = PerfScene::read(TEXT).expect("reads");
    assert_eq!(
        (scene.width, scene.frames, scene.path.len()),
        (1280, 300, 3)
    );
    for (from, to, why) in [
        ("frames: 300", "frames: 0", "frames"),
        ("width: 1280", "width: 0", "size"),
        ("fov_y: 0.9", "fov_y: 3.5", "fov_y"),
        ("warmup_frames: 120", "warmup: 120", "unknown field"),
    ] {
        let bad = TEXT.replace(from, to);
        let err = PerfScene::read(&bad).expect_err(why);
        assert!(
            err.to_string().contains(why) || why == "unknown field",
            "{why}: {err}"
        );
    }
    let no_path = TEXT.split("path:").next().unwrap().to_owned() + "path: [],\n)";
    let err = PerfScene::read(&no_path).expect_err("an empty path");
    assert!(err.to_string().contains("path"), "{err}");
}

#[test]
fn measured_frames_move_evenly_through_the_keys() {
    let scene = PerfScene::read(TEXT).unwrap();
    let pose = |i| scene.pose(i, 5);
    let key = |eye: [f32; 3]| CameraKey {
        eye,
        target: [0.0, 1.0, 0.0],
    };
    assert_eq!(pose(0), key([0.0, 2.0, 10.0]), "starts on the first key");
    assert_eq!(pose(1), key([5.0, 2.0, 5.0]), "halfway to the second");
    assert_eq!(pose(2), key([10.0, 2.0, 0.0]));
    assert_eq!(pose(4), key([0.0, 6.0, -10.0]), "ends on the last key");
    assert_eq!(
        scene.pose(0, 1),
        key([0.0, 2.0, 10.0]),
        "one frame: the first key"
    );
}
