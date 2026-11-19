//! Role files: `studio/agents/<role>.md`, YAML frontmatter plus a Markdown body (M3 phase 3.3).

mod common;

use concordia_studio::{RoleColor, RoleFile, RoleId, RoleSpec, StudioError, default_roster};
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
fn color_must_be_a_claude_code_name_or_a_hex_rgb() {
    let with = |color: &str| {
        common::role("writer", &["task_list"], &["content/**"])
            .replace("mcp_tools:", &format!("color: \"{color}\"\nmcp_tools:"))
    };
    for good in ["red", "cyan", "#a1B2c3"] {
        let role = RoleFile::parse(&with(good)).unwrap_or_else(|e| panic!("{good}: {e}"));
        assert_eq!(
            role.spec.color.map(|c| c.to_string()),
            Some(good.to_owned())
        );
    }
    for bad in ["teal", "Red", "#12345", "#gggggg", ""] {
        let err = RoleFile::parse(&with(bad)).unwrap_err().to_string();
        assert!(
            err.contains("red, blue, green, yellow, purple, orange, pink or cyan")
                && err.contains("#rrggbb"),
            "{bad:?}: {err}"
        );
    }
}

#[test]
fn a_hex_color_maps_to_the_nearest_claude_code_color() {
    let cases = [
        ("#ff7f50", "orange"),
        ("#1e90ff", "blue"),
        ("#00ced1", "cyan"),
        ("#ff69b4", "pink"),
        ("#228b22", "green"),
        ("#8a2be2", "purple"),
    ];
    for (hex, name) in cases {
        let color: RoleColor = hex.parse().unwrap();
        assert_eq!(color.agent_color(), name, "{hex}");
        assert_eq!(color.display_hex(), hex, "{hex}");
    }
    let named: RoleColor = "green".parse().unwrap();
    assert_eq!(named.agent_color(), "green");
    assert!(named.display_hex().starts_with('#') && named.display_hex().len() == 7);
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
        prop::option::of(prop_oneof![
            Just("red".to_owned()),
            Just("cyan".to_owned()),
            "#[0-9a-f]{6}"
        ]),
    )
        .prop_map(
            |(id, display, description, count, tools, write, turns, color)| {
                let mut spec = RoleSpec::new(RoleId::new(&id).unwrap(), display, description);
                spec.count = std::num::NonZeroU32::new(count).unwrap();
                spec.mcp_tools = tools;
                spec.paths.write = write;
                spec.max_turns = turns.and_then(std::num::NonZeroU32::new);
                spec.color = color.map(|c| c.parse().unwrap());
                spec
            },
        )
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
