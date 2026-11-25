//! `editor_shell_smoke_renders_native_viewport` (needs a GUI session: macOS with Metal, or Linux
//! on X11 with Vulkan): the built editor opens on a copy of the example project, the UI reports its
//! viewport, 10 frames present into the native view, and a capture of the same view is not blank.
//! On Linux the viewport also detaches before its window goes (M4 4.1v(c)); without a display it
//! skips unless `CONCORDIA_REQUIRE_VULKAN=1`.
#![cfg(any(target_os = "macos", target_os = "linux"))]

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
    if cfg!(target_os = "linux") && std::env::var_os("DISPLAY").is_none() {
        assert!(
            !concordia_testkit_vulkan_required(),
            "CONCORDIA_REQUIRE_VULKAN=1 but there is no X11 display"
        );
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
    assert!(stdout.contains("SMOKE ok queued=10 rendered="), "{stdout}");
    if cfg!(target_os = "linux") {
        assert!(stdout.contains("VIEWPORT detached"), "{stdout}\n{stderr}");
    }
}

/// `CONCORDIA_REQUIRE_VULKAN` is exactly `1` (as `concordia_testkit::vulkan_required`).
fn concordia_testkit_vulkan_required() -> bool {
    std::env::var("CONCORDIA_REQUIRE_VULKAN").as_deref() == Ok("1")
}
