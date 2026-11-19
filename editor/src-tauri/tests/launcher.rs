//! The Claude Code launcher (M3 phase 3.8, ADR-0083): detects `claude`, shows the login method,
//! opens an interactive Director session in a terminal or runs a headless one. It never reads,
//! stores or logs credentials. Fake `claude` and `open` shims stand in for the real programs.
#![cfg(unix)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use concordia_editor::claude_launcher::{
    Launcher, StreamEvent, headless_argv, parse_stream_line, redact,
};

const SECRET: &str = "sk-ant-test-0123456789";

fn script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A fake `claude`: version, auth status, and a short stream-json run for `-p`.
fn claude_shim(dir: &Path, auth_method: &str) {
    script(
        &dir.join("claude"),
        &format!(
            r#"case "$1" in
  --version) echo '2.1.328 (Claude Code)'; exit 0;;
  auth) echo '{{"loggedIn":true,"authMethod":"{auth_method}"}}'; exit 0;;
  -p)
    echo '{{"type":"system","subtype":"init","session_id":"s1"}}'
    echo '{{"type":"assistant","parent_tool_use_id":null,"message":{{"content":[{{"type":"text","text":"Planning T-0001"}},{{"type":"tool_use","id":"toolu_1","name":"Agent","input":{{}}}}]}}}}'
    echo '{{"type":"assistant","parent_tool_use_id":"toolu_1","message":{{"content":[{{"type":"tool_use","id":"toolu_2","name":"mcp__concordia__task_claim","input":{{}}}}]}}}}'
    echo '{{"type":"result","subtype":"success","is_error":false,"num_turns":3}}'
    exit 0;;
esac
exit 1
"#
        ),
    );
}

/// A temp project, HOME and bin dir; the environment the launcher's children get.
struct Sandbox {
    project: tempfile::TempDir,
    home: tempfile::TempDir,
    bin: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            project: tempfile::tempdir().unwrap(),
            home: tempfile::tempdir().unwrap(),
            bin: tempfile::tempdir().unwrap(),
        }
    }

    fn env(&self) -> Vec<(OsString, OsString)> {
        vec![
            (
                "PATH".into(),
                format!("{}:/usr/bin:/bin", self.bin.path().display()).into(),
            ),
            ("HOME".into(), self.home.path().into()),
            ("ANTHROPIC_API_KEY".into(), SECRET.into()),
        ]
    }

    fn launcher(&self) -> Launcher {
        Launcher::new(self.project.path(), self.env()).with_opener(self.bin.path().join("open"))
    }
}

/// Every file under `dir`, relative.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path.strip_prefix(dir).unwrap().to_owned());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn launcher_detects_missing_claude_and_reports_install_hint() {
    let sandbox = Sandbox::new();
    let found = sandbox.launcher().detect();
    assert_eq!(found.claude, None);
    let hint = found.install_hint.expect("an install hint");
    assert!(
        hint.contains("install") && hint.contains("code.claude.com"),
        "{hint}"
    );
}

#[test]
fn auth_status_json_parsed_and_api_key_never_required() {
    let sandbox = Sandbox::new();
    claude_shim(sandbox.bin.path(), "claude.ai");
    let found = sandbox.launcher().detect();
    assert_eq!(found.claude, Some(sandbox.bin.path().join("claude")));
    assert_eq!(found.version.as_deref(), Some("2.1.328 (Claude Code)"));
    assert_eq!(found.auth_method.as_deref(), Some("claude.ai"));
    assert!(found.subscription);
    assert_eq!(found.install_hint, None);
    let text = format!("{found:?}");
    assert!(
        !text.contains(SECRET),
        "detection must not carry the key: {text}"
    );
}

#[test]
fn launcher_builds_headless_argv_with_director_agent_and_no_secrets() {
    let argv = headless_argv("Work the board until T-0001 is done.", Some(2.5)).unwrap();
    assert_eq!(&argv[..2], ["-p", "Work the board until T-0001 is done."]);
    let pairs: Vec<(&str, &str)> = argv
        .windows(2)
        .map(|w| (w[0].as_str(), w[1].as_str()))
        .collect();
    assert!(pairs.contains(&("--agent", "director")), "{argv:?}");
    assert!(
        pairs.contains(&("--output-format", "stream-json")),
        "{argv:?}"
    );
    assert!(pairs.contains(&("--max-budget-usd", "2.5")), "{argv:?}");
    assert!(argv.iter().any(|a| a == "--verbose"), "{argv:?}");
    assert!(
        !argv.iter().any(|a| a == "--bare"),
        "--bare skips the subscription login"
    );
    for word in ["key", "token", "sk-ant", "ANTHROPIC"] {
        assert!(
            !argv
                .iter()
                .any(|a| a.to_lowercase().contains(&word.to_lowercase())),
            "{argv:?}"
        );
    }
    assert!(
        headless_argv("  ", None).is_err(),
        "an empty prompt is refused"
    );
    assert!(
        headless_argv("go", Some(-1.0)).is_err(),
        "a negative budget is refused"
    );
    assert!(
        !headless_argv("go", None)
            .unwrap()
            .iter()
            .any(|a| a == "--max-budget-usd")
    );
}

#[test]
fn launcher_does_not_write_any_credential_or_env_file() {
    let sandbox = Sandbox::new();
    claude_shim(sandbox.bin.path(), "claude.ai");
    script(&sandbox.bin.path().join("open"), "exit 0\n");
    let launcher = sandbox.launcher();
    let mut events = Vec::new();
    let status = launcher
        .run_headless("Work the board.", None, |e| events.push(e))
        .unwrap();
    assert!(status.success());
    launcher.open_terminal("Terminal").unwrap();
    assert_eq!(files(sandbox.home.path()), Vec::<PathBuf>::new());
    assert_eq!(
        files(sandbox.project.path()),
        [
            PathBuf::from(".concordia/launcher/director.command"),
            PathBuf::from(".concordia/launcher/launcher.log"),
        ]
    );
    for file in files(sandbox.project.path()) {
        let text = std::fs::read_to_string(sandbox.project.path().join(&file)).unwrap();
        assert!(!text.contains(SECRET), "{}: {text}", file.display());
        assert!(
            !text.contains("ANTHROPIC_API_KEY="),
            "{}: {text}",
            file.display()
        );
    }
}

#[test]
fn interactive_launch_opens_the_terminal_on_a_director_command_file() {
    let sandbox = Sandbox::new();
    claude_shim(sandbox.bin.path(), "claude.ai");
    let args = sandbox.bin.path().join("open-args");
    script(
        &sandbox.bin.path().join("open"),
        &format!("printf '%s\\n' \"$@\" > '{}'\n", args.display()),
    );
    let command = sandbox.launcher().open_terminal("iTerm").unwrap();
    let opened = std::fs::read_to_string(&args).unwrap();
    assert_eq!(
        opened.lines().collect::<Vec<_>>(),
        ["-a", "iTerm", command.to_str().unwrap()]
    );
    let text = std::fs::read_to_string(&command).unwrap();
    assert!(text.starts_with("#!/bin/sh\n"), "{text}");
    assert!(text.contains("--agent director"), "{text}");
    let project = sandbox.project.path().display().to_string();
    assert!(text.contains(&format!("cd '{project}'")), "{text}");
    assert!(
        sandbox.launcher().open_terminal("").is_err(),
        "a terminal app is required"
    );
}

#[test]
fn launcher_log_redacts_environment_values() {
    let env = vec![
        (OsString::from("ANTHROPIC_API_KEY"), OsString::from(SECRET)),
        (OsString::from("SHORT"), OsString::from("ab")),
    ];
    let line = format!("child failed: key={SECRET} ok ab");
    let redacted = redact(&line, &env);
    assert!(!redacted.contains(SECRET), "{redacted}");
    assert!(
        redacted.contains("<redacted:ANTHROPIC_API_KEY>"),
        "{redacted}"
    );
    assert!(
        redacted.contains("ok ab"),
        "tiny values would redact everything: {redacted}"
    );

    let sandbox = Sandbox::new();
    script(
        &sandbox.bin.path().join("claude"),
        &format!("echo \"$ANTHROPIC_API_KEY\" >&2\necho 'not json {SECRET}'\nexit 3\n"),
    );
    let status = sandbox.launcher().run_headless("go", None, |_| {}).unwrap();
    assert_eq!(status.code(), Some(3));
    let log = std::fs::read_to_string(
        sandbox
            .project
            .path()
            .join(".concordia/launcher/launcher.log"),
    )
    .unwrap();
    assert!(!log.contains(SECRET), "{log}");
    assert!(log.contains("<redacted:ANTHROPIC_API_KEY>"), "{log}");
}

#[test]
fn stream_json_lines_are_summarized_with_their_parent_tool_use() {
    let line = r#"{"type":"assistant","parent_tool_use_id":"toolu_1","message":{"content":[{"type":"text","text":"Claiming"},{"type":"tool_use","id":"toolu_2","name":"mcp__concordia__task_claim","input":{"id":"T-0001"}}]}}"#;
    assert_eq!(
        parse_stream_line(line),
        Some(StreamEvent {
            group: Some("toolu_1".into()),
            kind: "assistant".into(),
            text: "Claiming · mcp__concordia__task_claim".into(),
        })
    );
    let result = r#"{"type":"result","subtype":"success","is_error":false,"num_turns":3}"#;
    assert_eq!(
        parse_stream_line(result).map(|e| e.text),
        Some("success after 3 turns".into())
    );
    assert_eq!(parse_stream_line("not json"), None);
}

#[test]
fn headless_events_reach_the_listener_in_order() {
    let sandbox = Sandbox::new();
    claude_shim(sandbox.bin.path(), "claude.ai");
    let mut events = Vec::new();
    sandbox
        .launcher()
        .run_headless("go", None, |e| events.push(e))
        .unwrap();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["system", "assistant", "assistant", "result"]);
    assert_eq!(events[2].group.as_deref(), Some("toolu_1"));
    assert_eq!(events[1].group, None);
}
