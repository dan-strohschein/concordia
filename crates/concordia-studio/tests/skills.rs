//! Engine skills (M3 phase 3.6): the default set, their frontmatter, and skill validation.

mod common;

use std::path::Path;

use concordia_studio::{CLAUDE_SKILL_FIELDS, Roster, SkillFile, default_roster, init};

/// The eight built-in skills the plan names (3.6), plus the team-comms skill (3.6b).
const PLANNED: [&str; 11] = [
    "concordia-add-material",
    "concordia-director-planning",
    "concordia-fix-bug-with-replay",
    "concordia-generate-mesh-blender",
    "concordia-record-replay",
    "concordia-scripting",
    "concordia-synthesize-sfx",
    "concordia-task-workflow",
    "concordia-team-comms",
    "concordia-terra-templates",
    "concordia-write-playtest",
];

fn skills() -> Vec<SkillFile> {
    default_roster()
        .skills
        .iter()
        .map(|(name, text)| SkillFile::parse(name, text).unwrap_or_else(|e| panic!("{name}: {e}")))
        .collect()
}

#[test]
fn the_default_roster_ships_the_planned_skills() {
    let names: Vec<&str> = default_roster().skills.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, PLANNED);
}

#[test]
fn skill_frontmatter_uses_documented_fields_only() {
    for (name, text) in default_roster().skills {
        let yaml = text
            .strip_prefix("---\n")
            .and_then(|t| t.split_once("\n---\n"))
            .map(|(y, _)| y)
            .unwrap_or_else(|| panic!("{name}: no frontmatter"));
        for line in yaml.lines().filter(|l| !l.starts_with([' ', '#', '-'])) {
            let key = line.split(':').next().unwrap();
            assert!(CLAUDE_SKILL_FIELDS.contains(&key), "{name}: {key}");
        }
    }
}

#[test]
fn skill_descriptions_under_listing_limit() {
    for skill in skills() {
        let listed = skill.description.chars().count()
            + skill
                .when_to_use
                .as_deref()
                .map_or(0, |w| w.chars().count());
        assert!(listed <= 1536, "{}: {listed} chars", skill.name);
        assert!(
            skill.description.chars().count() >= 40,
            "{}: too vague",
            skill.name
        );
    }
}

#[test]
fn only_director_planning_is_user_invocable() {
    for (name, text) in default_roster().skills {
        let invocable = text.contains("\nuser-invocable: true\n");
        assert_eq!(invocable, *name == "concordia-director-planning", "{name}");
    }
}

#[test]
fn skills_for_tools_not_built_yet_say_so_and_name_their_phase() {
    let offered: Vec<String> = common::catalog().names().map(str::to_owned).collect();
    for skill in skills() {
        let missing: Vec<String> = mentioned_tools(&skill.body)
            .into_iter()
            .filter(|t| !offered.contains(t))
            .collect();
        if missing.is_empty() {
            assert!(
                !skill.body.contains("> **Not available yet:**"),
                "{}: every tool exists; drop the note",
                skill.name
            );
            continue;
        }
        let note = skill
            .body
            .lines()
            .find(|l| l.starts_with("> **Not available yet:**"))
            .unwrap_or_else(|| panic!("{}: names {missing:?} with no note", skill.name));
        assert!(
            note.contains("phase 3."),
            "{}: name the phase: {note}",
            skill.name
        );
        for tool in &missing {
            assert!(
                note.contains(tool.as_str()),
                "{}: the note must list {tool}",
                skill.name
            );
        }
    }
}

#[test]
fn a_skill_with_an_unknown_field_or_wrong_name_is_rejected() {
    let unknown = "---\nname: concordia-x\ndescription: A skill with an unknown frontmatter field in it.\ncolour: red\n---\nBody.\n";
    let err = SkillFile::parse("concordia-x", unknown).unwrap_err();
    assert!(err.to_string().contains("colour"), "{err}");
    let renamed = "---\nname: concordia-y\ndescription: A skill whose name does not match its folder name.\n---\nBody.\n";
    let err = SkillFile::parse("concordia-x", renamed).unwrap_err();
    assert!(err.to_string().contains("concordia-x"), "{err}");
}

#[test]
fn init_writes_the_skills_and_roster_load_checks_them() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), Path::new("studio")).unwrap();
    for name in PLANNED {
        assert!(
            dir.path()
                .join(format!("studio/skills/{name}/SKILL.md"))
                .is_file(),
            "{name}"
        );
    }
    let skill = dir
        .path()
        .join("studio/skills/concordia-task-workflow/SKILL.md");
    let text = std::fs::read_to_string(&skill).unwrap();
    std::fs::write(&skill, text.replacen("---\n", "---\nbogus: 1\n", 1)).unwrap();
    let err = Roster::load(&dir.path().join("studio"), &common::catalog()).unwrap_err();
    assert!(err.to_string().contains("bogus"), "{err}");
}

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
fn team_comms_skill_teaches_the_caps_glossary_ids_and_when_to_ask() {
    let comms = skills()
        .into_iter()
        .find(|s| s.name == "concordia-team-comms")
        .expect("3.6b adds concordia-team-comms");
    for needle in [
        "280",
        "500",
        "## Glossary",
        "## When to ask",
        "Q-",
        "T-",
        "mcp__concordia__studio_ask",
    ] {
        assert!(comms.body.contains(needle), "missing {needle}");
    }
    assert!(
        comms.body.contains("Don't wait"),
        "asking never blocks: return to the director instead"
    );
}

/// Every `mcp__concordia__<tool> {json}` example in `text` for tools starting with `prefix`.
fn examples(text: &str, prefix: &str) -> Vec<(String, serde_json::Value)> {
    let marker = format!("mcp__concordia__{prefix}");
    text.match_indices(&marker)
        .filter_map(|(i, _)| {
            let rest = &text[i + "mcp__concordia__".len()..];
            let tool: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let after = rest[tool.len()..].trim_start_matches(' ');
            let json = after.strip_prefix('{').map(|_| balanced(after))?;
            Some((
                tool,
                serde_json::from_str(json).unwrap_or_else(|e| panic!("{json}: {e}")),
            ))
        })
        .collect()
}

/// The `{...}` object at the start of `text`.
fn balanced(text: &str) -> &str {
    let mut depth = 0;
    for (i, c) in text.char_indices() {
        depth += match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        };
        if depth == 0 {
            return &text[..=i];
        }
    }
    text
}

#[test]
fn replay_skills_use_the_real_replay_tools_and_arguments() {
    let skills = default_roster().skills;
    let mut checked = 0;
    for name in ["concordia-record-replay", "concordia-fix-bug-with-replay"] {
        let text = skills
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, t)| *t)
            .unwrap_or_else(|| panic!("{name}"));
        for (tool, args) in examples(text, "replay_") {
            let parsed = match tool.as_str() {
                "replay_record" => {
                    serde_json::from_value::<concordia_replay::ReplayRecordParams>(args.clone())
                        .map(|_| ())
                }
                "replay_run" => {
                    serde_json::from_value::<concordia_replay::ReplayRunParams>(args.clone())
                        .map(|_| ())
                }
                other => panic!("{name}: no tool {other}"),
            };
            parsed.unwrap_or_else(|e| panic!("{name}: {tool} {args}: {e}"));
            checked += 1;
        }
    }
    assert!(
        checked >= 4,
        "the skills show how to record, stop and run ({checked} examples)"
    );
}

#[test]
fn material_skill_uses_the_real_material_tools_and_arguments() {
    let text = default_roster()
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-add-material")
        .map(|(_, t)| *t)
        .unwrap();
    let mut checked = 0;
    for (tool, args) in examples(text, "material_") {
        let parsed = match tool.as_str() {
            "material_get" => {
                serde_json::from_value::<concordia_assets::MaterialGetParams>(args.clone())
                    .map(|_| ())
            }
            "material_set" => {
                serde_json::from_value::<concordia_assets::MaterialSetParams>(args.clone())
                    .map(|_| ())
            }
            other => panic!("no tool {other}"),
        };
        parsed.unwrap_or_else(|e| panic!("{tool} {args}: {e}"));
        if let Some(material) = args.get("material") {
            serde_json::from_value::<concordia_assets::Material>(material.clone())
                .unwrap()
                .validate()
                .unwrap_or_else(|e| panic!("{tool}: the example is a valid material: {e}"));
        }
        checked += 1;
    }
    assert!(
        checked >= 2,
        "the skill shows how to read and write a material ({checked} examples)"
    );
}
