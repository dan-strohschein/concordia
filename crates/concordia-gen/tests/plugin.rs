//! Opt-in content plugins (M3 phase 3.7c): out-of-process providers, loaded only when
//! `[gen]` lists them, speaking JSON over stdin and stdout.

use concordia_gen::{GenConfig, GenContext, GenModule, PluginConfig};
use serde_json::json;

const PLUGIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plugin/echo_plugin.py"
);

fn echo() -> PluginConfig {
    PluginConfig {
        id: "echo".into(),
        kind: concordia_gen::GenKind::texture,
        extensions: vec!["txt".into()],
        command: vec!["python3".into(), "-I".into(), PLUGIN.into()],
        timeout_s: None,
    }
}

/// No plugin runs unless the project lists it; a listed one is not procedural.
#[test]
fn model_provider_plugin_is_never_loaded_unless_configured() {
    let plain = GenModule::new(&GenConfig::default());
    let ids: Vec<&str> = plain.registry().iter().map(|p| p.id()).collect();
    assert_eq!(
        ids,
        ["mesh", "sfx", "music", "texture", "blender"],
        "the built-in providers only"
    );
    let with = GenModule::new(&GenConfig {
        plugin: vec![echo()],
        ..GenConfig::default()
    });
    let p = with.registry().get("echo").expect("listed");
    assert!(!p.procedural());
    assert_eq!(p.extensions(), ["txt"]);
}

/// The plugin gets the request, the seed and where to write; its file is the output and its
/// stdout's `info` is reported. A failing plugin's exit code and stderr are reported.
#[test]
fn plugin_provider_runs_out_of_process() {
    let dir = tempfile::tempdir().unwrap();
    let module = GenModule::new(&GenConfig {
        plugin: vec![echo()],
        ..GenConfig::default()
    });
    let p = module.registry().get("echo").unwrap();
    let ctx = GenContext {
        project: dir.path(),
    };
    let out = p.generate(&json!({"text": "hello"}), 9, &ctx).unwrap();
    assert_eq!(out.bytes, b"hello");
    assert_eq!(out.info, json!({"bytes": 5, "seed": 9}));
    let err = p
        .generate(&json!({"fail": true}), 0, &ctx)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("exited with 3") && err.contains("asked to fail"),
        "{err}"
    );
}
