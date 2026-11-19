//! Role files: `studio/agents/<role>.md`, YAML frontmatter plus a Markdown body (M3 phase 3.3).

mod common;

use concordia_studio::{RoleFile, RoleId, RoleSpec, StudioError, default_roster};
use proptest::prelude::*;

#[test]
fn default_roster_role_files_all_parse_and_validate() {
    let roster = default_roster();
    assert_eq!(roster.roles.len(), 12);
    for (stem, text) in roster.roles {
        RoleFile::parse(text)
            .and_then(|role| role.validate(stem, &common::catalog()).map(|()| role))
            .unwrap_or_else(|e| panic!("{stem}: {e}"));
    }
}

#[test]
fn role_id_must_match_file_stem() {
    let text = common::role("level-designer", &["scene_*"], &["scenes/**"]);
    let role = RoleFile::parse(&text).unwrap();
    let err = role.validate("levels", &common::catalog()).unwrap_err();
    assert!(matches!(err, StudioError::RoleStem { .. }), "{err}");
    assert!(err.to_string().contains("levels.md"), "{err}");
}

#[test]
fn unknown_frontmatter_key_is_a_validation_error_with_suggestion() {
    let text = common::role("writer", &["task_list"], &["content/**"])
        .replace("description:", "descripton:");
    let err = RoleFile::parse(&text).unwrap_err();
    match &err {
        StudioError::UnknownKey {
            key, did_you_mean, ..
        } => {
            assert_eq!(key, "descripton");
            assert_eq!(did_you_mean.as_deref(), Some("description"));
        }
        other => panic!("expected UnknownKey, got {other}"),
    }
}

#[test]
fn mcp_tool_scope_globs_must_match_at_least_one_known_tool() {
    let text = common::role("level-designer", &["scnee_*", "query_run"], &["scenes/**"]);
    let err = RoleFile::parse(&text)
        .unwrap()
        .validate("level-designer", &common::catalog())
        .unwrap_err();
    match &err {
        StudioError::UnknownTools { patterns, .. } => assert_eq!(patterns, &["scnee_*"]),
        other => panic!("expected UnknownTools, got {other}"),
    }
}

#[test]
fn write_paths_outside_project_root_are_rejected() {
    for bad in ["../**", "/etc/**", "scenes/../../x"] {
        let text = common::role("level-designer", &["scene_*"], &[bad]);
        let err = RoleFile::parse(&text)
            .unwrap()
            .validate("level-designer", &common::catalog())
            .unwrap_err();
        assert!(
            matches!(err, StudioError::PathOutsideProject { .. }),
            "{bad}: {err}"
        );
    }
}

#[test]
fn missing_frontmatter_fence_is_a_parse_error() {
    let err = RoleFile::parse("# just markdown\n").unwrap_err();
    assert!(matches!(err, StudioError::Frontmatter { .. }), "{err}");
}

fn arb_spec() -> impl Strategy<Value = RoleSpec> {
    (
        "[a-z][a-z0-9]{0,6}(-[a-z0-9]{1,5}){0,2}",
        "[A-Za-z][A-Za-z ]{0,18}",
        "[A-Za-z][A-Za-z .,:'-]{0,80}",
        1u32..5,
        prop::collection::vec("[a-z]{1,8}(_[a-z]{1,8})?", 0..4),
        prop::collection::vec("[a-z]{1,6}/\\*\\*", 0..3),
        prop::option::of(1u32..200),
    )
        .prop_map(|(id, display, description, count, tools, write, turns)| {
            let mut spec = RoleSpec::new(RoleId::new(&id).unwrap(), display, description);
            spec.count = std::num::NonZeroU32::new(count).unwrap();
            spec.mcp_tools = tools;
            spec.paths.write = write;
            spec.max_turns = turns.and_then(std::num::NonZeroU32::new);
            spec
        })
}

proptest! {
    #[test]
    fn prop_role_file_roundtrip_canonical(spec in arb_spec(), body in "[ -~\n]{0,120}") {
        let role = RoleFile::new(spec, body);
        let text = role.to_text();
        let parsed = RoleFile::parse(&text).unwrap();
        prop_assert_eq!(&parsed, &role);
        prop_assert_eq!(parsed.to_text(), text);
    }
}
