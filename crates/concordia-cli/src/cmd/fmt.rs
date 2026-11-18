//! `concordia fmt`.

use std::path::{Path, PathBuf};

use concordia_api::{Engine, EngineConfig};
use concordia_reflect::TypeRegistry;

use crate::Failure;

/// `concordia fmt` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Report files that are not canonical (exit 1) instead of rewriting them.
    #[arg(long)]
    check: bool,
    /// Project whose modules define the components (default: the current directory).
    #[arg(long, default_value = ".")]
    project: PathBuf,
    /// Files or directories (default: the current directory).
    paths: Vec<PathBuf>,
}

const EXTENSIONS: [&str; 2] = [".scene.ron", ".prefab.ron"];

/// An engine with the project's modules, for its type registry.
fn project_engine(root: &Path) -> Result<Engine, Failure> {
    let project = concordia_app::Project::load(root).map_err(|e| Failure::usage(e.to_string()))?;
    let mut config = EngineConfig::new(root);
    config.modules = project
        .modules()
        .map_err(|e| Failure::usage(e.to_string()))?;
    Engine::new(config).map_err(|e| Failure::usage(e.message))
}

pub(crate) fn run(args: &Args) -> Result<(), Failure> {
    let engine = project_engine(&args.project)?;
    let registry = engine.registry();
    let roots = if args.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        args.paths.clone()
    };
    let mut files = Vec::new();
    for root in &roots {
        collect(root, &mut files)
            .map_err(|e| Failure::usage(format!("{}: {e}", root.display())))?;
    }
    files.sort();
    let (mut changed, mut broken) = (Vec::new(), 0);
    for file in &files {
        match format_file(file, registry, args.check) {
            Ok(true) => changed.push(file),
            Ok(false) => {}
            Err(message) => {
                eprintln!("{}: {message}", file.display());
                broken += 1;
            }
        }
    }
    for file in &changed {
        println!("{}", file.display());
    }
    match (broken, args.check && !changed.is_empty()) {
        (0, false) => Ok(()),
        (0, true) => Err(Failure {
            code: 1,
            message: format!(
                "{} file(s) are not canonical; run `concordia fmt`",
                changed.len()
            ),
        }),
        _ => Err(Failure {
            code: 2,
            message: format!("{broken} file(s) could not be parsed"),
        }),
    }
}

/// Formats one file; returns whether it was (or, with `check`, would be) changed.
fn format_file(file: &Path, registry: &TypeRegistry, check: bool) -> Result<bool, String> {
    let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let canonical = concordia_scene::format(&text, registry).map_err(|e| e.to_string())?;
    if canonical == text {
        return Ok(false);
    }
    if !check {
        std::fs::write(file, canonical).map_err(|e| e.to_string())?;
    }
    Ok(true)
}

/// Scene and prefab files under `path`, skipping hidden directories (such as `.concordia`) and `target`.
fn collect(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        out.push(path.to_owned());
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let child = entry.path();
        if child.is_dir() {
            if !name.starts_with('.') && name != "target" {
                collect(&child, out)?;
            }
        } else if EXTENSIONS.iter().any(|ext| name.ends_with(ext)) {
            out.push(child);
        }
    }
    Ok(())
}
