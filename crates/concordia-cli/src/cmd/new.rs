//! `concordia new <dir>`.

use std::path::{Path, PathBuf};

use concordia_core::id::{IdSource, SystemIdGen};
use concordia_scene::{FORMAT_VERSION, SceneDoc, write_scene};
use serde_json::json;

use crate::Failure;

/// `concordia new` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Project directory (created if missing).
    dir: PathBuf,
    /// Project name (default: the directory name).
    #[arg(long)]
    name: Option<String>,
    /// Write into a non-empty directory (existing files other than the project files are kept).
    #[arg(long)]
    force: bool,
    /// Leave out Agent Studio (add it later with `concordia studio init`).
    #[arg(long)]
    no_studio: bool,
    /// Only the core engine: no physics, assets, render or Agent Studio.
    #[arg(long)]
    minimal: bool,
}

/// What the project's manifest switches on.
struct Features {
    modules: bool,
    studio: bool,
}

pub(crate) fn run(args: &Args) -> Result<(), Failure> {
    ensure_usable(&args.dir, args.force)?;
    let name = args.name.clone().unwrap_or_else(|| default_name(&args.dir));
    let features = Features {
        modules: !args.minimal,
        studio: !args.minimal && !args.no_studio,
    };
    let write = |relative: &str, text: String| write_file(&args.dir.join(relative), &text);
    write("concordia.toml", manifest(&name, &features))?;
    write("scenes/main.scene.ron", main_scene()?)?;
    if features.modules {
        write(STARTER_MAP, starter_map()?)?;
    }
    write(
        ".gitignore",
        ".concordia/\n.claude/settings.local.json\n".to_owned(),
    )?;
    write(".mcp.json", mcp_json()?)?;
    if features.studio && !args.dir.join("studio").exists() {
        concordia_studio::init(&args.dir, Path::new("studio"))
            .map_err(|e| Failure::usage(e.to_string()))?;
    }
    if features.studio {
        super::studio::sync(&args.dir, false, false, false)?;
    }
    eprintln!("created project {name:?} in {}", args.dir.display());
    Ok(())
}

fn ensure_usable(dir: &Path, force: bool) -> Result<(), Failure> {
    let non_empty = std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some());
    match non_empty && !force {
        true => Err(Failure::usage(format!(
            "{} is not empty; pass --force to create the project there anyway",
            dir.display()
        ))),
        false => Ok(()),
    }
}

fn default_name(dir: &Path) -> String {
    let absolute = std::path::absolute(dir).unwrap_or_else(|_| dir.to_owned());
    absolute
        .file_name()
        .map_or_else(|| "game".to_owned(), |n| n.to_string_lossy().into_owned())
}

fn manifest(name: &str, features: &Features) -> String {
    let name = serde_json::to_string(name).unwrap_or_else(|_| "\"game\"".to_owned());
    let mut text = format!(
        "# Concordia project manifest.\n[project]\nname = {name}\n\n[scene]\nmain = \"scenes/main.scene.ron\"\n\n[sim]\nrate_hz = 60\nseed = 0\n\n[journal]\nsync = true\n"
    );
    if features.modules {
        text.push_str(&format!(
            "\n# Input: actions and their bindings (the starter map clicks to move).\n[input]\nmap = \"{STARTER_MAP}\"\n"
        ));
        text.push_str("\n[physics]\ngravity = [0.0, -9.81, 0.0]\n");
        text.push_str(
            "\n# Navigation: navmeshes from static colliders (NavMeshVolume), nav.build and nav.path.\n[nav]\n",
        );
        text.push_str("\n[assets]\n");
        text.push_str(
            "\n# Animation: skeletons, clips, .animgraph.ron graphs and events (uses the assets module).\n[anim]\n",
        );
        text.push_str(
            "\n# Audio: .cue.ron cues, buses and the mixer (offline headless; the default device in concordia run).\n[audio]\n",
        );
        text.push_str(
            "\n# In-game UI: UiRoot trees, .theme.ron themes and fonts, ui.inspect (drawn by the render module).\n[ui]\n",
        );
        text.push_str("\n[render]\n");
        text.push_str(concat!(
            "# Shadows: cascaded (csm, the default) or virtual shadow maps (vsm; M4 4.7).\n",
            "# shadows = \"vsm\"\n",
            "# [render.vsm]\n",
            "# pool_pages = 4096      # physical pages, 64 KB each\n",
            "# levels = 16            # sun clipmap levels, 8 m wide at level 0\n",
            "# resolution_bias = 0.0  # -1 finer, +1 coarser\n",
        ));
        text.push_str(
            "\n# Gameplay modules: add [[scripts.module]] entries (kind = \"wasm\" or \"native\", path).\n[scripts]\n",
        );
        text.push_str(
            "\n# Replays: replay.record / replay.run, files under tests/replays/.\n[replay]\n",
        );
        text.push_str(
            "\n# Generators: gen.list / gen.run (meshes, sounds, music, textures, Blender scripts).\n[gen]\n",
        );
    }
    if features.studio {
        text.push_str(
            "\n# Agent Studio: studio/team.toml, studio/agents/, studio/tasks/.\n[studio]\n",
        );
    }
    text
}

/// Where new projects keep their input map.
pub(crate) const STARTER_MAP: &str = "input/default.input.ron";

/// The starter input map's text.
pub(crate) fn starter_map() -> Result<String, Failure> {
    concordia_input::InputMap::starter()
        .to_ron()
        .map_err(|e| Failure::usage(e.to_string()))
}

fn main_scene() -> Result<String, Failure> {
    let id = SystemIdGen
        .next_id()
        .map_err(|e| Failure::usage(e.to_string()))?;
    let doc = SceneDoc {
        format: FORMAT_VERSION,
        id,
        name: "Main".into(),
        entities: Vec::new(),
    };
    write_scene(&doc).map_err(|e| Failure::usage(e.to_string()))
}

/// Registers the MCP server as "concordia" for Claude Code: `concordia` on PATH, no user paths.
fn mcp_json() -> Result<String, Failure> {
    let config = json!({ "mcpServers": { "concordia": {
        "command": "concordia",
        "args": ["mcp", "--project", "${CLAUDE_PROJECT_DIR:-.}"]
    } } });
    let mut text =
        serde_json::to_string_pretty(&config).map_err(|e| Failure::usage(e.to_string()))?;
    text.push('\n');
    Ok(text)
}

fn write_file(path: &Path, text: &str) -> Result<(), Failure> {
    let fail = |e: std::io::Error| Failure::usage(format!("{}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    std::fs::write(path, text).map_err(fail)
}
