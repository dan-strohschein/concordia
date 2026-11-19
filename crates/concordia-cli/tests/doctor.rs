//! `concordia studio doctor`: environment checks; a subscription login is all Studio needs (M3 phase 3.5).

mod common;

use std::path::Path;

use common::{concordia, stderr, stdout};

/// A fake `claude` on PATH that prints `auth_json` for `claude auth status`.
fn shim(dir: &Path, auth_json: &str) {
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '2.1.328 (Claude Code)'; exit 0; fi\nif [ \"$1 $2\" = \"auth status\" ]; then echo '{auth_json}'; exit 0; fi\nexit 1\n"
    );
    let path = dir.join("claude");
    std::fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn doctor(project: &Path, path_env: &str) -> std::process::Output {
    concordia()
        .args(["studio", "doctor"])
        .current_dir(project)
        .env("PATH", path_env)
        .env_remove("ANTHROPIC_API_KEY")
        .output()
        .unwrap()
}

/// The directory of the `concordia` binary under test (the hooks need it on PATH).
fn concordia_dir() -> String {
    Path::new(env!("CARGO_BIN_EXE_concordia"))
        .parent()
        .unwrap()
        .display()
        .to_string()
}

fn new_project(dir: &Path) -> std::path::PathBuf {
    assert!(common::run(&["new", "g"], dir).status.success());
    dir.join("g")
}

#[test]
fn doctor_reports_a_subscription_login_as_ok_and_never_requires_an_api_key() {
    let dir = tempfile::tempdir().unwrap();
    let project = new_project(dir.path());
    let bin = tempfile::tempdir().unwrap();
    shim(bin.path(), r#"{"loggedIn":true,"authMethod":"claude.ai"}"#);
    let path = format!("{}:{}:/usr/bin:/bin", bin.path().display(), concordia_dir());
    let output = doctor(&project, &path);
    let out = stdout(&output);
    assert!(output.status.success(), "{out}\n{}", stderr(&output));
    assert!(out.contains("claude.ai"), "{out}");
    assert!(out.contains("no API key needed"), "{out}");
    assert!(out.contains("in sync"), "{out}");
}

#[test]
fn doctor_reports_a_missing_claude_cli_with_an_install_hint() {
    let dir = tempfile::tempdir().unwrap();
    let project = new_project(dir.path());
    let output = doctor(&project, &format!("{}:/usr/bin:/bin", concordia_dir()));
    assert_eq!(output.status.code(), Some(1));
    let out = stdout(&output);
    assert!(out.contains("claude") && out.contains("install"), "{out}");
}
