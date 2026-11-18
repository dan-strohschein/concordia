//! `editor_shell_smoke_renders_native_viewport` (macOS GPU, needs a GUI session): the built editor
//! opens on a copy of the example project, the UI reports its viewport, 10 frames present into the
//! native view, and a capture of the same view is not blank.
#![cfg(target_os = "macos")]

use std::process::Command;

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn editor_shell_smoke_renders_native_viewport() {
    if std::env::var_os("CONCORDIA_SKIP_EDITOR_SMOKE").is_some() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    copy_dir(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/m1-physics-stack"),
        dir.path(),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_concordia-editor"))
        .args(["--project", dir.path().to_str().unwrap(), "--smoke-test"])
        .output()
        .expect("the editor runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("SMOKE ok presented=10"), "{stdout}");
}
