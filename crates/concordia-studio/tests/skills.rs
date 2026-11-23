//! Engine skills (M3 phase 3.6): the default set, their frontmatter, and skill validation.

mod common;

use std::path::Path;

use concordia_studio::{CLAUDE_SKILL_FIELDS, Roster, SkillFile, default_roster, init};

/// The eight built-in skills the plan names (3.6), plus team comms (3.6b), animation (M2 2.4.5)
/// and navigation (M2 2.5a).
const PLANNED: [&str; 15] = [
    "concordia-add-material",
    "concordia-animation-graphs",
    "concordia-director-planning",
    "concordia-fix-bug-with-replay",
    "concordia-generate-mesh-blender",
    "concordia-nav-agents",
    "concordia-navigation",
    "concordia-record-replay",
    "concordia-scripting",
    "concordia-synthesize-sfx",
    "concordia-task-workflow",
    "concordia-team-comms",
    "concordia-terra-templates",
    "concordia-traffic",
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

#[test]
fn tech_artist_knows_materials_arrive_inside_gltf_and_fbx_models() {
    let roster = default_roster();
    let skill = roster
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-add-material")
        .map(|(_, t)| *t)
        .unwrap();
    let prompt = roster
        .roles
        .iter()
        .find(|(n, _)| *n == "tech-artist")
        .map(|(_, t)| *t)
        .unwrap();
    for text in [skill, prompt] {
        for format in ["`.gltf`", "`.glb`", "`.fbx`"] {
            assert!(text.contains(format), "{format} missing");
        }
        assert!(
            text.contains("#material/<name>"),
            "imported materials are sub-assets"
        );
    }
}

/// The animation skill's text.
fn animation_skill() -> &'static str {
    default_roster()
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-animation-graphs")
        .map(|(_, t)| *t)
        .expect("2.4.5 adds concordia-animation-graphs")
}

#[test]
fn animator_skill_is_available() {
    let text = animation_skill();
    assert!(
        !text.contains("Not available yet"),
        "every tool it names exists"
    );
    for needle in [
        "StateMachine(",
        "Blend2(",
        "BlendN(",
        "OneShot(",
        "TimeScale(",
        "Transition(",
        "AtEnd",
        "Auto",
        "\"travel\"",
        "\"triggers\"",
        "\"world\": \"play\"",
        "clip_events",
        "AnimEvent",
        "BoneAttachment",
        "Tweens",
        "mcp__concordia__anim_inspect",
        "\"skeleton\": true",
        "assets/anim/previews/",
        ".import.ron",
        "phase 3.7",
    ] {
        assert!(text.contains(needle), "the skill explains {needle}");
    }
    let animator = default_roster()
        .roles
        .iter()
        .find(|(n, _)| *n == "animator")
        .map(|(_, t)| concordia_studio::RoleFile::parse(t).unwrap())
        .unwrap();
    assert!(
        animator
            .spec
            .skills
            .iter()
            .any(|s| s == "concordia-animation-graphs"),
        "the animator preloads it: {:?}",
        animator.spec.skills
    );
}

/// The fenced ```ron blocks of `text`.
fn ron_blocks(text: &str) -> Vec<&str> {
    text.split("```ron\n")
        .skip(1)
        .filter_map(|rest| rest.split_once("\n```").map(|(block, _)| block))
        .collect()
}

#[test]
fn animation_skill_examples_are_real_params_and_a_valid_graph() {
    let text = animation_skill();
    let mut checked = 0;
    for (tool, args) in examples(text, "") {
        let parsed: Result<(), String> = match tool.as_str() {
            "anim_inspect" => serde_json::from_value::<concordia_anim::InspectParams>(args.clone())
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "capture_screenshot" => {
                serde_json::from_value::<concordia_render::CaptureScreenshotParams>(args.clone())
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            "prefab_instantiate" => serde_json::from_value::<
                <concordia_assets::PrefabInstantiate as concordia_api::Method>::Params,
            >(args.clone())
            .map(|_| ())
            .map_err(|e| e.to_string()),
            "component_set" => component_value(&args["component"], args["value"].clone()),
            "component_patch" => {
                let mut value = serde_json::to_value(concordia_anim::AnimGraphPlayer::new(
                    "asset:0190a2b4-0000-7000-8000-000000000001"
                        .parse()
                        .unwrap(),
                ))
                .unwrap();
                merge(&mut value, &args["patch"]);
                component_value(&args["component"], value)
            }
            _ => continue,
        };
        parsed.unwrap_or_else(|e| panic!("{tool} {args}: {e}"));
        checked += 1;
    }
    assert!(
        checked >= 6,
        "the skill shows inspecting, capturing, previewing and steering ({checked} examples)"
    );
    let blocks = ron_blocks(text);
    let graph = blocks
        .iter()
        .find(|b| b.contains("StateMachine("))
        .expect("a whole graph example");
    concordia_assets::read_animgraph(graph).unwrap_or_else(|e| panic!("the graph example: {e}"));
    let settings = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("settings: "))
        .and_then(|s| s.strip_suffix(','))
        .expect("a sidecar settings line");
    let settings: serde_json::Value = serde_json::from_str(settings).unwrap();
    let events = concordia_import_model::ModelSettings::from_json(&settings).unwrap();
    assert!(!events.clip_events.is_empty(), "{settings}");
}

/// Deserializes `value` as the named animation component.
fn component_value(component: &serde_json::Value, value: serde_json::Value) -> Result<(), String> {
    let result = match component.as_str() {
        Some("AnimGraphPlayer") => {
            serde_json::from_value::<concordia_anim::AnimGraphPlayer>(value).map(|_| ())
        }
        Some("BoneAttachment") => {
            serde_json::from_value::<concordia_anim::BoneAttachment>(value).map(|_| ())
        }
        Some("Tweens") => serde_json::from_value::<concordia_anim::Tweens>(value).map(|_| ()),
        other => return Err(format!("no example check for {other:?}")),
    };
    result.map_err(|e| e.to_string())
}

/// JSON merge patch (RFC 7386), as `component.patch` applies it.
fn merge(target: &mut serde_json::Value, patch: &serde_json::Value) {
    let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) else {
        *target = patch.clone();
        return;
    };
    for (key, value) in patch {
        match value {
            serde_json::Value::Null => {
                target.remove(key);
            }
            _ => merge(target.entry(key).or_insert(serde_json::Value::Null), value),
        }
    }
}

#[test]
fn terra_templates_skill_crowd_example_is_a_valid_template() {
    let text = default_roster()
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-terra-templates")
        .map(|(_, t)| *t)
        .unwrap();
    let block = ron_blocks(text)
        .into_iter()
        .find(|b| b.contains("\"Crowd\""))
        .expect("a Crowd trait example");
    let doc = format!(
        "Template(\n    format: 1,\n    description: \"Skill example.\",\n    {block}\n    components: {{}},\n)\n"
    );
    let mut registry = concordia_reflect::TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    concordia_terra_sim::register(&mut registry).unwrap();
    concordia_nav::register_types(&mut registry).unwrap();
    concordia_terra_crowd::register_types(&mut registry).unwrap();
    let doc = concordia_scene::read_template(&doc, &registry)
        .unwrap_or_else(|e| panic!("the Crowd example: {e}"));
    let compiled = concordia_terra_sim::Template::from_doc("example", &doc)
        .unwrap()
        .compile(&registry)
        .unwrap_or_else(|e| panic!("the Crowd example: {e}"));
    assert_eq!(
        compiled.component_names(),
        ["CrowdAgent", "NavAgent", "Transform"]
    );
    for needle in ["NavAgent.target", "CharacterController", "`crowd`"] {
        assert!(text.contains(needle), "the skill explains {needle}");
    }
}

#[test]
fn terra_templates_skill_lod_examples_are_real_params_and_a_valid_template() {
    let text = default_roster()
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-terra-templates")
        .map(|(_, t)| *t)
        .unwrap();
    let mut checked = 0;
    for (tool, args) in examples(text, "") {
        match tool.as_str() {
            "component_set" if args["component"] == "LodViewer" => {
                serde_json::from_value::<concordia_terra::LodViewer>(args["value"].clone())
                    .unwrap_or_else(|e| panic!("{args}: {e}"));
            }
            "terra_stats" => {
                serde_json::from_value::<concordia_api::WorldSel>(args["world"].clone())
                    .unwrap_or_else(|e| panic!("{args}: {e}"));
            }
            "capture_screenshot" => {
                serde_json::from_value::<concordia_render::CaptureScreenshotParams>(args.clone())
                    .unwrap_or_else(|e| panic!("{args}: {e}"));
            }
            _ => continue,
        }
        checked += 1;
    }
    assert!(
        checked >= 3,
        "marking a viewer and reading the buckets ({checked})"
    );
    let block = ron_blocks(text)
        .into_iter()
        .find(|b| b.contains("\"Lod\""))
        .expect("a Lod trait example");
    let doc = format!(
        "Template(\n    format: 1,\n    description: \"Skill example.\",\n    {block}\n    components: {{}},\n)\n"
    );
    let mut registry = concordia_reflect::TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    concordia_terra_sim::register(&mut registry).unwrap();
    let doc = concordia_scene::read_template(&doc, &registry)
        .unwrap_or_else(|e| panic!("the Lod example: {e}"));
    let compiled = concordia_terra_sim::Template::from_doc("example", &doc)
        .unwrap()
        .compile(&registry)
        .unwrap_or_else(|e| panic!("the Lod example: {e}"));
    assert_eq!(compiled.component_names(), ["LodProfile"]);
    let block = ron_blocks(text)
        .into_iter()
        .find(|b| b.contains("\"Representation\""))
        .expect("a Representation example");
    concordia_assets::register_types(&mut registry).unwrap();
    concordia_render::register_types(&mut registry).unwrap();
    let doc = format!(
        "Template(\n    format: 1,\n    description: \"Skill example.\",\n    traits: {{}},\n    {block}\n)\n"
    );
    let doc = concordia_scene::read_template(&doc, &registry)
        .unwrap_or_else(|e| panic!("the Representation example: {e}"));
    let compiled = concordia_terra_sim::Template::from_doc("example", &doc)
        .unwrap()
        .compile(&registry)
        .unwrap_or_else(|e| panic!("the Representation example: {e}"));
    assert_eq!(compiled.component_names(), ["Representation"]);
    for needle in ["LodViewer", "hysteresis", "`lod`", "`levels`", "`due`"] {
        assert!(text.contains(needle), "the skill explains {needle}");
    }
}

#[test]
fn navigation_skill_examples_are_real_params_and_components() {
    let text = default_roster()
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-navigation")
        .map(|(_, t)| *t)
        .expect("the navigation skill");
    let mut seen = std::collections::BTreeSet::new();
    for (tool, args) in examples(text, "") {
        let checked = match tool.as_str() {
            "nav_build" => {
                serde_json::from_value::<concordia_nav::NavBuildParams>(args.clone()).map(|_| ())
            }
            "nav_path" => {
                serde_json::from_value::<concordia_nav::NavPathParams>(args.clone()).map(|_| ())
            }
            "capture_screenshot" => {
                serde_json::from_value::<concordia_render::CaptureScreenshotParams>(args.clone())
                    .map(|_| ())
            }
            "component_set" => match args["component"].as_str() {
                Some("NavMeshVolume") => {
                    serde_json::from_value::<concordia_nav::NavMeshVolume>(args["value"].clone())
                        .map(|_| ())
                }
                Some("NavArea") => {
                    serde_json::from_value::<concordia_nav::NavArea>(args["value"].clone())
                        .map(|_| ())
                }
                Some("NavAreaGrid") => {
                    serde_json::from_value::<concordia_nav::NavAreaGrid>(args["value"].clone())
                        .map(|_| ())
                }
                other => panic!("no example check for {other:?}"),
            },
            _ => continue,
        };
        checked.unwrap_or_else(|e| panic!("{tool} {args}: {e}"));
        seen.insert(match tool.as_str() {
            "component_set" => args["component"].as_str().unwrap().to_owned(),
            t => t.to_owned(),
        });
    }
    for needed in [
        "nav_build",
        "nav_path",
        "capture_screenshot",
        "NavMeshVolume",
        "NavArea",
        "NavAreaGrid",
    ] {
        assert!(seen.contains(needed), "the skill shows {needed}: {seen:?}");
    }
}

#[test]
fn nav_agents_skill_examples_are_real_params_and_its_mirror_matches_nav_agent() {
    let text = default_roster()
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-nav-agents")
        .map(|(_, t)| *t)
        .expect("the nav agents skill");
    let mut seen = std::collections::BTreeSet::new();
    for (tool, args) in examples(text, "") {
        let checked = match tool.as_str() {
            "nav_path" => {
                serde_json::from_value::<concordia_nav::NavPathParams>(args.clone()).map(|_| ())
            }
            "query_run" => {
                const FIELDS: [&str; 7] = [
                    "with", "without", "where", "select", "world", "limit", "cursor",
                ];
                let keys = args.as_object().expect("an object");
                assert!(
                    keys.keys().all(|k| FIELDS.contains(&k.as_str())),
                    "query_run {args}: only query.run's fields"
                );
                assert!(
                    args["with"].is_array(),
                    "query_run {args}: `with` is required"
                );
                serde_json::from_value::<concordia_api::WorldSel>(args["world"].clone()).map(|_| ())
            }
            _ => continue,
        };
        checked.unwrap_or_else(|e| panic!("{tool} {args}: {e}"));
        seen.insert(tool);
    }
    for needed in ["nav_path", "query_run"] {
        assert!(seen.contains(needed), "the skill shows {needed}: {seen:?}");
    }
    // The script mirrors name every field, in order (wire layouts follow fields).
    assert_eq!(
        mirror_fields(text, "NavAgent"),
        struct_fields::<concordia_nav::NavAgent>(),
        "the NavAgent mirror in the skill matches NavAgent"
    );
    assert_eq!(
        mirror_fields(text, "CrowdAgent"),
        struct_fields::<concordia_nav::CrowdAgent>(),
        "the CrowdAgent mirror in the skill matches CrowdAgent"
    );
    assert_eq!(
        mirror_fields(text, "CrowdAvoidance"),
        struct_fields::<concordia_nav::CrowdAvoidance>(),
        "the CrowdAvoidance mirror in the skill matches CrowdAvoidance"
    );
    for needle in ["CrowdState", "`crowd`", "CharacterController", "surface"] {
        assert!(text.contains(needle), "the skill explains {needle}");
    }
}

/// The field names of the script mirror `pub struct <name>` in any rust block of `text`.
fn mirror_fields<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let header = format!("pub struct {name} {{");
    let block = text
        .split("```rust")
        .skip(1)
        .filter_map(|r| r.split("```").next())
        .find(|b| b.contains(&header))
        .unwrap_or_else(|| panic!("a rust block with the {name} mirror"));
    let start = block.find(&header).unwrap();
    let body = &block[start..block[start..].find('}').map_or(block.len(), |e| start + e)];
    body.lines()
        .filter_map(|l| l.trim().strip_prefix("pub "))
        .filter_map(|l| l.split_once(':').map(|(name, _)| name.trim()))
        .collect()
}

/// The field names of reflected struct `T`, in order.
fn struct_fields<T: concordia_reflect::Reflect>() -> Vec<&'static str> {
    let info = T::type_info();
    let concordia_reflect::Shape::Struct { fields, .. } = &info.shape else {
        panic!("{} is a struct", info.name)
    };
    fields.iter().map(|f| f.name).collect()
}

#[test]
fn traffic_skill_examples_are_real_params_components_and_a_valid_template() {
    let text = default_roster()
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-traffic")
        .map(|(_, t)| *t)
        .expect("the traffic skill");
    let mut seen = std::collections::BTreeSet::new();
    let component = |name: &str, value: &serde_json::Value| -> Result<(), serde_json::Error> {
        match name {
            "Lane" => {
                serde_json::from_value::<concordia_terra_crowd::Lane>(value.clone()).map(|_| ())
            }
            "Junction" => {
                serde_json::from_value::<concordia_terra_crowd::Junction>(value.clone()).map(|_| ())
            }
            "Vehicle" => {
                serde_json::from_value::<concordia_terra_crowd::Vehicle>(value.clone()).map(|_| ())
            }
            "Name" | "Transform" => Ok(()),
            other => panic!("no example check for {other}"),
        }
    };
    for (tool, args) in examples(text, "") {
        match tool.as_str() {
            "entity_spawn" => {
                for (name, value) in args["components"].as_object().expect("components") {
                    component(name, value).unwrap_or_else(|e| panic!("{name} {value}: {e}"));
                    seen.insert(name.clone());
                }
            }
            "component_set" => {
                let name = args["component"].as_str().expect("a component");
                component(name, &args["value"]).unwrap_or_else(|e| panic!("{args}: {e}"));
                seen.insert(name.to_owned());
            }
            "capture_screenshot" => {
                let p = serde_json::from_value::<concordia_render::CaptureScreenshotParams>(
                    args.clone(),
                )
                .unwrap_or_else(|e| panic!("{args}: {e}"));
                assert!(format!("{p:?}").contains("lanes"), "draws the lanes layer");
            }
            "query_run" => {
                assert!(
                    args["with"].is_array(),
                    "query_run {args}: `with` is required"
                );
                serde_json::from_value::<concordia_api::WorldSel>(args["world"].clone()).unwrap();
            }
            _ => continue,
        }
        seen.insert(tool);
    }
    for needed in [
        "Lane",
        "Junction",
        "entity_spawn",
        "component_set",
        "capture_screenshot",
        "query_run",
    ] {
        assert!(seen.contains(needed), "the skill shows {needed}: {seen:?}");
    }
    let block = ron_blocks(text)
        .into_iter()
        .find(|b| b.contains("\"Traffic\""))
        .expect("a Traffic trait example");
    let doc = format!(
        "Template(\n    format: 1,\n    description: \"Skill example.\",\n    {block}\n    components: {{}},\n)\n"
    );
    let mut registry = concordia_reflect::TypeRegistry::new();
    concordia_scene::builtin::register(&mut registry).unwrap();
    concordia_terra_sim::register(&mut registry).unwrap();
    concordia_nav::register_types(&mut registry).unwrap();
    concordia_terra_crowd::register_types(&mut registry).unwrap();
    let doc = concordia_scene::read_template(&doc, &registry)
        .unwrap_or_else(|e| panic!("the Traffic example: {e}"));
    let compiled = concordia_terra_sim::Template::from_doc("example", &doc)
        .unwrap()
        .compile(&registry)
        .unwrap_or_else(|e| panic!("the Traffic example: {e}"));
    assert_eq!(compiled.component_names(), ["Transform", "Vehicle"]);
    for needle in [
        "priority",
        "next",
        "TrafficProblems",
        "`lanes`",
        "Kinematic",
    ] {
        assert!(text.contains(needle), "the skill explains {needle}");
    }
}

#[test]
fn nav_agents_skill_mirrors_vehicle() {
    let text = default_roster()
        .skills
        .iter()
        .find(|(n, _)| *n == "concordia-nav-agents")
        .map(|(_, t)| *t)
        .expect("the nav agents skill");
    assert_eq!(
        mirror_fields(text, "Vehicle"),
        struct_fields::<concordia_terra_crowd::Vehicle>(),
        "the Vehicle mirror in the skill matches Vehicle"
    );
    for needle in [
        "VehicleArrived",
        "VehicleRouteFailed",
        "VehicleState",
        "destination",
    ] {
        assert!(text.contains(needle), "the skill explains {needle}");
    }
}
