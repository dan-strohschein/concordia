//! `studio/team.toml` and the resolved roster (M3 phase 3.3).

mod common;

use concordia_studio::{GateOn, GateParty, RoleFile, Roster, StudioError, TaskStatus, Team};

fn role_files() -> Vec<RoleFile> {
    common::roles()
        .into_iter()
        .map(|(_, text)| RoleFile::parse(&text).unwrap())
        .collect()
}

fn resolve(team: &str) -> Result<Roster, StudioError> {
    Roster::resolve(&Team::parse(team)?, role_files(), &common::catalog())
}

#[test]
fn team_count_overrides_role_default() {
    let roster = resolve(common::TEAM).unwrap();
    assert_eq!(roster.role("level-designer").unwrap().count.get(), 3);
    assert_eq!(roster.role("qa-playtester").unwrap().count.get(), 1);
}

#[test]
fn disabled_role_is_excluded_from_roster() {
    let roster = resolve(common::TEAM).unwrap();
    assert!(roster.role("writer").is_none());
    let disabled = roster.disabled();
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].role.as_str(), "writer");
    assert_eq!(disabled[0].reason, "no text assets yet");
}

#[test]
fn director_role_must_exist_and_be_enabled() {
    let missing = common::TEAM.replace("director = \"director\"", "director = \"boss\"");
    assert!(matches!(
        resolve(&missing),
        Err(StudioError::Director { .. })
    ));
    let disabled = format!("{}\n[roles.director]\nenabled = false\n", common::TEAM);
    assert!(matches!(
        resolve(&disabled),
        Err(StudioError::Director { .. })
    ));
}

#[test]
fn team_override_for_unknown_role_is_rejected_with_suggestion() {
    let typo = format!("{}\n[roles.level-designr]\ncount = 2\n", common::TEAM);
    let err = resolve(&typo).unwrap_err();
    assert!(err.to_string().contains("level-designer"), "{err}");
}

#[test]
fn gate_paths_and_transitions_parse() {
    let team = Team::parse(common::TEAM).unwrap();
    assert_eq!(
        team.gates[0].on,
        GateOn::TaskTransition {
            from: TaskStatus::in_review,
            to: TaskStatus::done
        }
    );
    assert_eq!(
        team.gates[0].require_any,
        [
            GateParty::Role("qa-playtester".parse().unwrap()),
            GateParty::Human
        ]
    );
    assert_eq!(
        team.gates[1].on,
        GateOn::Paths(vec!["studio/**".into(), ".claude/**".into()])
    );
}

#[test]
fn malformed_gate_transition_is_rejected() {
    let bad = common::TEAM.replace("in_review->done", "in_review=>done");
    assert!(Team::parse(&bad).is_err());
}

#[test]
fn escalation_defaults_to_three_attempts() {
    assert_eq!(
        Team::parse(common::TEAM)
            .unwrap()
            .escalation
            .max_attempts_per_task
            .get(),
        3
    );
}
