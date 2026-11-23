//! Audio tools over MCP (M2 phase 2.7a.2): the five `audio_*` tools are served with generated
//! schemas when `[audio]` is on, none mutates the world (each is read-only to MCP), and an agent
//! renders a cue it imported: 0.5 s of DC 0.5 is 24,000 frames at −6.0206 dBFS.

mod common;

use std::io::Cursor;

use common::{call, connect_with, options, text};
use concordia_api::{EngineModule, params_schema, result_schema};
use serde_json::{Value, json};

async fn project() -> (tempfile::TempDir, common::Client) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("concordia.toml"), "[assets]\n\n[audio]\n").unwrap();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = Cursor::new(Vec::new());
    let mut w = hound::WavWriter::new(&mut bytes, spec).unwrap();
    for _ in 0..4800 {
        w.write_sample(16384i16).unwrap();
    }
    w.finalize().unwrap();
    std::fs::create_dir_all(dir.path().join("assets/sfx")).unwrap();
    std::fs::write(dir.path().join("assets/sfx/dc.wav"), bytes.into_inner()).unwrap();
    let mut options = options(&dir);
    options.modules = concordia_app::Project::load(dir.path())
        .and_then(|p| p.modules())
        .unwrap();
    let client = connect_with(options, Default::default()).await;
    (dir, client)
}

#[tokio::test]
async fn audio_methods_have_generated_schemas_and_mcp_tools() {
    let (_dir, client) = project().await;
    let tools = client.list_all_tools().await.unwrap();
    let module = concordia_audio::AudioModule::new(Default::default()).unwrap();
    for method in module.methods() {
        let tool = tools
            .iter()
            .find(|t| t.name == method.tool_name())
            .unwrap_or_else(|| panic!("{} is offered with [audio]", method.name));
        assert_eq!(
            serde_json::to_value(tool.input_schema.as_ref()).unwrap(),
            params_schema(&method)
        );
        assert_eq!(
            serde_json::to_value(tool.output_schema.as_ref().expect("an output schema")).unwrap(),
            result_schema(&method)
        );
        assert_eq!(
            tool.annotations.as_ref().and_then(|a| a.read_only_hint),
            Some(true),
            "{}",
            method.name
        );
    }
}

#[tokio::test]
async fn agent_renders_an_imported_cue_over_mcp() {
    let (dir, client) = project().await;
    let ok = |r: rmcp::model::CallToolResult| -> Value {
        assert_ne!(r.is_error, Some(true), "{}", text(&r));
        serde_json::from_str(&text(&r)).unwrap()
    };
    let sound = ok(call(
        &client,
        "asset_import",
        json!({"path": "assets/sfx/dc.wav"}),
    )
    .await)["id"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::write(
        dir.path().join("assets/sfx/hum.cue.ron"),
        format!("(sounds: [(sound: \"asset:{sound}\")], looping: true)\n"),
    )
    .unwrap();
    let cue = ok(call(
        &client,
        "asset_import",
        json!({"path": "assets/sfx/hum.cue.ron"}),
    )
    .await)["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = ok(call(
        &client,
        "audio_render",
        json!({"cue": format!("asset:{cue}"), "seconds": 0.5}),
    )
    .await);
    assert_eq!(r["frames"], 24_000);
    assert!(
        (r["peak_dbfs"].as_f64().unwrap() + 6.0206).abs() < 1e-3,
        "{r}"
    );
}
