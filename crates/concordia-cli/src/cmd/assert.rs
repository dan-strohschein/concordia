//! `concordia assert eval` and `concordia test playtests` (M3 phase 3.2b).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::replay::Cli;
use crate::Failure;

/// `concordia assert` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct AssertArgs {
    #[command(subcommand)]
    command: AssertCommand,
}

#[derive(Debug, clap::Subcommand)]
enum AssertCommand {
    /// Check assertions over a replay, a playtest or the scene now (exit 0 pass, 1 fail, 2 error).
    Eval(EvalArgs),
}

#[derive(Debug, clap::Args)]
struct EvalArgs {
    /// Assertions, e.g. "within 600 ticks: distance(tag(\"a\"), tag(\"b\")) < 2m".
    #[arg(required = true)]
    assertions: Vec<String>,
    /// Check over this replay's inputs.
    #[arg(long)]
    replay: Option<String>,
    /// Check over this playtest (its assertions too).
    #[arg(long)]
    scenario: Option<String>,
    /// Print the result object as JSON.
    #[arg(long)]
    json: bool,
    /// Project root.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

/// `concordia test` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct TestArgs {
    #[command(subcommand)]
    command: TestCommand,
}

#[derive(Debug, clap::Subcommand)]
enum TestCommand {
    /// Run every tests/playtests/**/*.playtest.ron (exit 1 if any fails).
    Playtests(PlaytestArgs),
}

#[derive(Debug, clap::Args)]
struct PlaytestArgs {
    /// Project root.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

pub(crate) fn run_assert(args: &AssertArgs) -> Result<(), Failure> {
    let AssertCommand::Eval(a) = &args.command;
    let (source, path) = match (&a.replay, &a.scenario) {
        (Some(r), None) => ("replay", Some(r.clone())),
        (None, Some(s)) => ("scenario", Some(s.clone())),
        (None, None) => ("live", None),
        (Some(_), Some(_)) => return Err(Failure::usage("pass --replay or --scenario, not both")),
    };
    let scene = match &path {
        Some(p) => scene_of(&a.project, source, p)?,
        None => main_scene(&a.project)?,
    };
    let mut cli = Cli::open(&a.project, None, &scene)?;
    let params = json!({"assertions": a.assertions, "source": source, "path": path});
    let result = evaluate(&mut cli, params)?;
    if a.json {
        println!("{result}");
    } else {
        print_results(&result);
    }
    verdict(&result, "assertions")
}

pub(crate) fn run_test(args: &TestArgs) -> Result<(), Failure> {
    let TestCommand::Playtests(a) = &args.command;
    let suites = playtests(&a.project)?;
    let mut failed = Vec::new();
    for rel in &suites {
        let scene = scene_of(&a.project, "scenario", rel)?;
        let mut cli = Cli::open(&a.project, None, &scene)?;
        let result = evaluate(&mut cli, json!({"source": "scenario", "path": rel}))?;
        let ok = result["ok"].as_bool() == Some(true);
        println!("{} {rel}", if ok { "ok  " } else { "FAIL" });
        if !ok {
            print_results(&result);
            failed.push(rel.clone());
        }
    }
    println!("{} playtests, {} failed", suites.len(), failed.len());
    match failed.as_slice() {
        [] => Ok(()),
        names => Err(Failure::usage(format!("failed: {}", names.join(", ")))),
    }
}

/// `assert.eval`; a parse or type error exits 2.
fn evaluate(cli: &mut Cli, params: Value) -> Result<Value, Failure> {
    cli.try_call("assert.eval", params)
        .map_err(|e| match e.kind {
            concordia_api::ApiErrorKind::InvalidParams => Failure {
                code: 2,
                message: e.message,
            },
            _ => super::api_failure(&e),
        })
}

fn verdict(result: &Value, what: &str) -> Result<(), Failure> {
    match result["ok"].as_bool() {
        Some(true) => Ok(()),
        _ => Err(Failure::usage(format!("{what} failed or undecided"))),
    }
}

fn print_results(result: &Value) {
    for r in result["results"].as_array().into_iter().flatten() {
        let at = r["decided_at"]
            .as_u64()
            .map_or_else(|| "undecided".to_owned(), |t| format!("at tick {t}"));
        let status = r["status"].as_str().unwrap_or("?");
        let label = if status == "undecided" {
            status.to_owned()
        } else {
            format!("{status} {at}")
        };
        println!(
            "  {label}: {} {}",
            r["assertion"].as_str().unwrap_or(""),
            r["witness"]
        );
    }
}

/// The scene a replay or playtest runs on.
fn scene_of(root: &Path, source: &str, rel: &str) -> Result<String, Failure> {
    let file = root.join(rel);
    let text = std::fs::read_to_string(&file)
        .map_err(|e| Failure::usage(format!("{}: {e}", file.display())))?;
    let scene = if source == "replay" {
        concordia_replay::read_replay(&text)
            .map(|r| r.header.scene)
            .map_err(|e| e.to_string())
    } else {
        concordia_scene::read_document::<concordia_replay::Playtest>(&text)
            .map(|p| p.scene)
            .map_err(|e| e.to_string())
    };
    scene.map_err(|e| Failure {
        code: 2,
        message: format!("{rel}: {e}"),
    })
}

fn main_scene(root: &Path) -> Result<String, Failure> {
    concordia_app::Project::load(root)
        .map_err(|e| Failure::usage(e.to_string()))?
        .manifest
        .scene
        .main
        .ok_or_else(|| Failure::usage("no [scene] main to check live; pass --replay or --scenario"))
}

/// Every `tests/playtests/**/*.playtest.ron` in the project at `root`, project-relative and
/// sorted.
fn playtests(root: &Path) -> Result<Vec<String>, Failure> {
    let mut out = Vec::new();
    let mut stack = vec![root.join("tests/playtests")];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.to_string_lossy().ends_with(".playtest.ron") {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    Ok(out)
}
