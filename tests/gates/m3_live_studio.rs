//! M3 live gate (run via `just gate-live M3`): a real `claude -p --agent director` session on the
//! subscription login (no API key) worked T-0001 in a copy of the fixture game. This test judges
//! what it left with the same checker as the scripted loop (`m3_checker`): outcomes, never
//! transcripts. The checker itself is tested without a model by `m3_studio_loop_scripted`.

use std::path::PathBuf;

use concordia_tests::m3_checker::{check_all, check_env};
use serde_json::Value;

fn env_path(var: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {var} (run via `just gate-live M3`)"))
}

#[test]
#[ignore = "run via just gate-live M3"]
fn m3_live_director_gameplay_qa_fix_seeded_bug() {
    let result: Value =
        serde_json::from_str(&std::fs::read_to_string(env_path("M3_LIVE_RESULT")).unwrap())
            .unwrap();
    assert_eq!(result["is_error"], false, "the agent run failed: {result}");
    let names = std::fs::read_to_string(env_path("M3_LIVE_ENV")).unwrap();
    let mut failures: Vec<String> = check_env(&names).err().into_iter().collect();
    failures.extend(check_all(
        &env_path("M3_LIVE_PROJECT"),
        &env_path("M3_LIVE_BASELINE"),
        &env_path("M3_LIVE_PRISTINE"),
    ));
    assert!(
        failures.is_empty(),
        "M3 live outcomes failed:\n- {}",
        failures.join("\n- ")
    );
}

/// A key or token in the agent's environment fails the gate, whatever the provider.
#[test]
fn m3_live_env_check_rejects_keys_and_tokens() {
    assert!(check_env("PATH\nHOME\nCONCORDIA_X\n").is_ok());
    for bad in [
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ] {
        assert!(check_env(&format!("PATH\n{bad}\n")).is_err(), "{bad}");
    }
}
