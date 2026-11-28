//! concordia-tests' integration tests and milestone gates in one binary, so the engine links
//! once (2026-11-27). Test names start with their file's module: `just gate-tests M<n>` selects
//! `test(/^m<n>_/)`. `bless_reference` stays its own (release-only) binary.

mod canyon;
mod gi_reference;
mod graph_library;
mod lamplight_content;
mod lamplight_mirrors;
mod lamplight_night;
mod lamplight_showcase;
mod lamplight_snuffer;
mod lamplight_town;
mod lamplight_ui;
mod lamplight_world;
#[path = "../gates/m0_headless_mcp.rs"]
mod m0_headless_mcp;
#[path = "../gates/m0_live_check.rs"]
mod m0_live_check;
#[path = "../gates/m1_live_check.rs"]
mod m1_live_check;
#[path = "../gates/m1_mcp_coverage.rs"]
mod m1_mcp_coverage;
#[path = "../gates/m1_mcp_screenshot.rs"]
mod m1_mcp_screenshot;
#[path = "../gates/m1_physics_golden.rs"]
mod m1_physics_golden;
#[path = "../gates/m2_lamplight.rs"]
mod m2_lamplight;
#[path = "../gates/m3_live_studio.rs"]
mod m3_live_studio;
#[path = "../gates/m3_replay_assert.rs"]
mod m3_replay_assert;
#[path = "../gates/m3_studio_loop_scripted.rs"]
mod m3_studio_loop_scripted;
#[path = "../gates/m3_studio_sync.rs"]
mod m3_studio_sync;
#[path = "../gates/m3s_enforcement.rs"]
mod m3s_enforcement;
#[path = "../gates/m3s_live_check.rs"]
mod m3s_live_check;
#[path = "../gates/m3s_shared_engine.rs"]
mod m3s_shared_engine;
#[path = "../gates/m3s_studio_sync.rs"]
mod m3s_studio_sync;
#[path = "../gates/m3s_task_board.rs"]
mod m3s_task_board;
#[path = "../gates/m3s_team_questions.rs"]
mod m3s_team_questions;
#[path = "../gates/m_minus1_scaffold.rs"]
mod m_minus1_scaffold;
mod reference_blessing;
mod replay_m1_physics;
mod repo_hygiene;
mod snuffer_twin;
mod vsm_reference;
