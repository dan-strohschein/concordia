//! `concordia studio doctor`: is this machine ready to run Agent Studio in Claude Code?
//!
//! The checks live in `concordia_studio::doctor` (shared with `studio.doctor` and the editor's
//! launcher). A Claude subscription login is all Studio needs; an API key is reported, never required.

use std::path::Path;

use concordia_studio::doctor::{self, Check, CheckStatus, DoctorEnv};

use crate::Failure;

pub(crate) fn run(project: &Path) -> Result<(), Failure> {
    let mut checks = doctor::machine_checks(&DoctorEnv::from_process());
    checks.extend(in_sync(project));
    let failed = checks
        .iter()
        .filter(|c| c.status == CheckStatus::fail)
        .count();
    checks.iter().for_each(print);
    match failed {
        0 => Ok(()),
        n => Err(Failure {
            code: 1,
            message: format!("{n} check(s) failed"),
        }),
    }
}

fn print(check: &Check) {
    let tag = match check.status {
        CheckStatus::ok => "ok  ",
        CheckStatus::note => "note",
        CheckStatus::fail => "FAIL",
    };
    println!("{tag}  {}", check.text);
}

/// The freshness check, for Studio projects only.
fn in_sync(project: &Path) -> Option<Check> {
    let manifest = concordia_app::Project::load(project).ok()?.manifest;
    manifest.studio.as_ref()?;
    let mut engine = super::project_engine(project).ok()?;
    let session = engine.open_session();
    let params = serde_json::json!({"check": true, "actor": "human:local"});
    let stale = engine
        .call(session, "studio.sync", params)
        .map(|report| report["stale"] != false)
        .map_err(|e| e.message);
    Some(doctor::sync_check(stale))
}
