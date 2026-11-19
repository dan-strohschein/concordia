//! `concordia fmt`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use concordia_api::{Engine, EngineConfig};
use concordia_reflect::TypeRegistry;
use concordia_scene::SceneError;

use crate::Failure;

/// `concordia fmt` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    /// Report files that are not canonical (exit 1) instead of rewriting them.
    #[arg(long)]
    check: bool,
    /// Project whose modules define the components (default: each file's nearest `concordia.toml`).
    #[arg(long)]
    project: Option<PathBuf>,
    /// Files or directories (default: the current directory).
    paths: Vec<PathBuf>,
}

const EXTENSIONS: [&str; 2] = [".scene.ron", ".prefab.ron"];
const MANIFEST: &str = "concordia.toml";

/// The project a file is formatted against: a root, or none (built-in components only).
type ProjectRoot = Option<PathBuf>;

/// An engine with the project's modules, for its type registry.
fn project_engine(root: &Path) -> Result<Engine, Failure> {
    let project = concordia_app::Project::load(root).map_err(|e| Failure::usage(e.to_string()))?;
    let mut config = EngineConfig::new(root);
    config.modules = project
        .modules()
        .map_err(|e| Failure::usage(e.to_string()))?;
    Engine::new(config).map_err(|e| Failure::usage(e.message))
}

/// The nearest ancestor directory of `file` that holds a `concordia.toml`.
fn nearest_project(file: &Path) -> ProjectRoot {
    let absolute = std::path::absolute(file).ok()?;
    absolute
        .ancestors()
        .skip(1)
        .find(|dir| dir.join(MANIFEST).is_file())
        .map(Path::to_path_buf)
}

/// Engines by project root, built on first use (none: the core components only).
#[derive(Default)]
struct Engines(BTreeMap<ProjectRoot, Engine>);

impl Engines {
    fn registry(&mut self, root: &ProjectRoot) -> Result<&TypeRegistry, Failure> {
        if !self.0.contains_key(root) {
            let dir = root.as_deref().unwrap_or(Path::new("."));
            self.0.insert(root.clone(), project_engine(dir)?);
        }
        Ok(self.0[root].registry())
    }
}

pub(crate) fn run(args: &Args) -> Result<(), Failure> {
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
    let mut engines = Engines::default();
    let (mut changed, mut broken) = (Vec::new(), 0);
    for file in &files {
        let project = match &args.project {
            Some(dir) => Some(dir.clone()),
            None => nearest_project(file),
        };
        let registry = engines.registry(&project)?;
        match format_file(file, registry, args.check) {
            Ok(true) => changed.push(file),
            Ok(false) => {}
            Err(error) => {
                eprintln!("{}: {}", file.display(), describe(&error, &project));
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

/// A file's error, with a hint when a module component may be unknown only for lack of a project.
fn describe(error: &FormatError, project: &ProjectRoot) -> String {
    match (error, project) {
        (FormatError::Scene(e @ SceneError::UnknownComponent { .. }), None) => format!(
            "{e} (no {MANIFEST} found above this file, so only built-in components are known; pass --project <dir>)"
        ),
        _ => error.to_string(),
    }
}

/// Why one file could not be formatted.
#[derive(Debug)]
enum FormatError {
    Io(std::io::Error),
    Scene(SceneError),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Scene(e) => e.fmt(f),
        }
    }
}

impl From<std::io::Error> for FormatError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<SceneError> for FormatError {
    fn from(e: SceneError) -> Self {
        Self::Scene(e)
    }
}

/// Formats one file; returns whether it was (or, with `check`, would be) changed.
fn format_file(file: &Path, registry: &TypeRegistry, check: bool) -> Result<bool, FormatError> {
    let text = std::fs::read_to_string(file)?;
    let canonical = concordia_scene::format(&text, registry)?;
    if canonical == text {
        return Ok(false);
    }
    if !check {
        std::fs::write(file, canonical)?;
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
