//! The default roster's prompts and scopes (M3 phase 3.6).

mod common;

use std::path::Path;

use concordia_studio::{RoleFile, Roster, default_roster, init};

/// The default roster, initialized into a temp project and resolved.
fn roster() -> (tempfile::TempDir, Roster) {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), Path::new("studio")).unwrap();
    let roster = Roster::load(&dir.path().join("studio"), &common::catalog()).unwrap();
    (dir, roster)
}

/// Every default role file, enabled or not.
fn role_files() -> Vec<RoleFile> {
    default_roster()
        .roles
        .iter()
        .map(|(id, text)| RoleFile::parse(text).unwrap_or_else(|e| panic!("{id}: {e}")))
        .collect()
}

const SECTIONS: [&str; 7] = [
    "## Mission",
    "## You own",
    "## You never",
    "## Workflow",
    "## How you verify",
    "## When to escalate",
    "## Output contract",
];

#[test]
fn default_roster_has_exactly_the_twelve_design_roles() {
    let mut ids: Vec<&str> = default_roster().roles.iter().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [
            "animator",
            "composer",
            "director",
            "gameplay-programmer",
            "level-designer",
            "modeler",
            "qa-playtester",
            "sound-designer",
            "tech-artist",
            "ui-ux",
            "vfx-artist",
            "writer"
        ]
    );
}

#[test]
fn every_role_prompt_has_required_sections_in_order() {
    for role in role_files() {
        let positions: Vec<Option<usize>> = SECTIONS.iter().map(|s| role.body.find(s)).collect();
        for (section, at) in SECTIONS.iter().zip(&positions) {
            assert!(at.is_some(), "{}: missing {section}", role.spec.role);
        }
        let found: Vec<usize> = positions.into_iter().flatten().collect();
        assert!(
            found.windows(2).all(|w| w[0] < w[1]),
            "{}: sections out of order",
            role.spec.role
        );
    }
}

#[test]
fn every_output_contract_has_a_worked_example() {
    for role in role_files() {
        let contract = &role.body[role.body.find("## Output contract").unwrap()..];
        assert!(
            contract.contains("Example") && contract.contains("```"),
            "{}: the output contract needs a fenced worked example",
            role.spec.role
        );
    }
}

#[test]
fn every_role_description_is_specific_enough() {
    for role in role_files() {
        let d = &role.spec.description;
        assert!(
            (40..=400).contains(&d.chars().count()),
            "{}: description is {} chars",
            role.spec.role,
            d.chars().count()
        );
        for placeholder in ["TODO", "TBD", "placeholder", "arrive in M3 phase 3.6"] {
            assert!(!d.contains(placeholder), "{}: {d}", role.spec.role);
            assert!(
                !role.body.contains(placeholder),
                "{}: the body still has {placeholder:?}",
                role.spec.role
            );
        }
    }
}

/// The literal part of a path pattern, before its first wildcard.
fn literal_prefix(pattern: &str) -> &str {
    let end = pattern.find(['*', '?', '[', '{']).unwrap_or(pattern.len());
    &pattern[..end]
}

fn overlaps(a: &str, b: &str) -> bool {
    let (a, b) = (literal_prefix(a), literal_prefix(b));
    a.starts_with(b) || b.starts_with(a)
}

#[test]
fn no_two_roles_share_write_paths_except_declared_overlaps() {
    let (_dir, roster) = roster();
    let shared = &roster.team.paths.shared;
    let roles = role_files();
    for (i, a) in roles.iter().enumerate() {
        for b in &roles[i + 1..] {
            for pa in &a.spec.paths.write {
                for pb in &b.spec.paths.write {
                    if !overlaps(pa, pb) {
                        continue;
                    }
                    let declared = shared.iter().any(|s| s == pa || s == pb);
                    assert!(
                        declared,
                        "{} writes {pa} and {} writes {pb}: declare the overlap in team.toml [paths] shared",
                        a.spec.role, b.spec.role
                    );
                }
            }
        }
    }
}

#[test]
fn qa_role_has_no_write_access_to_code_or_content_paths() {
    let qa = role_files()
        .into_iter()
        .find(|r| r.spec.role.as_str() == "qa-playtester")
        .unwrap();
    for pattern in &qa.spec.paths.write {
        assert!(
            pattern.starts_with("tests/replays/") || pattern.starts_with("tests/playtests/"),
            "QA may write only evidence: {pattern}"
        );
    }
    assert!(
        qa.spec
            .harness_tools
            .iter()
            .all(|t| !matches!(t, concordia_studio::HarnessTool::Bash)),
        "QA has no shell"
    );
}

#[test]
fn every_skill_referenced_by_a_role_exists() {
    let skills: Vec<&str> = default_roster().skills.iter().map(|(n, _)| *n).collect();
    for role in role_files() {
        for skill in &role.spec.skills {
            assert!(
                skills.contains(&skill.as_str()),
                "{} preloads unknown skill {skill}",
                role.spec.role
            );
        }
    }
}

#[test]
fn a_role_naming_a_missing_skill_is_rejected_at_load() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), Path::new("studio")).unwrap();
    let file = dir.path().join("studio/agents/writer.md");
    let text = std::fs::read_to_string(&file)
        .unwrap()
        .replace("skills: [", "skills: [\"concordia-no-such-skill\", ");
    assert!(
        text.contains("concordia-no-such-skill"),
        "writer preloads skills"
    );
    std::fs::write(&file, text).unwrap();
    let err = Roster::load(&dir.path().join("studio"), &common::catalog()).unwrap_err();
    assert!(err.to_string().contains("concordia-no-such-skill"), "{err}");
}

#[test]
fn prompts_never_mention_api_keys_except_to_say_none_needed() {
    let texts = default_roster()
        .roles
        .iter()
        .chain(default_roster().skills.iter())
        .map(|(name, text)| (*name, *text));
    for (name, text) in texts {
        for line in text.lines() {
            let lower = line.to_lowercase();
            if lower.contains("api key") || line.contains("ANTHROPIC_API_KEY") {
                assert!(
                    lower.contains("no api key") || lower.contains("never need"),
                    "{name}: {line}"
                );
            }
        }
    }
}

#[test]
fn gameplay_and_tech_artist_prompts_carry_the_clean_room_rule() {
    for role in role_files() {
        if ["gameplay-programmer", "tech-artist"].contains(&role.spec.role.as_str()) {
            assert!(
                role.body.contains(CLEAN_ROOM),
                "{}: the clean-room rule, verbatim",
                role.spec.role
            );
        }
    }
}

/// The clean-room rule as `CLAUDE.md` states it.
const CLEAN_ROOM: &str = "Never copy, port or translate code from Unreal Engine or any other source-available-but-not-open-source engine.";

#[test]
fn enabled_role_prompts_name_only_tools_in_their_scope() {
    let (_dir, roster) = roster();
    for role in roster.roles() {
        for name in mentioned_tools(&role.body) {
            assert!(
                role.mcp_tools.contains(&name),
                "{} mentions mcp__concordia__{name}, which it cannot call",
                role.role
            );
        }
    }
}

/// `mcp__concordia__<tool>` names in a text.
fn mentioned_tools(text: &str) -> Vec<String> {
    text.match_indices("mcp__concordia__")
        .map(|(i, prefix)| {
            text[i + prefix.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect()
        })
        .collect()
}

#[test]
fn every_enabled_role_can_ask_answer_and_read_questions() {
    let (_dir, roster) = roster();
    for role in roster.roles() {
        for tool in ["studio_ask", "studio_answer", "studio_questions"] {
            assert!(
                role.mcp_tools.iter().any(|t| t == tool),
                "{} lacks {tool}",
                role.role
            );
        }
        assert!(
            role.skills.iter().any(|s| s == "concordia-team-comms"),
            "{} preloads concordia-team-comms",
            role.role
        );
        let escalates = role.mcp_tools.iter().any(|t| t == "studio_escalate");
        assert_eq!(escalates, role.role.as_str() == "director", "{}", role.role);
    }
}

#[test]
fn director_prompt_routes_questions_and_escalates_only_when_no_role_can_answer() {
    let director = role_files()
        .into_iter()
        .find(|r| r.spec.role.as_str() == "director")
        .unwrap();
    for needle in [
        "mcp__concordia__studio_escalate",
        "mcp__concordia__studio_answer",
        "never guess a design decision",
        "## Routing questions",
    ] {
        assert!(
            director.body.contains(needle),
            "director prompt lacks {needle:?}"
        );
    }
}

#[test]
fn every_default_role_has_a_color_and_enabled_roles_differ() {
    for role in role_files() {
        assert!(role.spec.color.is_some(), "{} has no color", role.spec.role);
    }
    let (_dir, roster) = roster();
    let mut colors: Vec<&str> = roster
        .roles()
        .iter()
        .map(|r| r.color.as_ref().map_or("", |c| c.agent_color()))
        .collect();
    colors.sort_unstable();
    let count = colors.len();
    colors.dedup();
    assert_eq!(
        colors.len(),
        count,
        "enabled roles share a color: {colors:?}"
    );
}

#[test]
fn level_designer_places_crowds_with_terra_tools_templates_and_the_template_skill() {
    let (_dir, roster) = roster();
    let role = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "level-designer")
        .expect("the level designer is enabled");
    for tool in [
        "terra_spawn_batch",
        "terra_despawn_batch",
        "terra_stats",
        "terra_inspect",
    ] {
        assert!(role.mcp_tools.iter().any(|t| t == tool), "{tool}");
    }
    assert!(role.skills.iter().any(|s| s == "concordia-terra-templates"));
    assert!(
        role.write.iter().any(|p| p == "templates/**"),
        "{:?}",
        role.write
    );
    for needle in [
        "Spawner",
        "mcp__concordia__terra_spawn_batch",
        ".template.ron",
    ] {
        assert!(role.body.contains(needle), "the prompt explains {needle}");
    }
}

#[test]
fn gameplay_programmer_writes_and_reloads_scripts_with_the_scripting_skill() {
    let (_dir, roster) = roster();
    let role = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "gameplay-programmer")
        .expect("the gameplay programmer is enabled");
    assert!(
        role.mcp_tools.iter().any(|t| t == "script_reload"),
        "{:?}",
        role.mcp_tools
    );
    assert!(
        role.skills.iter().any(|s| s == "concordia-scripting"),
        "{:?}",
        role.skills
    );
    assert!(
        role.write.iter().any(|p| p == "scripts/**"),
        "{:?}",
        role.write
    );
    for needle in [
        "mcp__concordia__script_reload",
        "export_module!",
        "cargo build --target wasm32-wasip2",
        "never builds",
    ] {
        assert!(role.body.contains(needle), "the prompt explains {needle}");
    }
    assert!(
        !role.body.contains("Disabled in the default team"),
        "{}",
        role.body
    );
}

#[test]
fn qa_playtester_records_and_runs_replays_with_the_replay_skill() {
    let (_dir, roster) = roster();
    let qa = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "qa-playtester")
        .expect("QA is enabled");
    // Scopes are loaded with their globs expanded against the project's tools.
    let replays = |tools: &[String]| {
        ["replay_record", "replay_run"]
            .iter()
            .all(|t| tools.iter().any(|x| x == t))
    };
    assert!(replays(&qa.mcp_tools), "{:?}", qa.mcp_tools);
    assert!(
        qa.skills.iter().any(|s| s == "concordia-record-replay"),
        "{:?}",
        qa.skills
    );
    for needle in ["mcp__concordia__replay_run", "tests/replays/"] {
        assert!(qa.body.contains(needle), "the prompt explains {needle}");
    }
    for role in ["gameplay-programmer", "level-designer"] {
        let found = roster
            .roles()
            .iter()
            .find(|r| r.role.as_str() == role)
            .unwrap_or_else(|| panic!("{role} is enabled"));
        assert!(replays(&found.mcp_tools), "{role}: {:?}", found.mcp_tools);
    }
    let programmer = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "gameplay-programmer")
        .unwrap();
    assert!(
        programmer
            .skills
            .iter()
            .any(|s| s == "concordia-fix-bug-with-replay"),
        "{:?}",
        programmer.skills
    );
}
