//! `concordia fmt` (M0 phase 0.7).

mod common;

use common::{run, stdout};

const MESSY: &str = r#"Scene(id: "0193a1c2-7e40-7000-8000-000000000001", format: 1, name: "S",
  entities: [Entity(components: {"Name": "A"}, id: "0193a1c2-7e40-7000-8000-000000000002")])"#;

#[test]
fn fmt_check_fails_on_noncanonical_and_fix_rewrites() {
    let dir = tempfile::tempdir().unwrap();
    let scenes = dir.path().join("scenes");
    std::fs::create_dir_all(&scenes).unwrap();
    std::fs::write(scenes.join("messy.scene.ron"), MESSY).unwrap();
    std::fs::create_dir_all(dir.path().join(".concordia")).unwrap();
    std::fs::write(
        dir.path().join(".concordia/ignored.scene.ron"),
        "not even ron",
    )
    .unwrap();

    let check = run(&["fmt", "--check"], dir.path());
    assert_eq!(check.status.code(), Some(1));
    assert!(
        stdout(&check).contains("messy.scene.ron"),
        "{}",
        stdout(&check)
    );

    let fix = run(&["fmt"], dir.path());
    assert!(
        fix.status.success(),
        "{}",
        String::from_utf8_lossy(&fix.stderr)
    );
    let text = std::fs::read_to_string(scenes.join("messy.scene.ron")).unwrap();
    assert!(text.starts_with("// Concordia scene, format 1."));
    assert!(
        run(&["fmt", "--check", "scenes"], dir.path())
            .status
            .success()
    );
}

#[test]
fn fmt_reports_parse_errors_with_position() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("bad.scene.ron"),
        "Scene(format: 1,\n  id: @)",
    )
    .unwrap();
    let output = run(&["fmt", "--check"], dir.path());
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bad.scene.ron: line 2, col 7"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
