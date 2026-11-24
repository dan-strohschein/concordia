//! `concordia reference bless <name>`: traces a path-traced reference at full quality, measures
//! the renderer against it and its own noise, and writes it (ADR-0090). Never in CI.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use concordia_api::Engine;
use concordia_app::Project;
use concordia_core::id::SessionId;
use concordia_pathtrace::Traced;
use concordia_testkit::golden::{Golden, GoldenError, RgbaImage, flip_score};
use concordia_testkit::reference::{Blessed, ReferenceFile, SUITE};
use serde_json::{Value, json};

use crate::Failure;

/// `concordia reference` arguments.
#[derive(Debug, clap::Args)]
pub(crate) struct ReferenceArgs {
    #[command(subcommand)]
    command: ReferenceCommand,
}

#[derive(Debug, clap::Subcommand)]
enum ReferenceCommand {
    /// Trace `<golden>/m4_reference/<name>.reference.ron` and write `<name>.png` with the
    /// measured baseline and noise.
    Bless(BlessArgs),
}

/// `concordia reference bless` arguments.
#[derive(Debug, clap::Args)]
struct BlessArgs {
    /// The reference's name.
    name: String,
    /// The golden store.
    #[arg(long, default_value = "tests/golden")]
    golden: PathBuf,
}

pub(crate) fn run(args: &ReferenceArgs) -> Result<(), Failure> {
    match &args.command {
        ReferenceCommand::Bless(bless) => bless_reference(bless),
    }
}

/// Where the reference, the renderer's image and their FLIP heat map are written for review.
const REVIEW_DIR: &str = "target/reference-review";

/// Seed offset for the second trace (the noise measurement).
const NOISE_SEED_OFFSET: u32 = 0x9e37;
/// Near and far planes of a reference's free view.
const NEAR: f32 = 0.1;
const FAR: f32 = 1000.0;

fn bless_reference(args: &BlessArgs) -> Result<(), Failure> {
    let golden = Golden::new(
        args.golden.clone(),
        PathBuf::from(REVIEW_DIR),
        false,
        std::env::var("CI").as_deref() == Ok("true"),
    );
    if golden.is_ci() {
        return Err(fail(GoldenError::BlessInCi));
    }
    let mut file = golden.reference(&args.name).map_err(fail)?;
    let mut session = Session::open(&file)?;
    let first = session.trace(&file, file.seed)?;
    let second = session.trace(&file, file.seed.wrapping_add(NOISE_SEED_OFFSET))?;
    let rendered = session.screenshot(&file)?;
    let reference = image(&first)?;
    let blessed = Blessed {
        backend: format!("{:?}", first.backend),
        git: git_hash(Path::new(&file.project))?,
        baseline: flip_score(&reference, &rendered).map_err(fail)?,
        noise: flip_score(&reference, &image(&second)?).map_err(fail)?,
    };
    println!(
        "{}: traced on {} in {:.1} s; baseline FLIP mean {:.4} p99 {:.4}; noise FLIP mean {:.4} p99 {:.4}",
        args.name,
        blessed.backend,
        first.elapsed_ms / 1000.0,
        blessed.baseline.mean,
        blessed.baseline.p99,
        blessed.noise.mean,
        blessed.noise.p99
    );
    let review = golden
        .write_comparison(SUITE, &args.name, &reference, &rendered)
        .map_err(fail)?;
    println!(
        "review: {review}/{}.{{expected,actual,flip}}.png (reference, renderer, FLIP)",
        args.name
    );
    file.blessed = Some(blessed);
    golden
        .bless_reference(&args.name, &file, &reference)
        .map_err(fail)?;
    println!(
        "wrote {}",
        args.golden
            .join(SUITE)
            .join(format!("{}.png", args.name))
            .display()
    );
    Ok(())
}

fn fail(e: impl std::fmt::Display) -> Failure {
    Failure::usage(e.to_string())
}

fn image(traced: &Traced) -> Result<RgbaImage, Failure> {
    RgbaImage::new(traced.width, traced.height, traced.rgba.clone()).map_err(fail)
}

/// The short hash of HEAD in the repository holding `project`.
fn git_hash(project: &Path) -> Result<String, Failure> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map_err(|e| fail(format!("git: {e}")))?;
    if !out.status.success() {
        return Err(fail("git rev-parse failed; bless inside the repository"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The view as `capture.screenshot` and `capture.reference` take it (None: the scene camera).
fn view(file: &ReferenceFile) -> Option<Value> {
    file.view.as_ref().map(|v| {
        json!({"eye": v.eye, "target": v.target, "projection": {"Perspective": {
            "fov_y": v.fov_y_degrees.to_radians(), "near": NEAR, "far": FAR}}})
    })
}

/// An engine on the reference's project with its scene open.
struct Session {
    engine: Engine,
    session: SessionId,
}

impl Session {
    fn open(file: &ReferenceFile) -> Result<Self, Failure> {
        let project = Path::new(&file.project);
        let loaded = Project::load(project).map_err(fail)?;
        let scene = file
            .scene
            .clone()
            .or_else(|| loaded.manifest.scene.main.clone())
            .ok_or_else(|| {
                fail(format!(
                    "{}: no scene given and no main scene",
                    project.display()
                ))
            })?;
        let mut engine = Engine::new(loaded.engine_config().map_err(fail)?)
            .map_err(|e| fail(format!("{e:?}")))?;
        let session = engine.open_session();
        let mut opened = Self { engine, session };
        opened.call(
            "session.open",
            json!({"client": "bless-reference", "actor": "tool:bless-reference"}),
        )?;
        opened.call("scene.open", json!({"path": scene}))?;
        Ok(opened)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, Failure> {
        self.engine
            .call(self.session, method, params)
            .map_err(|e| fail(format!("{method}: {}", e.message)))
    }

    fn trace(&mut self, file: &ReferenceFile, seed: u32) -> Result<Traced, Failure> {
        let mut params = json!({"width": file.width, "height": file.height, "spp": file.spp,
            "seed": seed, "max_bounces": file.max_bounces});
        if let Some(view) = view(file) {
            params["view"] = view;
        }
        let params = serde_json::from_value(params).map_err(fail)?;
        concordia_pathtrace::trace(&mut self.engine, &params).map_err(|e| fail(e.message))
    }

    fn screenshot(&mut self, file: &ReferenceFile) -> Result<RgbaImage, Failure> {
        let mut params = json!({"width": file.width, "height": file.height, "ui": false});
        if let Some(view) = view(file) {
            params["view"] = view;
        }
        let shot = self.call("capture.screenshot", params)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(shot["image"]["data_base64"].as_str().unwrap_or_default())
            .map_err(fail)?;
        RgbaImage::from_png(&bytes).map_err(fail)
    }
}
