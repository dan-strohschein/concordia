//! Per-role tool budgets (ADR-0073): each default role's generated tool definitions stay small.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::{connect_with, options};
use concordia_studio::{Roster, SyncOptions, ToolCatalog, sync};
use rmcp::model::ClientConfig;
use serde_json::Value;

/// The most tokens (4 chars each) one role's engine tool definitions may take (ADR-0073, raised
/// from 22k to 50k on 2026-11-21 until the tool schemas are slimmed).
const ROLE_TOOL_BUDGET: usize = 50_000;

/// The roles the default team enables.
const ENABLED: [&str; 10] = [
    "animator",
    "composer",
    "director",
    "gameplay-programmer",
    "level-designer",
    "qa-playtester",
    "sound-designer",
    "tech-artist",
    "ui-ux",
    "writer",
];

/// Every module a Studio project can enable, so every role's tools are served.
const MODULES: &str = "[input]\nmap = \"input/default.input.ron\"\n\n[physics]\n\n[nav]\n\n[assets]\n\n[anim]\n\n[audio]\n\n[ui]\n\n[render]\n\n[scripts]\n\n[replay]\n\n[studio]\n";

/// The served tool definitions of a project with the default roster, by name.
async fn served_tools(dir: &tempfile::TempDir) -> BTreeMap<String, Value> {
    concordia_studio::init(dir.path(), Path::new("studio")).unwrap();
    std::fs::write(dir.path().join("concordia.toml"), MODULES).unwrap();
    std::fs::create_dir_all(dir.path().join("input")).unwrap();
    let starter = concordia_input::InputMap::starter().to_ron().unwrap();
    std::fs::write(dir.path().join("input/default.input.ron"), starter).unwrap();
    let mut opts = options(dir);
    opts.modules = concordia_app::Project::load(dir.path())
        .unwrap()
        .modules()
        .unwrap();
    opts.studio = Some("studio".into());
    let client = connect_with(opts, ClientConfig::default()).await;
    let tools = client.list_all_tools().await.unwrap();
    tools
        .iter()
        .map(|t| (t.name.to_string(), serde_json::to_value(t).unwrap()))
        .collect()
}

/// Runs `studio sync` and returns each generated agent's engine tool names.
fn synced_role_tools(root: &Path, catalog: &ToolCatalog) -> BTreeMap<String, Vec<String>> {
    let roster = Roster::load(&root.join("studio"), catalog).unwrap();
    sync(
        root,
        Path::new("studio"),
        &roster,
        catalog,
        SyncOptions::default(),
    )
    .unwrap();
    std::fs::read_dir(root.join(".claude/agents"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            let role = path.file_stem().unwrap().to_string_lossy().into_owned();
            (role, engine_tools(&text))
        })
        .collect()
}

/// The `mcp__concordia__*` names on an agent file's `tools:` line.
fn engine_tools(agent: &str) -> Vec<String> {
    let line = agent
        .lines()
        .find_map(|l| l.strip_prefix("tools: "))
        .unwrap_or_else(|| panic!("no tools line:\n{agent}"));
    line.split(", ")
        .filter_map(|t| t.trim().strip_prefix("mcp__concordia__"))
        .map(str::to_owned)
        .collect()
}

/// About how many tokens the named tools' definitions take.
fn tokens(tools: &[String], served: &BTreeMap<String, Value>) -> usize {
    tools
        .iter()
        .map(|name| {
            let def = served
                .get(name)
                .unwrap_or_else(|| panic!("{name} is not served"));
            serde_json::to_string(def).unwrap().len() / 4
        })
        .sum()
}

#[tokio::test]
async fn every_enabled_default_role_stays_under_its_tool_budget() {
    let dir = tempfile::tempdir().unwrap();
    let served = served_tools(&dir).await;
    let catalog = ToolCatalog::from_names(served.keys().cloned());
    let roles = synced_role_tools(dir.path(), &catalog);
    assert_eq!(
        roles.keys().map(String::as_str).collect::<Vec<_>>(),
        ENABLED
    );
    for (role, tools) in &roles {
        assert!(!tools.is_empty(), "{role} has engine tools");
    }
    let sizes: Vec<(String, usize, usize)> = roles
        .iter()
        .map(|(role, tools)| (role.clone(), tools.len(), tokens(tools, &served)))
        .collect();
    for (role, count, size) in &sizes {
        println!("{role}: {count} tools, about {size} tokens");
    }
    let over: Vec<String> = sizes
        .iter()
        .filter(|(_, _, size)| *size >= ROLE_TOOL_BUDGET)
        .map(|(role, count, size)| format!("{role}: {count} tools, about {size} tokens"))
        .collect();
    assert!(
        over.is_empty(),
        "over the {ROLE_TOOL_BUDGET}-token role budget: {over:?} (all roles: {sizes:?})"
    );
}

#[tokio::test]
async fn animator_is_enabled_with_anim_tools_under_budget() {
    let dir = tempfile::tempdir().unwrap();
    let served = served_tools(&dir).await;
    let catalog = ToolCatalog::from_names(served.keys().cloned());
    let roles = synced_role_tools(dir.path(), &catalog);
    let animator = roles.get("animator").expect("the animator is enabled");
    for tool in [
        "anim_inspect",
        "asset_import",
        "component_patch",
        "capture_screenshot",
        "sim_step",
        "scene_new",
    ] {
        assert!(animator.iter().any(|t| t == tool), "{tool}: {animator:?}");
    }
    let size = tokens(animator, &served);
    println!("animator: {} tools, about {size} tokens", animator.len());
    assert!(
        size < ROLE_TOOL_BUDGET,
        "the animator's {} tools take about {size} tokens",
        animator.len()
    );
}

#[tokio::test]
async fn sound_designer_and_composer_are_enabled_with_audio_tools_under_budget() {
    let dir = tempfile::tempdir().unwrap();
    let served = served_tools(&dir).await;
    let catalog = ToolCatalog::from_names(served.keys().cloned());
    let roles = synced_role_tools(dir.path(), &catalog);
    for (role, needed) in [
        (
            "sound-designer",
            &[
                "audio_play",
                "audio_render",
                "audio_buses",
                "audio_inspect",
                "component_set",
                "asset_import",
            ][..],
        ),
        (
            "composer",
            &[
                "audio_play",
                "audio_render",
                "audio_buses",
                "component_set",
                "asset_import",
            ][..],
        ),
    ] {
        let tools = roles
            .get(role)
            .unwrap_or_else(|| panic!("{role} is enabled (A1)"));
        for tool in needed {
            assert!(tools.iter().any(|t| t == tool), "{role}: {tool}: {tools:?}");
        }
        let size = tokens(tools, &served);
        println!("{role}: {} tools, about {size} tokens", tools.len());
        assert!(
            size < ROLE_TOOL_BUDGET,
            "{role}'s {} tools take about {size} tokens",
            tools.len()
        );
    }
}

#[tokio::test]
async fn ui_ux_is_enabled_with_ui_tools_under_budget() {
    let dir = tempfile::tempdir().unwrap();
    let served = served_tools(&dir).await;
    let catalog = ToolCatalog::from_names(served.keys().cloned());
    let roles = synced_role_tools(dir.path(), &catalog);
    let tools = roles
        .get("ui-ux")
        .expect("the UI/UX designer is enabled (2.7b.1)");
    for tool in [
        "ui_inspect",
        "capture_screenshot",
        "entity_spawn",
        "component_set",
        "asset_import",
        "txn_begin",
    ] {
        assert!(tools.iter().any(|t| t == tool), "ui-ux: {tool}: {tools:?}");
    }
    let size = tokens(tools, &served);
    println!("ui-ux: {} tools, about {size} tokens", tools.len());
    assert!(
        size < ROLE_TOOL_BUDGET,
        "ui-ux's {} tools take about {size} tokens",
        tools.len()
    );
}
