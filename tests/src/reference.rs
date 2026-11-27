//! Blessing path-traced references (M4 4.1b4, ADR-0090): trace a reference at full quality and a
//! second seed for its noise, render the same view with the renderer for the baseline, and write
//! the image and its `.reference.ron`. Test-only (it compares with testkit's FLIP); run through
//! `just bless-reference <name>`, never in CI.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use concordia_api::Engine;
use concordia_app::Project;
use concordia_core::id::SessionId;
use concordia_pathtrace::Traced;
use concordia_testkit::golden::{Golden, GoldenError, RgbaImage, flip_score};
use concordia_testkit::reference::{Blessed, ReferenceFile, SUITE};
use serde_json::{Value, json};

/// Seed offset for the second trace (the noise measurement).
const NOISE_SEED_OFFSET: u32 = 0x9e37;
/// Near and far planes of a reference's free view.
const NEAR: f32 = 0.1;
const FAR: f32 = 1000.0;

/// What blessing measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Blessing {
    /// The written reference image.
    pub image: PathBuf,
    /// Where the reference, the renderer's image and their FLIP map were written for review.
    pub review: String,
    /// The measurements recorded in the reference file.
    pub blessed: Blessed,
    /// Tracing time of the first seed (ms).
    pub elapsed_ms: f64,
}

/// Blesses reference `name` in the golden store at `golden` (`<golden>/m4_reference/`), writing
/// review images under `review_dir`; `ci` refuses before tracing.
pub fn bless(name: &str, golden: &Path, review_dir: &Path, ci: bool) -> Result<Blessing, String> {
    let store = Golden::new(golden.to_path_buf(), review_dir.to_path_buf(), false, ci);
    if store.is_ci() {
        return Err(fail(GoldenError::BlessInCi));
    }
    let mut file = store.reference(name).map_err(fail)?;
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
    let review = store
        .write_comparison(SUITE, name, &reference, &rendered)
        .map_err(fail)?;
    file.blessed = Some(blessed.clone());
    store
        .bless_reference(name, &file, &reference)
        .map_err(fail)?;
    Ok(Blessing {
        image: golden.join(SUITE).join(format!("{name}.png")),
        review,
        blessed,
        elapsed_ms: first.elapsed_ms,
    })
}

fn fail(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn image(traced: &Traced) -> Result<RgbaImage, String> {
    RgbaImage::new(traced.width, traced.height, traced.rgba.clone()).map_err(fail)
}

/// The short hash of HEAD in the repository holding `project`.
fn git_hash(project: &Path) -> Result<String, String> {
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

/// Reference `name`'s view rendered by the renderer with the sun's shadows as `technique` (`csm` or
/// `vsm`; M4 4.7), and the blessed reference image, from the golden store at `golden`.
pub fn render_against(
    name: &str,
    golden: &Path,
    technique: &str,
) -> Result<(RgbaImage, RgbaImage), String> {
    let store = Golden::new(golden.to_path_buf(), golden.join("../review"), false, false);
    let file = store.reference(name).map_err(fail)?;
    let reference = std::fs::read(golden.join(SUITE).join(format!("{name}.png"))).map_err(|e| {
        fail(format!(
            "{name}.png: {e} (bless it with `just bless-reference {name}`)"
        ))
    })?;
    let reference = RgbaImage::from_png(&reference).map_err(fail)?;
    let mut session = Session::open(&file)?;
    session.call("render.set_shadows", json!({"technique": technique}))?;
    Ok((session.screenshot(&file)?, reference))
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
    fn open(file: &ReferenceFile) -> Result<Self, String> {
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
        let generated = loaded.manifest.generators.is_some();
        let mut engine = Engine::new(loaded.engine_config().map_err(fail)?)
            .map_err(|e| fail(format!("{e:?}")))?;
        let session = engine.open_session();
        let mut opened = Self { engine, session };
        opened.call(
            "session.open",
            json!({"client": "bless-reference", "actor": "tool:bless-reference"}),
        )?;
        // Generated meshes aren't in git: remake missing ones from their sidecars' recipes.
        if generated {
            opened.call("gen.restore", json!({"actor": "tool:bless-reference"}))?;
        }
        opened.call("scene.open", json!({"path": scene}))?;
        Ok(opened)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.engine
            .call(self.session, method, params)
            .map_err(|e| fail(format!("{method}: {}", e.message)))
    }

    fn trace(&mut self, file: &ReferenceFile, seed: u32) -> Result<Traced, String> {
        let mut params = json!({"width": file.width, "height": file.height, "spp": file.spp,
            "seed": seed, "max_bounces": file.max_bounces});
        if let Some(view) = view(file) {
            params["view"] = view;
        }
        let params = serde_json::from_value(params).map_err(fail)?;
        concordia_pathtrace::trace(&mut self.engine, &params).map_err(|e| fail(e.message))
    }

    fn screenshot(&mut self, file: &ReferenceFile) -> Result<RgbaImage, String> {
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
