//! `concordia_studio::doctor`: the machine checks behind `concordia studio doctor`, `studio.doctor`
//! and the editor's Claude Code launcher (M3 phase 3.8). Fake `claude` shims stand in for the CLI.
#![cfg(unix)]

use std::ffi::OsString;
use std::path::Path;

use concordia_studio::doctor::{self, CheckStatus, DoctorEnv};

/// A fake `claude` in `dir` that prints `auth_json` for `claude auth status`.
fn shim(dir: &Path, auth_json: &str) {
    use std::os::unix::fs::PermissionsExt;
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '2.1.328 (Claude Code)'; exit 0; fi\nif [ \"$1 $2\" = \"auth status\" ]; then echo '{auth_json}'; exit 0; fi\nexit 1\n"
    );
    let path = dir.join("claude");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn env(path: &Path, api_key_set: bool) -> DoctorEnv {
    DoctorEnv {
        path: Some(OsString::from(format!("{}:/usr/bin:/bin", path.display()))),
        api_key_set,
    }
}

#[test]
fn a_subscription_login_is_ok_and_no_api_key_is_needed() {
    let bin = tempfile::tempdir().unwrap();
    shim(bin.path(), r#"{"loggedIn":true,"authMethod":"claude.ai"}"#);
    let env = env(bin.path(), false);
    assert_eq!(doctor::claude_path(&env), Some(bin.path().join("claude")));
    assert_eq!(
        doctor::claude_version(&env).as_deref(),
        Some("2.1.328 (Claude Code)")
    );
    assert_eq!(doctor::auth_method(&env).as_deref(), Some("claude.ai"));
    let checks = doctor::machine_checks(&env);
    let login = checks.iter().find(|c| c.id == "login").unwrap();
    assert_eq!(login.status, CheckStatus::ok);
    assert!(login.text.contains("no API key needed"), "{}", login.text);
    assert!(checks.iter().all(|c| c.id != "api_key"), "{checks:?}");
}

#[test]
fn a_missing_claude_cli_fails_with_an_install_hint() {
    let empty = tempfile::tempdir().unwrap();
    let env = env(empty.path(), false);
    assert_eq!(doctor::claude_path(&env), None);
    let checks = doctor::machine_checks(&env);
    let claude = checks.iter().find(|c| c.id == "claude").unwrap();
    assert_eq!(claude.status, CheckStatus::fail);
    assert!(claude.text.contains("install"), "{}", claude.text);
    assert!(checks.iter().all(|c| c.id != "login"), "{checks:?}");
}

#[test]
fn an_api_key_is_only_a_note_and_other_logins_are_notes() {
    let bin = tempfile::tempdir().unwrap();
    shim(bin.path(), r#"{"loggedIn":true,"authMethod":"api_key"}"#);
    let checks = doctor::machine_checks(&env(bin.path(), true));
    let status = |id: &str| checks.iter().find(|c| c.id == id).map(|c| c.status);
    assert_eq!(status("api_key"), Some(CheckStatus::note));
    assert_eq!(status("login"), Some(CheckStatus::note));
}

#[test]
fn not_logged_in_fails() {
    let bin = tempfile::tempdir().unwrap();
    shim(bin.path(), r#"{"loggedIn":false,"authMethod":"none"}"#);
    let checks = doctor::machine_checks(&env(bin.path(), false));
    let login = checks.iter().find(|c| c.id == "login").unwrap();
    assert_eq!(login.status, CheckStatus::fail);
}

#[test]
fn sync_freshness_checks_say_what_to_run() {
    assert_eq!(doctor::sync_check(Ok(false)).status, CheckStatus::ok);
    let stale = doctor::sync_check(Ok(true));
    assert_eq!(stale.status, CheckStatus::fail);
    assert!(
        stale.text.contains("concordia studio sync"),
        "{}",
        stale.text
    );
    let broken = doctor::sync_check(Err("bad team.toml".into()));
    assert!(broken.text.contains("bad team.toml"), "{}", broken.text);
}
