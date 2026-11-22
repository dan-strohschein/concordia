//! Shared fixtures: a small team, its role files, and an engine with the Studio module.
#![allow(dead_code)]

pub mod hook;
pub mod sync;

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use concordia_api::{ApiError, Engine, EngineConfig};
use concordia_core::id::SessionId;
use concordia_studio::{StudioModule, ToolCatalog};
use serde_json::{Value, json};

/// Tools a `concordia new` project offers (core + physics, assets, anim, render and Studio).
pub const PROJECT_TOOLS: &[&str] = &[
    "anim_inspect",
    "asset_import",
    "asset_list",
    "capture_screenshot",
    "component_add",
    "component_get",
    "component_patch",
    "component_remove",
    "component_set",
    "entity_create",
    "entity_despawn",
    "entity_duplicate",
    "entity_get",
    "entity_reparent",
    "entity_spawn",
    "entity_templates",
    "log_tail",
    "material_get",
    "material_set",
    "physics_raycast",
    "prefab_instantiate",
    "project_files",
    "query_run",
    "render_stats",
    "replay_record",
    "replay_run",
    "scene_info",
    "scene_new",
    "scene_open",
    "scene_save",
    "schema_get",
    "script_reload",
    "sim_pause",
    "sim_play",
    "sim_step",
    "sim_stop",
    "studio_activity",
    "studio_answer",
    "studio_ask",
    "studio_escalate",
    "studio_questions",
    "studio_roster",
    "studio_status",
    "terra_despawn_batch",
    "terra_inspect",
    "terra_spawn_batch",
    "terra_stats",
    "task_approve",
    "task_claim",
    "task_comment",
    "task_create",
    "task_get",
    "task_list",
    "task_reject",
    "task_update",
    "txn_abort",
    "txn_begin",
    "txn_commit",
    "txn_history",
    "txn_redo",
    "txn_undo",
    "txn_undo_to",
];

/// The catalog of [`PROJECT_TOOLS`].
pub fn catalog() -> ToolCatalog {
    ToolCatalog::from_names(PROJECT_TOOLS.iter().copied())
}

pub const TEAM: &str = r#"[team]
name = "test"
director = "director"

[roles.level-designer]
count = 3

[roles.writer]
enabled = false
reason = "no text assets yet"

[[gates]]
on = { task_transition = "in_review->done" }
require_any = ["role:qa-playtester", "human"]

[[gates]]
on = { paths = ["studio/**", ".claude/**"] }
require_any = ["human"]
"#;

/// A role file with the given id, tools and write paths.
pub fn role(id: &str, tools: &[&str], write: &[&str]) -> String {
    format!(
        "---\nrole: {id}\ndisplay_name: {id}\ndescription: The {id} role, used by the Studio test fixtures to check parsing and scopes.\nmcp_tools: {tools:?}\npaths:\n  write: {write:?}\n---\n# {id}\nInstructions.\n"
    )
}

/// The fixture roles: director, level-designer, qa-playtester, writer.
pub fn roles() -> Vec<(&'static str, String)> {
    vec![
        (
            "director",
            role(
                "director",
                &["task_*", "studio_*", "query_run"],
                &["studio/tasks/**"],
            ),
        ),
        (
            "level-designer",
            role(
                "level-designer",
                &["scene_*", "entity_*", "component_*", "task_*"],
                &["scenes/**"],
            ),
        ),
        (
            "qa-playtester",
            role(
                "qa-playtester",
                &["query_run", "scene_info", "task_*"],
                &["tests/playtests/**"],
            ),
        ),
        ("writer", role("writer", &["task_list"], &["content/**"])),
    ]
}

/// Writes `studio/team.toml` and `studio/agents/*.md` under `root`.
pub fn write_studio(root: &Path) {
    let agents = root.join("studio/agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(root.join("studio/team.toml"), TEAM).unwrap();
    for (stem, text) in roles() {
        std::fs::write(agents.join(format!("{stem}.md")), text).unwrap();
    }
}

/// An engine on `root` with only the Studio module and deterministic ids.
pub fn engine(root: &Path) -> Engine {
    let mut config = EngineConfig::new(root);
    config.threads = NonZeroUsize::MIN;
    config.deterministic_ids = Some(7);
    config.modules = vec![Arc::new(StudioModule::new(root, "studio"))];
    Engine::new(config).unwrap()
}

/// A Studio project in a temp dir, its engine and a session.
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub engine: Engine,
    pub session: SessionId,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        write_studio(dir.path());
        let mut engine = engine(dir.path());
        let session = engine.open_session();
        Self {
            dir,
            engine,
            session,
        }
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, ApiError> {
        self.engine.call(self.session, method, params)
    }

    pub fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params.clone())
            .unwrap_or_else(|e| panic!("{method} {params}: {}", e.message))
    }

    /// Creates a level-designer task as the director; returns its id.
    pub fn create(&mut self, title: &str) -> String {
        let created = self.ok(
            "task.create",
            json!({"title": title, "role": "level-designer", "actor": "agent:director"}),
        );
        created["task"]["id"].as_str().unwrap().to_owned()
    }

    /// Moves `id` from todo to in_review as `actor`.
    pub fn submit(&mut self, id: &str, actor: &str) {
        self.ok("task.claim", json!({"id": id, "actor": actor}));
        self.ok(
            "task.update",
            json!({"id": id, "status": "in_review", "actor": actor}),
        );
    }

    pub fn path(&self, relative: &str) -> std::path::PathBuf {
        self.dir.path().join(relative)
    }
}
