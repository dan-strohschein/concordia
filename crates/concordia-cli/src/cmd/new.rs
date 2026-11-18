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
}

pub(crate) fn run(args: &Args) -> Result<(), Failure> {
    ensure_usable(&args.dir, args.force)?;
    let name = args.name.clone().unwrap_or_else(|| default_name(&args.dir));
    let write = |relative: &str, text: String| write_file(&args.dir.join(relative), &text);
    write("concordia.toml", manifest(&name))?;
    write("scenes/main.scene.ron", main_scene()?)?;
    write(".gitignore", ".concordia/\n".to_owned())?;
    write(".mcp.json", mcp_json()?)?;
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

fn manifest(name: &str) -> String {
    let name = serde_json::to_string(name).unwrap_or_else(|_| "\"game\"".to_owned());
    format!(
        "# Concordia project manifest.\n[project]\nname = {name}\n\n[scene]\nmain = \"scenes/main.scene.ron\"\n\n[sim]\nrate_hz = 60\nseed = 0\n\n[journal]\nsync = true\n"
    )
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

/// Registers the MCP server as "concordia" for Claude Code, launched from the project root.
fn mcp_json() -> Result<String, Failure> {
    let exe = std::env::current_exe()
        .map_err(|e| Failure::usage(format!("cannot locate the concordia binary: {e}")))?;
    let config = json!({ "mcpServers": { "concordia": { "command": exe, "args": ["mcp", "--project", "."] } } });
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
