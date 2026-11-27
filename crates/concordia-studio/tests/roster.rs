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
    // Claude Code has 8 agent colors and M2 2.7a.2 enables a ninth role, so the generated files
    // may repeat a name once all 8 are used; the exact colors (the editor's) always differ.
    let (_dir, roster) = roster();
    let mut exact: Vec<String> = roster
        .roles()
        .iter()
        .map(|r| r.color.as_ref().map_or(String::new(), |c| c.display_hex()))
        .collect();
    exact.sort_unstable();
    let count = exact.len();
    exact.dedup();
    assert_eq!(exact.len(), count, "enabled roles share a color: {exact:?}");
    let mut named: Vec<&str> = roster
        .roles()
        .iter()
        .map(|r| r.color.as_ref().map_or("", |c| c.agent_color()))
        .collect();
    named.sort_unstable();
    named.dedup();
    assert_eq!(
        named.len(),
        count.min(8),
        "every agent color is used before any repeats: {named:?}"
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
fn level_designer_checks_reachability_with_nav_tools_and_the_navigation_skill() {
    let (_dir, roster) = roster();
    let role = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "level-designer")
        .expect("the level designer is enabled");
    for tool in ["nav_build", "nav_path"] {
        assert!(
            role.mcp_tools.iter().any(|t| t == tool),
            "{tool}: {:?}",
            role.mcp_tools
        );
    }
    assert!(
        role.skills.iter().any(|s| s == "concordia-navigation"),
        "{:?}",
        role.skills
    );
    for needle in ["NavMeshVolume", "mcp__concordia__nav_build", "islands"] {
        assert!(role.body.contains(needle), "the prompt explains {needle}");
    }
}

#[test]
fn qa_clicks_like_a_player_with_input_inject_and_pointer_rays() {
    let (_dir, roster) = roster();
    let role = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "qa-playtester")
        .expect("QA is enabled");
    for tool in ["input_inject", "viewport_pointer_ray", "ui_hit_test"] {
        assert!(
            role.mcp_tools.iter().any(|t| t == tool),
            "{tool}: {:?}",
            role.mcp_tools
        );
    }
    for needle in [
        "mcp__concordia__viewport_pointer_ray",
        "PointerRay",
        "mcp__concordia__input_inject",
        "mcp__concordia__ui_hit_test",
    ] {
        assert!(role.body.contains(needle), "the prompt explains {needle}");
    }
}

#[test]
fn gameplay_programmer_checks_paths_with_nav_path_and_the_nav_agents_skill() {
    let (_dir, roster) = roster();
    let role = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "gameplay-programmer")
        .expect("the gameplay programmer is enabled");
    assert!(
        role.mcp_tools.iter().any(|t| t == "nav_path"),
        "{:?}",
        role.mcp_tools
    );
    assert!(
        !role.mcp_tools.iter().any(|t| t == "nav_build"),
        "building navmeshes stays the level designer's: {:?}",
        role.mcp_tools
    );
    assert!(
        role.skills.iter().any(|s| s == "concordia-nav-agents"),
        "{:?}",
        role.skills
    );
    for needle in ["NavAgent", "NavArrived", "mcp__concordia__nav_path"] {
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
    // The level designer's work never records replays, so it no longer gets them (ADR-0073).
    for (role, has) in [("gameplay-programmer", true), ("level-designer", false)] {
        let found = roster
            .roles()
            .iter()
            .find(|r| r.role.as_str() == role)
            .unwrap_or_else(|| panic!("{role} is enabled"));
        assert_eq!(
            replays(&found.mcp_tools),
            has,
            "{role}: {:?}",
            found.mcp_tools
        );
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

/// The M3 live gate's follow-up: the gameplay programmer had to ask QA to record its proof,
/// because it could not inject input. It now drives its own bug replay, and the skill says how.
#[test]
fn gameplay_programmer_drives_its_own_bug_replay_with_input_inject() {
    let (_dir, roster) = roster();
    let programmer = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "gameplay-programmer")
        .expect("the gameplay programmer is enabled");
    assert!(
        programmer.mcp_tools.iter().any(|t| t == "input_inject"),
        "{:?}",
        programmer.mcp_tools
    );
    let skill = default_roster()
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-fix-bug-with-replay")
        .map(|(_, t)| *t)
        .unwrap();
    let reproduce = skill
        .lines()
        .find(|l| l.starts_with("1. **Reproduce.**"))
        .expect("the skill's first step reproduces the bug");
    assert!(
        reproduce.contains("mcp__concordia__input_inject")
            && reproduce.contains("mcp__concordia__sim_step"),
        "the first step drives the recording itself: {reproduce}"
    );
}

/// Found by the M3 live gate's re-run: once the gameplay programmer handed in its own replay,
/// QA verified it but recorded none. QA's evidence is its own recording, for the whole window.
#[test]
fn qa_records_its_own_evidence_for_the_whole_window() {
    let qa = role_files()
        .into_iter()
        .find(|r| r.spec.role.as_str() == "qa-playtester")
        .unwrap();
    let rule = qa
        .body
        .lines()
        .find(|l| l.starts_with("- Reproducibility:"))
        .expect("QA's reproducibility rule");
    for needle in [
        "always record your own",
        "whole window",
        "not your evidence",
    ] {
        assert!(rule.contains(needle), "{needle}: {rule}");
    }
}

/// A role's `>` note says what is not there yet; one naming a tool the catalog serves is stale.
#[test]
fn role_notes_never_promise_a_tool_that_already_exists() {
    let offered: Vec<String> = common::catalog().names().map(str::to_owned).collect();
    for role in role_files() {
        for note in role.body.lines().filter(|l| l.starts_with("> ")) {
            for tool in &offered {
                assert!(
                    !note.contains(&format!("`{tool}`")),
                    "{}: the note names {tool}, which exists: {note}",
                    role.spec.role
                );
            }
        }
    }
}

#[test]
fn tech_artist_edits_materials_with_the_material_tools_and_skill() {
    let (_dir, roster) = roster();
    let artist = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "tech-artist")
        .expect("the tech artist is enabled");
    for tool in ["material_get", "material_set"] {
        assert!(
            artist.mcp_tools.iter().any(|t| t == tool),
            "{tool}: {:?}",
            artist.mcp_tools
        );
    }
    for needle in [
        "mcp__concordia__material_get",
        "mcp__concordia__material_set",
        "\"name\": \"Material\"",
    ] {
        assert!(artist.body.contains(needle), "the prompt explains {needle}");
    }
    let skill = default_roster()
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-add-material")
        .map(|(_, t)| *t)
        .unwrap();
    for needle in [
        "mcp__concordia__material_set",
        "Vertical(",
        "Horizontal(",
        "Coverage(",
        "haze",
        "fuzz",
        "subsurface",
        "thin_film",
        "closure_budget",
    ] {
        assert!(skill.contains(needle), "the skill explains {needle}");
    }
    for text in [artist.body.as_str(), skill] {
        assert!(!text.contains("MaterialData"), "M1's type is retired");
    }
}

/// The director's section of a shared skill: only the director acts on it.
const DIRECTOR_SECTION: &str = "## For the director";

/// What `skill` tells `role` to do: a skill's director section counts only for the director.
fn skill_text_for<'a>(role: &str, skill: &'a str) -> std::borrow::Cow<'a, str> {
    let Some(start) = skill.find(DIRECTOR_SECTION).filter(|_| role != "director") else {
        return skill.into();
    };
    let rest = &skill[start + DIRECTOR_SECTION.len()..];
    let end = rest
        .find("\n## ")
        .map_or(skill.len(), |i| start + DIRECTOR_SECTION.len() + i);
    format!("{}{}", &skill[..start], &skill[end..]).into()
}

/// The `mcp__concordia__` tools a role's skills tell it to use, that the project offers.
fn skill_tools(role: &RoleFile) -> Vec<String> {
    let offered: Vec<String> = common::catalog().names().map(str::to_owned).collect();
    let skills = default_roster().skills;
    role.spec
        .skills
        .iter()
        .flat_map(|name| {
            let text = skills.iter().find(|(n, _)| n == name).map(|(_, t)| *t);
            let text = text.unwrap_or_else(|| panic!("{}: no skill {name}", role.spec.role));
            mentioned_tools(&skill_text_for(role.spec.role.as_str(), text))
        })
        .filter(|t| offered.contains(t))
        .collect()
}

#[test]
fn default_role_scopes_list_tools_by_name_not_by_glob() {
    for role in role_files() {
        let globs: Vec<&String> = role
            .spec
            .mcp_tools
            .iter()
            .filter(|t| t.contains(['*', '?', '[', '{']))
            .collect();
        assert!(
            globs.is_empty(),
            "{}: name each tool the admin grants, not {globs:?} (ADR-0073)",
            role.spec.role
        );
    }
}

#[test]
fn every_scoped_tool_is_used_by_the_role_prompt_or_its_skills() {
    for role in role_files() {
        let mut used = mentioned_tools(&role.body);
        used.extend(skill_tools(&role));
        let scope = common::catalog()
            .expand(&role.spec.mcp_tools)
            .unwrap_or_else(|e| panic!("{}: {e}", role.spec.role));
        let unused: Vec<&String> = scope.iter().filter(|t| !used.contains(t)).collect();
        assert!(
            unused.is_empty(),
            "{}: neither its prompt nor its skills use {unused:?}",
            role.spec.role
        );
    }
}

#[test]
fn every_tool_a_role_skill_tells_it_to_use_is_in_scope() {
    for role in role_files() {
        let scope = common::catalog()
            .expand(&role.spec.mcp_tools)
            .unwrap_or_else(|e| panic!("{}: {e}", role.spec.role));
        for tool in skill_tools(&role) {
            assert!(
                scope.contains(&tool),
                "{}: a preloaded skill uses mcp__concordia__{tool}, which it cannot call",
                role.spec.role
            );
        }
    }
}

/// The world-inspection tools the director gave up (ADR-0073): specialists attach evidence.
const WORLD_TOOLS: [&str; 8] = [
    "scene_info",
    "entity_get",
    "component_get",
    "query_run",
    "schema_get",
    "capture_screenshot",
    "render_stats",
    "log_tail",
];

#[test]
fn director_coordinates_with_task_team_and_history_tools_only() {
    let (_dir, roster) = roster();
    let director = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "director")
        .expect("the director is enabled");
    for tool in &director.mcp_tools {
        let coordination =
            tool.starts_with("task_") || tool.starts_with("studio_") || tool == "txn_history";
        assert!(
            coordination,
            "the director coordinates; it never calls {tool}"
        );
    }
    for tool in WORLD_TOOLS {
        assert!(
            !director.body.contains(&format!("mcp__concordia__{tool}")),
            "{tool}"
        );
    }
    for needle in [
        "mcp__concordia__task_create",
        "mcp__concordia__txn_history",
        "evidence",
    ] {
        assert!(
            director.body.contains(needle),
            "the prompt explains {needle}"
        );
    }
}

/// The animator's scope (M2 2.4.5, ADR-0073): previews, steering, inspection and the team tools.
const ANIMATOR_TOOLS: [&str; 24] = [
    "anim_inspect",
    "asset_import",
    "asset_list",
    "capture_screenshot",
    "component_patch",
    "component_set",
    "gen_list",
    "gen_run",
    "log_tail",
    "prefab_instantiate",
    "query_run",
    "scene_new",
    "scene_open",
    "scene_save",
    "schema_get",
    "sim_step",
    "sim_stop",
    "studio_answer",
    "studio_ask",
    "studio_questions",
    "task_claim",
    "task_comment",
    "task_get",
    "task_update",
];

#[test]
fn animator_previews_and_steers_graphs_with_its_tools_and_skill() {
    let (_dir, roster) = roster();
    let animator = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "animator")
        .expect("the animator is enabled");
    assert_eq!(animator.mcp_tools, ANIMATOR_TOOLS);
    assert_eq!(
        animator.write,
        [
            "assets/anim/**",
            "assets/blender/**",
            "assets/models/**/*.import.ron"
        ]
    );
    assert_eq!(
        animator.skills,
        [
            "concordia-task-workflow",
            "concordia-animation-graphs",
            "concordia-generate-mesh-blender",
            "concordia-team-comms"
        ]
    );
    for needle in [
        "mcp__concordia__anim_inspect",
        "mcp__concordia__component_patch",
        "\"world\": \"play\"",
        "\"skeleton\": true",
        "assets/anim/previews/",
        ".animgraph.ron",
        "clip_events",
        "mcp__concordia__gen_run",
    ] {
        assert!(
            animator.body.contains(needle),
            "the prompt explains {needle}"
        );
    }
    assert!(
        !animator.body.contains("Disabled in the default team"),
        "{}",
        animator.body
    );
    assert!(
        roster
            .team
            .paths
            .shared
            .iter()
            .any(|p| p == "assets/models/**/*.import.ron"),
        "the modeler and the animator share model sidecars: {:?}",
        roster.team.paths.shared
    );
}

#[test]
fn scenes_are_shared_by_the_level_designer_and_the_tech_artist() {
    let (_dir, roster) = roster();
    assert!(
        roster.team.paths.shared.iter().any(|p| p == "scenes/**"),
        "{:?}",
        roster.team.paths.shared
    );
    // The level designer may write anything under scenes/; the tech artist only scene files.
    for (role, pattern) in [
        ("level-designer", "scenes/**"),
        ("tech-artist", "scenes/**/*.scene.ron"),
    ] {
        let found = roster
            .roles()
            .iter()
            .find(|r| r.role.as_str() == role)
            .unwrap_or_else(|| panic!("{role} is enabled"));
        assert!(
            found.write.iter().any(|p| p == pattern),
            "{role}: {:?}",
            found.write
        );
    }
}

/// The sound designer's scope (M2 2.7a.2, A1): import, wire emitters into scenes, listen, and
/// the team tools.
const SOUND_DESIGNER_TOOLS: [&str; 26] = [
    "asset_import",
    "asset_list",
    "audio_buses",
    "audio_inspect",
    "audio_play",
    "audio_render",
    "audio_stop",
    "component_get",
    "component_patch",
    "component_set",
    "entity_get",
    "gen_list",
    "gen_run",
    "log_tail",
    "query_run",
    "scene_save",
    "schema_get",
    "sim_step",
    "sim_stop",
    "studio_answer",
    "studio_ask",
    "studio_questions",
    "task_claim",
    "task_comment",
    "task_get",
    "task_update",
];

/// The composer's scope (M2 2.7a.2, A1): import music, place a music emitter, listen.
const COMPOSER_TOOLS: [&str; 22] = [
    "asset_import",
    "asset_list",
    "audio_buses",
    "audio_play",
    "audio_render",
    "audio_stop",
    "component_get",
    "component_patch",
    "component_set",
    "gen_list",
    "gen_run",
    "query_run",
    "scene_save",
    "sim_step",
    "sim_stop",
    "studio_answer",
    "studio_ask",
    "studio_questions",
    "task_claim",
    "task_comment",
    "task_get",
    "task_update",
];

#[test]
fn sound_designer_wires_and_listens_with_its_tools_and_skill() {
    let (_dir, roster) = roster();
    let role = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "sound-designer")
        .expect("the sound designer is enabled (A1)");
    assert_eq!(role.mcp_tools, SOUND_DESIGNER_TOOLS);
    assert_eq!(
        role.write,
        ["assets/sfx/**", "assets/audio/**", "scenes/**/*.scene.ron"]
    );
    assert_eq!(
        role.skills,
        [
            "concordia-task-workflow",
            "concordia-sound-design",
            "concordia-synthesize-sfx",
            "concordia-team-comms"
        ]
    );
    for needle in [
        "mcp__concordia__audio_render",
        "mcp__concordia__audio_play",
        "AudioEmitter",
        "PlayCue",
        "AnimEventCues",
        ".cue.ron",
        ".buses.ron",
        "mcp__concordia__gen_run",
    ] {
        assert!(role.body.contains(needle), "the prompt explains {needle}");
    }
    assert!(
        !role.body.contains("Disabled in the default team"),
        "{}",
        role.body
    );
}

const UI_UX_TOOLS: [&str; 23] = [
    "asset_import",
    "asset_list",
    "capture_screenshot",
    "component_get",
    "component_patch",
    "component_set",
    "entity_get",
    "entity_spawn",
    "query_run",
    "scene_open",
    "scene_save",
    "schema_get",
    "sim_step",
    "sim_stop",
    "studio_answer",
    "studio_ask",
    "studio_questions",
    "task_claim",
    "task_comment",
    "task_get",
    "task_update",
    "txn_begin",
    "txn_commit",
];

#[test]
fn ui_ux_builds_screens_with_its_tools_and_skill() {
    let (_dir, roster) = roster();
    let role = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "ui-ux")
        .expect("the UI/UX designer is enabled (2.7b.1, D8)");
    let mut tools: Vec<&str> = role.mcp_tools.iter().map(String::as_str).collect();
    tools.sort_unstable();
    let mut want: Vec<&str> = UI_UX_TOOLS.to_vec();
    want.extend(["ui_inspect", "ui_hit_test", "input_inject"]);
    want.sort_unstable();
    assert_eq!(tools, want);
    assert_eq!(
        role.write,
        [
            "ui/**",
            "assets/ui/**",
            "assets/fonts/**",
            "scenes/**/*.scene.ron"
        ]
    );
    assert_eq!(
        role.skills,
        [
            "concordia-task-workflow",
            "concordia-ui",
            "concordia-team-comms"
        ]
    );
    for needle in [
        "mcp__concordia__ui_inspect",
        "mcp__concordia__ui_hit_test",
        "mcp__concordia__input_inject",
        "mcp__concordia__capture_screenshot",
        "UiButton",
        "UiSlider",
        "UiState",
        "UiRoot",
        "UiNode",
        "UiText",
        ".theme.ron",
        "Anchored",
        "image generator",
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
fn composer_brings_finished_music_with_its_tools_and_skill() {
    let (_dir, roster) = roster();
    let role = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "composer")
        .expect("the composer is enabled (A1)");
    assert_eq!(role.mcp_tools, COMPOSER_TOOLS);
    assert_eq!(role.write, ["assets/music/**", "scenes/**/*.scene.ron"]);
    assert_eq!(
        role.skills,
        [
            "concordia-task-workflow",
            "concordia-music",
            "concordia-team-comms"
        ]
    );
    for needle in [
        "mcp__concordia__audio_render",
        "Music",
        "loop_start",
        "\"stream\"",
        "mcp__concordia__gen_run",
    ] {
        assert!(role.body.contains(needle), "the prompt explains {needle}");
    }
    assert!(
        !role.body.contains("Disabled in the default team"),
        "{}",
        role.body
    );
}

/// The generators reach the roles that make content (M3 3.7c): the modeler (enabled now), the
/// animator (Blender actions), the tech artist (textures), the sound designer (effects) and the
/// composer (scores), each with the skill that uses them. VFX waits for particles (M11).
#[test]
fn content_roles_generate_with_the_gen_tools_and_their_skills() {
    let (_dir, roster) = roster();
    for (role, skill) in [
        ("modeler", "concordia-generate-mesh-blender"),
        ("animator", "concordia-generate-mesh-blender"),
        ("tech-artist", "concordia-add-material"),
        ("sound-designer", "concordia-synthesize-sfx"),
        ("composer", "concordia-music"),
    ] {
        let r = roster
            .role(role)
            .unwrap_or_else(|| panic!("{role} is enabled"));
        for tool in ["gen_list", "gen_run"] {
            assert!(
                r.mcp_tools.iter().any(|t| t == tool),
                "{role}: {:?}",
                r.mcp_tools
            );
        }
        assert!(
            r.skills.iter().any(|s| s == skill),
            "{role}: {:?}",
            r.skills
        );
        assert!(!r.body.contains("Disabled in the default team"), "{role}");
    }
    let vfx = roster
        .disabled()
        .iter()
        .find(|d| d.role.as_str() == "vfx-artist")
        .expect("VFX stays off");
    assert!(vfx.reason.contains("M11"), "{}", vfx.reason);
}

/// Meshlet statistics (M4 4.2 Q7) go to the roles that make and tune geometry, and no others.
#[test]
fn only_the_modeler_and_tech_artist_read_meshlet_stats() {
    let (_dir, roster) = roster();
    let mut with: Vec<&str> = roster
        .roles()
        .iter()
        .filter(|r| r.mcp_tools.iter().any(|t| t == "asset_meshlet_stats"))
        .map(|r| r.role.as_str())
        .collect();
    with.sort_unstable();
    assert_eq!(with, ["modeler", "tech-artist"]);
}

/// The tech artist knows the shadow cache's hint (M4 4.7b): when to use each `mobility` value,
/// and the stat that says a `Static` hint is wrong.
#[test]
fn tech_artist_knows_the_mobility_hint() {
    let (_dir, roster) = roster();
    let artist = roster
        .roles()
        .iter()
        .find(|r| r.role.as_str() == "tech-artist")
        .expect("the tech artist is enabled");
    for needle in [
        "mobility",
        "Auto",
        "Movable",
        "Static",
        "static_hint_moved",
        "pages_cached",
    ] {
        assert!(artist.body.contains(needle), "the prompt explains {needle}");
    }
}
