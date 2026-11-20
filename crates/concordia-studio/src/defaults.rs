//! The default team, embedded from `roster/` (the only home of the default roster).

use std::path::{Path, PathBuf};

use crate::error::StudioError;

/// Role files of the default roster: (id, text), sorted by id.
const ROLES: [(&str, &str); 12] = [
    ("animator", include_str!("../roster/agents/animator.md")),
    ("composer", include_str!("../roster/agents/composer.md")),
    ("director", include_str!("../roster/agents/director.md")),
    (
        "gameplay-programmer",
        include_str!("../roster/agents/gameplay-programmer.md"),
    ),
    (
        "level-designer",
        include_str!("../roster/agents/level-designer.md"),
    ),
    ("modeler", include_str!("../roster/agents/modeler.md")),
    (
        "qa-playtester",
        include_str!("../roster/agents/qa-playtester.md"),
    ),
    (
        "sound-designer",
        include_str!("../roster/agents/sound-designer.md"),
    ),
    (
        "tech-artist",
        include_str!("../roster/agents/tech-artist.md"),
    ),
    ("ui-ux", include_str!("../roster/agents/ui-ux.md")),
    ("vfx-artist", include_str!("../roster/agents/vfx-artist.md")),
    ("writer", include_str!("../roster/agents/writer.md")),
];

/// Engine skills of the default roster: (name, `SKILL.md`), sorted by name.
const SKILLS: [(&str, &str); 10] = [
    (
        "concordia-add-material",
        include_str!("../roster/skills/concordia-add-material/SKILL.md"),
    ),
    (
        "concordia-director-planning",
        include_str!("../roster/skills/concordia-director-planning/SKILL.md"),
    ),
    (
        "concordia-fix-bug-with-replay",
        include_str!("../roster/skills/concordia-fix-bug-with-replay/SKILL.md"),
    ),
    (
        "concordia-generate-mesh-blender",
        include_str!("../roster/skills/concordia-generate-mesh-blender/SKILL.md"),
    ),
    (
        "concordia-record-replay",
        include_str!("../roster/skills/concordia-record-replay/SKILL.md"),
    ),
    (
        "concordia-synthesize-sfx",
        include_str!("../roster/skills/concordia-synthesize-sfx/SKILL.md"),
    ),
    (
        "concordia-task-workflow",
        include_str!("../roster/skills/concordia-task-workflow/SKILL.md"),
    ),
    (
        "concordia-team-comms",
        include_str!("../roster/skills/concordia-team-comms/SKILL.md"),
    ),
    (
        "concordia-terra-templates",
        include_str!("../roster/skills/concordia-terra-templates/SKILL.md"),
    ),
    (
        "concordia-write-playtest",
        include_str!("../roster/skills/concordia-write-playtest/SKILL.md"),
    ),
];

const TEAM: &str = include_str!("../roster/team.toml");

/// The default roster's files.
#[derive(Debug, Clone, Copy)]
pub struct DefaultRoster {
    /// `team.toml`.
    pub team: &'static str,
    /// Role files: (id, text), sorted by id.
    pub roles: &'static [(&'static str, &'static str)],
    /// Engine skills: (name, `SKILL.md` text), sorted by name.
    pub skills: &'static [(&'static str, &'static str)],
}

/// The default team: 12 roles, some disabled until their engine features land.
pub fn default_roster() -> DefaultRoster {
    DefaultRoster {
        team: TEAM,
        roles: &ROLES,
        skills: &SKILLS,
    }
}

/// Writes the default Studio files into `<root>/<dir>`; refuses if that folder exists.
pub fn init(root: &Path, dir: &Path) -> Result<Vec<PathBuf>, StudioError> {
    let studio = root.join(dir);
    if studio.exists() {
        return Err(StudioError::invalid(format!(
            "{} already exists; this project already has Agent Studio files",
            dir.display()
        )));
    }
    let roster = default_roster();
    let mut files = vec![
        (PathBuf::from("team.toml"), roster.team),
        (PathBuf::from("tasks/.gitkeep"), ""),
    ];
    files.extend(
        roster
            .roles
            .iter()
            .map(|(id, text)| (PathBuf::from(format!("agents/{id}.md")), *text)),
    );
    files.extend(
        roster
            .skills
            .iter()
            .map(|(name, text)| (PathBuf::from(format!("skills/{name}/SKILL.md")), *text)),
    );
    files
        .into_iter()
        .map(|(relative, text)| {
            let path = studio.join(&relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| StudioError::io(parent, &e))?;
            }
            std::fs::write(&path, text).map_err(|e| StudioError::io(&path, &e))?;
            Ok(dir.join(relative))
        })
        .collect()
}
